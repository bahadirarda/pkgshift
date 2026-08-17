use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path};

use node_semver::{Range, Version};
use serde_json::{Map, Value};

use crate::model::{DependencyProtocol, Diagnostic, LockGraph, PackageManagerId, ProjectIr};
use crate::util::{PkgshiftError, Result, read_text};

use super::package_name_end;

#[derive(Debug, Clone)]
pub(super) struct YarnPatchConversion {
    pub(super) base_specifier: String,
    pub(super) selector: String,
    pub(super) path: String,
}

fn exact_semver(value: &str) -> bool {
    let (without_build, build) = value
        .split_once('+')
        .map_or((value, None), |(version, build)| (version, Some(build)));
    let (core, prerelease) = without_build
        .split_once('-')
        .map_or((without_build, None), |(version, prerelease)| {
            (version, Some(prerelease))
        });
    let mut components = core.split('.');
    let valid_core = (0..3).all(|_| {
        components.next().is_some_and(|component| {
            !component.is_empty() && component.chars().all(|c| c.is_ascii_digit())
        })
    }) && components.next().is_none();
    let valid_suffix = |suffix: Option<&str>| {
        suffix.is_none_or(|suffix| {
            !suffix.is_empty()
                && suffix.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '.' | '-')
                })
        })
    };
    valid_core && valid_suffix(prerelease) && valid_suffix(build)
}

fn portable_semver_range(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.len() <= 256
        && value
            .chars()
            .any(|character| character.is_ascii_digit() || matches!(character, '*' | 'x' | 'X'))
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || character.is_ascii_whitespace()
                || matches!(
                    character,
                    '.' | '-' | '+' | '*' | '^' | '~' | '<' | '>' | '=' | '|'
                )
        })
        && !value.contains("|||")
        && !value.split("||").any(|branch| branch.trim().is_empty())
        && value.parse::<Range>().is_ok()
}

fn semver_satisfies(version: &str, range: &str) -> bool {
    let Ok(version) = version.parse::<Version>() else {
        return false;
    };
    range
        .parse::<Range>()
        .is_ok_and(|range| version.satisfies(&range))
}

fn package_selector(selector: &str) -> Option<(String, Option<String>)> {
    let name_end = package_name_end(selector)?;
    if name_end == selector.len() {
        return Some((selector.to_owned(), None));
    }
    let name = &selector[..name_end];
    let range = selector[name_end + 1..].trim();
    portable_semver_range(range).then(|| (name.to_owned(), Some(range.to_owned())))
}

fn normalize_patch_path(
    root: &Path,
    raw_path: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Option<String>> {
    let path = raw_path
        .strip_prefix("~/")
        .or_else(|| raw_path.strip_prefix("./"))
        .unwrap_or(raw_path);
    let relative = Path::new(path);
    let safe = !path.is_empty()
        && !path.contains('\\')
        && relative
            .extension()
            .and_then(|extension| extension.to_str())
            == Some("patch")
        && relative
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir));
    if !safe {
        diagnostics.push(Diagnostic::blocking(
            "PATCH_PATH_UNSUPPORTED",
            "A patch path is outside the deterministic project-relative subset.",
            vec![
                "Use a project-relative .patch file without parent-directory segments.".to_owned(),
            ],
        ));
        return Ok(None);
    }
    let mut absolute = root.to_path_buf();
    let components = relative.components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(segment) = component else {
            continue;
        };
        absolute.push(segment);
        match fs::symlink_metadata(&absolute) {
            Ok(metadata)
                if metadata.file_type().is_symlink()
                    || (index + 1 == components.len() && !metadata.is_file())
                    || (index + 1 < components.len() && !metadata.is_dir()) =>
            {
                diagnostics.push(Diagnostic::blocking(
                    "PATCH_PATH_UNSUPPORTED",
                    "A patch path traverses a symbolic link or non-file project entry.",
                    vec![
                        "Use a regular patch file beneath regular project directories.".to_owned(),
                    ],
                ));
                return Ok(None);
            }
            Ok(_) => {}
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                diagnostics.push(Diagnostic::blocking(
                    "PATCH_FILE_NOT_FOUND",
                    "A configured patch file does not exist in the project.",
                    vec![
                        "Restore the configured patch file before retrying the migration."
                            .to_owned(),
                    ],
                ));
                return Ok(None);
            }
            Err(source) => {
                return Err(PkgshiftError::Io {
                    path: absolute,
                    source,
                });
            }
        }
    }
    let Some(content) = read_text(&root.join(relative))? else {
        diagnostics.push(Diagnostic::blocking(
            "PATCH_FILE_NOT_FOUND",
            "A configured patch file does not exist in the project.",
            vec!["Restore the configured patch file before retrying the migration.".to_owned()],
        ));
        return Ok(None);
    };
    let has_old_header = content.lines().any(|line| line.starts_with("--- "));
    let has_new_header = content.lines().any(|line| line.starts_with("+++ "));
    let has_hunk = content.lines().any(|line| line.starts_with("@@ "));
    if !has_old_header
        || !has_new_header
        || !has_hunk
        || content.contains('\0')
        || content.contains("GIT binary patch")
        || content.contains("Binary files ")
    {
        diagnostics.push(Diagnostic::blocking(
            "PATCH_FORMAT_UNSUPPORTED",
            "A patch file is outside the portable text unified-diff subset.",
            vec![
                "Regenerate the patch as a text unified diff with file and hunk headers."
                    .to_owned(),
            ],
        ));
        return Ok(None);
    }
    Ok(Some(path.replace('\\', "/")))
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let encoded = std::str::from_utf8(bytes.get(index + 1..index + 3)?).ok()?;
            output.push(u8::from_str_radix(encoded, 16).ok()?);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).ok()
}

fn percent_encode(value: &str) -> String {
    let mut output = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'~') {
            output.push(char::from(byte));
        } else {
            use std::fmt::Write;
            write!(&mut output, "%{byte:02X}").expect("writing to a string cannot fail");
        }
    }
    output
}

pub(super) fn yarn_patch_conversion(
    root: &Path,
    name: &str,
    specifier: &str,
    target: PackageManagerId,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Option<YarnPatchConversion>> {
    let Some(value) = specifier.strip_prefix("patch:") else {
        return Ok(None);
    };
    let Some((source, raw_path)) = value.split_once('#') else {
        diagnostics.push(Diagnostic::blocking(
            "PATCH_LOCATOR_UNSUPPORTED",
            "A Yarn patch locator does not include one local patch file.",
            vec!["Regenerate the patch with Yarn patch-commit --save.".to_owned()],
        ));
        return Ok(None);
    };
    if raw_path.contains(['&', '!', '%']) || raw_path.contains("::") {
        diagnostics.push(Diagnostic::blocking(
            "PATCH_LOCATOR_UNSUPPORTED",
            "A Yarn patch locator uses multiple, optional, encoded, or parameterized patch sources.",
            vec!["Reduce the locator to one required project-relative patch file.".to_owned()],
        ));
        return Ok(None);
    }
    let Some(reference) = source.strip_prefix(&format!("{name}@")) else {
        diagnostics.push(Diagnostic::blocking(
            "PATCH_LOCATOR_UNSUPPORTED",
            "A Yarn patch locator aliases a different package identity.",
            vec![
                "Use a patch locator whose package identity matches the dependency key.".to_owned(),
            ],
        ));
        return Ok(None);
    };
    let encoded = reference
        .strip_prefix("npm%3A")
        .or_else(|| reference.strip_prefix("npm%3a"))
        .or_else(|| reference.strip_prefix("npm:"))
        .unwrap_or(reference);
    let Some(version) = percent_decode(encoded) else {
        diagnostics.push(Diagnostic::blocking(
            "PATCH_SELECTOR_UNSUPPORTED",
            "A Yarn patch source contains invalid percent encoding.",
            vec!["Regenerate the patch locator with Yarn.".to_owned()],
        ));
        return Ok(None);
    };
    if !portable_semver_range(&version)
        || (target == PackageManagerId::Bun && !exact_semver(&version))
    {
        diagnostics.push(Diagnostic::blocking(
            "PATCH_SELECTOR_UNSUPPORTED",
            if target == PackageManagerId::Bun {
                "Bun patch conversion requires one exact package version."
            } else {
                "A Yarn patch source is not a portable registry semver selector."
            },
            vec![if target == PackageManagerId::Bun {
                "Resolve the patch to an exact package version before targeting Bun.".to_owned()
            } else {
                "Use an exact version, semver range, wildcard, or comparator set.".to_owned()
            }],
        ));
        return Ok(None);
    }
    let Some(path) = normalize_patch_path(root, raw_path, diagnostics)? else {
        return Ok(None);
    };
    Ok(Some(YarnPatchConversion {
        base_specifier: version.clone(),
        selector: format!("{name}@{version}"),
        path,
    }))
}

pub(super) fn yarn_patch_name(specifier: &str) -> Option<&str> {
    let source = specifier.strip_prefix("patch:")?.split_once('#')?.0;
    let name_end = package_name_end(source)?;
    (name_end < source.len()).then_some(&source[..name_end])
}

pub(super) fn source_patched_dependencies(
    source: PackageManagerId,
    root_manifest: &Map<String, Value>,
    pnpm_manifest: &Map<String, Value>,
    pnpm_configuration: &Map<String, Value>,
) -> Map<String, Value> {
    let root = root_manifest.get("patchedDependencies");
    let pnpm_manifest = pnpm_manifest.get("patchedDependencies");
    let pnpm_configuration = pnpm_configuration.get("patchedDependencies");
    let candidates = match source {
        PackageManagerId::Pnpm => [pnpm_configuration, pnpm_manifest, root],
        _ => [root, pnpm_configuration, pnpm_manifest],
    };
    candidates
        .into_iter()
        .filter_map(|candidate| candidate.and_then(Value::as_object))
        .find(|entries| !entries.is_empty())
        .cloned()
        .unwrap_or_default()
}

pub(super) fn validated_patched_dependencies(
    root: &Path,
    entries: &Map<String, Value>,
    target: PackageManagerId,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Map<String, Value>> {
    let mut output = Map::new();
    for (selector, value) in entries {
        let Some((_, range)) = package_selector(selector) else {
            diagnostics.push(Diagnostic::blocking(
                "PATCH_SELECTOR_UNSUPPORTED",
                "A patched dependency has an invalid package or semver selector.",
                vec!["Use a package name with an optional portable semver selector.".to_owned()],
            ));
            continue;
        };
        if target == PackageManagerId::Bun && !range.as_deref().is_some_and(exact_semver) {
            diagnostics.push(Diagnostic::blocking(
                "PATCH_SELECTOR_UNSUPPORTED",
                "Bun patch conversion requires one exact package version.",
                vec![
                    "Resolve the patch to an exact package version before targeting Bun."
                        .to_owned(),
                ],
            ));
            continue;
        }
        let Some(raw_path) = value.as_str() else {
            diagnostics.push(Diagnostic::blocking(
                "PATCH_POLICY_UNSUPPORTED",
                "A patched dependency value is not a patch file path.",
                vec!["Use the current selector-to-path patchedDependencies shape.".to_owned()],
            ));
            continue;
        };
        if let Some(path) = normalize_patch_path(root, raw_path, diagnostics)? {
            output.insert(selector.clone(), Value::String(path));
        }
    }
    Ok(output)
}

fn observed_exact_versions(
    project_ir: &ProjectIr,
    source_lock_graph: Option<&LockGraph>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut versions = BTreeMap::<String, BTreeSet<String>>::new();
    for dependency in project_ir
        .packages
        .iter()
        .flat_map(|package| &package.dependencies)
        .filter(|dependency| dependency.protocol == DependencyProtocol::Semver)
        .filter(|dependency| exact_semver(&dependency.specifier))
    {
        versions
            .entry(dependency.name.clone())
            .or_default()
            .insert(dependency.specifier.clone());
    }
    if let Some(graph) = source_lock_graph {
        for node in &graph.nodes {
            if exact_semver(&node.version) {
                versions
                    .entry(node.name.clone())
                    .or_default()
                    .insert(node.version.clone());
            }
        }
    }
    versions
}

#[derive(Debug)]
struct SelectedPatch<'a> {
    path: &'a str,
    specificity: u8,
}

pub(super) fn yarn_patch_resolutions(
    patched_dependencies: &Map<String, Value>,
    project_ir: &ProjectIr,
    source_lock_graph: Option<&LockGraph>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Map<String, Value> {
    let observed_versions = observed_exact_versions(project_ir, source_lock_graph);
    let mut selected = BTreeMap::<(String, String), SelectedPatch<'_>>::new();
    for (selector, path) in patched_dependencies {
        let Some((name, range)) = package_selector(selector) else {
            continue;
        };
        let Some(path) = path.as_str() else {
            continue;
        };
        let (specificity, candidates) = match range.as_deref() {
            Some(range) if exact_semver(range) => (2, vec![range.to_owned()]),
            Some(range) => (
                1,
                observed_versions
                    .get(&name)
                    .into_iter()
                    .flatten()
                    .filter(|version| semver_satisfies(version, range))
                    .cloned()
                    .collect::<Vec<_>>(),
            ),
            None => (
                0,
                observed_versions
                    .get(&name)
                    .into_iter()
                    .flatten()
                    .cloned()
                    .collect::<Vec<_>>(),
            ),
        };
        if candidates.is_empty() {
            diagnostics.push(Diagnostic::blocking(
                "PATCH_RESOLUTION_EVIDENCE_MISSING",
                format!(
                    "The patch selector {selector} cannot be expanded to an exact Yarn locator."
                ),
                vec![
                    "Retain a source lockfile or use an exact registry version in a project dependency."
                        .to_owned(),
                ],
            ));
            continue;
        }
        for version in candidates {
            let key = (name.clone(), version);
            match selected.get(&key) {
                Some(existing) if existing.specificity == specificity && existing.path != path => {
                    diagnostics.push(Diagnostic::blocking(
                        "PATCH_POLICY_CONFLICT",
                        format!(
                            "Equal-priority patch selectors map {}@{} to different files.",
                            key.0, key.1
                        ),
                        vec!["Keep one patch file for each exact package resolution.".to_owned()],
                    ));
                }
                Some(existing) if existing.specificity >= specificity => {}
                _ => {
                    selected.insert(key, SelectedPatch { path, specificity });
                }
            }
        }
    }
    selected
        .into_iter()
        .map(|((name, version), patch)| {
            (
                format!("{name}@npm:{version}"),
                Value::String(format!(
                    "patch:{name}@npm%3A{}#~/{path}",
                    percent_encode(&version),
                    path = patch.path
                )),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_portable_ranges_without_accepting_protocols() {
        assert!(portable_semver_range("^2.0.0"));
        assert!(portable_semver_range(">=2.0.0 <3 || 4.x"));
        assert!(!portable_semver_range("workspace:*"));
        assert!(!portable_semver_range("https://example.com/archive.tgz"));
    }

    #[test]
    fn round_trips_yarn_range_encoding() {
        let value = "^2.0.0 || >=3.1.0 <4";
        assert_eq!(
            percent_decode(&percent_encode(value)).as_deref(),
            Some(value)
        );
    }
}
