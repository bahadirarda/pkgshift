use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::catalog::get_package_manager;
use crate::model::{
    Diagnostic, DiagnosticSeverity, EvidenceDetail, LockGraph, LockGraphEdge, LockGraphNode,
    PackageManagerId, SCHEMA_VERSION,
};
use crate::util::{PkgshiftError, Result, digest_bytes, short_digest};
use crate::verification_policy::PackagePlatformConstraint;

fn graph_diagnostic(
    code: &str,
    summary: impl Into<String>,
    lockfile_path: &str,
    blocking: bool,
    remediation: Vec<String>,
) -> Diagnostic {
    Diagnostic {
        code: code.to_owned(),
        severity: if blocking {
            DiagnosticSeverity::Error
        } else {
            DiagnosticSeverity::Warning
        },
        summary: summary.into(),
        blocking,
        evidence: vec![EvidenceDetail {
            location: lockfile_path.to_owned(),
            detail: "package manager lockfile".to_owned(),
        }],
        remediation,
    }
}

fn json_from_yaml(content: &str) -> std::result::Result<Value, String> {
    noyalib::from_str::<Value>(content).map_err(|error| error.to_string())
}

fn object<'a>(value: &'a Value, key: &str) -> Option<&'a serde_json::Map<String, Value>> {
    value.get(key).and_then(Value::as_object)
}

fn string_list(value: &Value, key: &str) -> Vec<String> {
    let mut values = value.get(key).map_or_else(Vec::new, |entry| {
        entry.as_array().map_or_else(
            || {
                entry
                    .as_str()
                    .map(str::to_ascii_lowercase)
                    .into_iter()
                    .collect()
            },
            |values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_ascii_lowercase)
                    .collect()
            },
        )
    });
    values.sort();
    values.dedup();
    values
}

fn platform_constraint(value: &Value) -> PackagePlatformConstraint {
    PackagePlatformConstraint {
        os: string_list(value, "os"),
        cpu: string_list(value, "cpu"),
        libc: string_list(value, "libc"),
    }
}

fn has_lockfile_version(value: &Value) -> bool {
    value
        .get("lockfileVersion")
        .is_some_and(|version| version.is_number() || version.is_string())
}

fn dependency_edges(value: &Value, parent: &str, edges: &mut Vec<LockGraphEdge>) {
    for (section, kind) in [
        ("dependencies", "dependency"),
        ("optionalDependencies", "optional"),
        ("peerDependencies", "peer"),
    ] {
        let Some(dependencies) = object(value, section) else {
            continue;
        };
        for name in dependencies.keys() {
            edges.push(LockGraphEdge {
                from: parent.to_owned(),
                dependency: name.clone(),
                kind: kind.to_owned(),
                target: None,
            });
        }
    }
}

pub(super) fn split_locator(locator: &str) -> Option<(String, String)> {
    let locator = locator
        .trim()
        .trim_matches(|character| character == '\'' || character == '"')
        .trim_start_matches('/');
    if locator.is_empty()
        || locator.contains("workspace:")
        || locator.contains("file:")
        || locator.contains("link:")
    {
        return None;
    }
    let separator = if locator.starts_with('@') {
        let slash = locator.find('/')?;
        locator[slash + 1..]
            .find('@')
            .map(|index| slash + 1 + index)?
    } else {
        locator.rfind('@')?
    };
    if separator == 0 || separator + 1 >= locator.len() {
        return None;
    }
    let name = locator[..separator].to_owned();
    let raw_version = &locator[separator + 1..];
    let version = raw_version
        .strip_prefix("npm:")
        .unwrap_or(raw_version)
        .split('(')
        .next()
        .unwrap_or(raw_version)
        .trim_matches(|character| character == '\'' || character == '"')
        .to_owned();
    if name.is_empty()
        || version.is_empty()
        || version.starts_with("git+")
        || version.starts_with("http:")
        || version.starts_with("https:")
    {
        return None;
    }
    Some((name, version))
}

fn is_local_pnpm_locator(locator: &str) -> bool {
    ["file:", "link:", "workspace:"]
        .iter()
        .any(|protocol| locator.starts_with(protocol) || locator.contains(&format!("@{protocol}")))
}

fn npm_name_from_path(path: &str) -> Option<String> {
    let marker = "node_modules/";
    let index = path.rfind(marker)?;
    let remainder = &path[index + marker.len()..];
    let mut segments = remainder.split('/');
    let first = segments.next()?;
    if first.starts_with('@') {
        Some(format!("{first}/{}", segments.next()?))
    } else {
        Some(first.to_owned())
    }
}

fn parse_npm(value: &Value) -> (Vec<LockGraphNode>, Vec<LockGraphEdge>) {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let Some(packages) = object(value, "packages") else {
        return (nodes, edges);
    };
    for (path, metadata) in packages {
        if path.is_empty()
            || metadata
                .get("link")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        {
            continue;
        }
        let Some(path_name) = npm_name_from_path(path) else {
            continue;
        };
        let Some(version) = metadata.get("version").and_then(Value::as_str) else {
            continue;
        };
        let name = metadata
            .get("name")
            .and_then(Value::as_str)
            .map_or(path_name, str::to_owned);
        let parent = format!("{name}@{version}");
        nodes.push(LockGraphNode {
            locator: path.clone(),
            name,
            version: version.to_owned(),
            integrity: metadata
                .get("integrity")
                .and_then(Value::as_str)
                .map(str::to_owned),
            platform: platform_constraint(metadata),
        });
        dependency_edges(metadata, &parent, &mut edges);
    }
    (nodes, edges)
}

fn parse_npm_checked(
    value: &Value,
) -> std::result::Result<(Vec<LockGraphNode>, Vec<LockGraphEdge>), String> {
    if !has_lockfile_version(value) {
        return Err("missing lockfileVersion".to_owned());
    }
    let packages = object(value, "packages")
        .ok_or_else(|| "package-lock files without a packages map are not supported".to_owned())?;
    for (path, metadata) in packages {
        if path.is_empty()
            || npm_name_from_path(path).is_none()
            || metadata.get("link").and_then(Value::as_bool) == Some(true)
        {
            continue;
        }
        if !metadata.is_object() || metadata.get("version").and_then(Value::as_str).is_none() {
            return Err(format!("unsupported package entry at {path}"));
        }
    }
    Ok(parse_npm(value))
}

fn parse_pnpm(value: &Value) -> (Vec<LockGraphNode>, Vec<LockGraphEdge>) {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let Some(packages) = object(value, "packages") else {
        return (nodes, edges);
    };
    let snapshots = object(value, "snapshots");
    for (locator, metadata) in packages {
        let Some((name, version)) = split_locator(locator) else {
            continue;
        };
        let parent = format!("{name}@{version}");
        nodes.push(LockGraphNode {
            locator: locator.clone(),
            name,
            version,
            integrity: metadata
                .get("resolution")
                .and_then(Value::as_object)
                .and_then(|resolution| resolution.get("integrity"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            platform: platform_constraint(metadata),
        });
        let dependency_source = snapshots
            .and_then(|entries| entries.get(locator))
            .unwrap_or(metadata);
        dependency_edges(dependency_source, &parent, &mut edges);
    }
    (nodes, edges)
}

fn parse_pnpm_checked(
    value: &Value,
) -> std::result::Result<(Vec<LockGraphNode>, Vec<LockGraphEdge>), String> {
    if !has_lockfile_version(value) {
        return Err("missing lockfileVersion".to_owned());
    }
    if let Some(packages) = value.get("packages") {
        let packages = packages
            .as_object()
            .ok_or_else(|| "packages must be a map".to_owned())?;
        for locator in packages.keys() {
            if split_locator(locator).is_none() && !is_local_pnpm_locator(locator) {
                return Err(format!("unsupported pnpm package locator {locator}"));
            }
        }
    }
    if value
        .get("snapshots")
        .is_some_and(|snapshots| !snapshots.is_object())
    {
        return Err("snapshots must be a map".to_owned());
    }
    Ok(parse_pnpm(value))
}

fn parse_scalar(line: &str, field: &str) -> Option<String> {
    line.strip_prefix(field)
        .map(str::trim)
        .map(|value| {
            value
                .trim_matches(|character| character == '\'' || character == '"')
                .to_owned()
        })
        .filter(|value| !value.is_empty())
}

#[derive(Default)]
struct YarnClassicEntry {
    locator: String,
    name: Option<String>,
    version: Option<String>,
    integrity: Option<String>,
    edges: Vec<(String, String)>,
}

fn flush_yarn_classic(
    entry: &mut YarnClassicEntry,
    nodes: &mut Vec<LockGraphNode>,
    edges: &mut Vec<LockGraphEdge>,
) {
    let (Some(name), Some(version)) = (entry.name.take(), entry.version.take()) else {
        *entry = YarnClassicEntry::default();
        return;
    };
    let parent = format!("{name}@{version}");
    nodes.push(LockGraphNode {
        locator: std::mem::take(&mut entry.locator),
        name,
        version,
        integrity: entry.integrity.take(),
        platform: PackagePlatformConstraint::default(),
    });
    edges.extend(
        entry
            .edges
            .drain(..)
            .map(|(dependency, kind)| LockGraphEdge {
                from: parent.clone(),
                dependency,
                kind,
                target: None,
            }),
    );
}

fn parse_yarn_classic(content: &str) -> (Vec<LockGraphNode>, Vec<LockGraphEdge>) {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut entry = YarnClassicEntry::default();
    let mut dependency_kind = None;
    for line in content.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim();
        if indent == 0 && trimmed.ends_with(':') {
            flush_yarn_classic(&mut entry, &mut nodes, &mut edges);
            let locator = trimmed.trim_end_matches(':').trim().to_owned();
            let first_selector = locator
                .trim_matches('"')
                .split(", ")
                .next()
                .unwrap_or_default();
            let name = split_locator(first_selector)
                .map(|(name, _)| name)
                .or_else(|| {
                    let separator = if first_selector.starts_with('@') {
                        let slash = first_selector.find('/')?;
                        first_selector[slash + 1..]
                            .find('@')
                            .map(|index| slash + 1 + index)?
                    } else {
                        first_selector.rfind('@')?
                    };
                    Some(first_selector[..separator].to_owned())
                });
            entry.locator = locator;
            entry.name = name;
            dependency_kind = None;
        } else if indent == 2 {
            dependency_kind = match trimmed {
                "dependencies:" => Some("dependency"),
                "optionalDependencies:" => Some("optional"),
                "peerDependencies:" => Some("peer"),
                _ => None,
            };
            if let Some(version) = parse_scalar(trimmed, "version") {
                entry.version = Some(version);
            } else if let Some(integrity) = parse_scalar(trimmed, "integrity") {
                entry.integrity = Some(integrity);
            }
        } else if indent >= 4
            && let Some(kind) = dependency_kind
            && let Some(name) = trimmed.split_whitespace().next()
        {
            entry
                .edges
                .push((name.trim_matches('"').to_owned(), kind.to_owned()));
        }
    }
    flush_yarn_classic(&mut entry, &mut nodes, &mut edges);
    (nodes, edges)
}

fn parse_yarn_classic_checked(
    content: &str,
) -> std::result::Result<(Vec<LockGraphNode>, Vec<LockGraphEdge>), String> {
    if !content
        .lines()
        .any(|line| line.trim() == "# yarn lockfile v1")
    {
        return Err("missing Yarn Classic lockfile header".to_owned());
    }
    Ok(parse_yarn_classic(content))
}

fn parse_yarn_modern(value: &Value) -> (Vec<LockGraphNode>, Vec<LockGraphEdge>) {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let Some(entries) = value.as_object() else {
        return (nodes, edges);
    };
    for (locator, metadata) in entries {
        if locator == "__metadata" {
            continue;
        }
        let Some(version) = metadata.get("version").and_then(Value::as_str) else {
            continue;
        };
        let resolution = metadata
            .get("resolution")
            .and_then(Value::as_str)
            .unwrap_or(locator);
        let Some((name, _)) = split_locator(resolution) else {
            continue;
        };
        let parent = format!("{name}@{version}");
        nodes.push(LockGraphNode {
            locator: locator.clone(),
            name,
            version: version.to_owned(),
            integrity: metadata
                .get("checksum")
                .and_then(Value::as_str)
                .map(|checksum| format!("yarn:{checksum}")),
            platform: platform_constraint(metadata),
        });
        dependency_edges(metadata, &parent, &mut edges);
    }
    (nodes, edges)
}

fn parse_yarn_modern_checked(
    value: &Value,
) -> std::result::Result<(Vec<LockGraphNode>, Vec<LockGraphEdge>), String> {
    let entries = value
        .as_object()
        .ok_or_else(|| "lockfile root must be a map".to_owned())?;
    if !entries.get("__metadata").is_some_and(Value::is_object) {
        return Err("missing Yarn Modern __metadata".to_owned());
    }
    for (locator, metadata) in entries {
        if locator == "__metadata" {
            continue;
        }
        let metadata = metadata
            .as_object()
            .ok_or_else(|| format!("unsupported Yarn entry {locator}"))?;
        if metadata.get("version").and_then(Value::as_str).is_none() {
            return Err(format!("missing version for Yarn entry {locator}"));
        }
        let resolution = metadata
            .get("resolution")
            .and_then(Value::as_str)
            .unwrap_or(locator);
        if split_locator(resolution).is_none()
            && !resolution.contains("workspace:")
            && !resolution.contains("file:")
            && !resolution.contains("link:")
        {
            return Err(format!("unsupported Yarn resolution {resolution}"));
        }
    }
    Ok(parse_yarn_modern(value))
}

fn bun_dependency_target(
    parent_locator: &str,
    dependency: &str,
    locator_resolutions: &BTreeMap<String, String>,
) -> Option<String> {
    let nested = format!("{parent_locator}/{dependency}");
    if let Some(target) = locator_resolutions.get(&nested) {
        return Some(target.clone());
    }
    let mut ancestors = locator_resolutions
        .keys()
        .filter(|candidate| parent_locator.starts_with(&format!("{candidate}/")))
        .collect::<Vec<_>>();
    ancestors.sort_by_key(|candidate| std::cmp::Reverse(candidate.len()));
    for ancestor in ancestors {
        let candidate = format!("{ancestor}/{dependency}");
        if let Some(target) = locator_resolutions.get(&candidate) {
            return Some(target.clone());
        }
    }
    locator_resolutions.get(dependency).cloned()
}

fn parse_bun(value: &Value) -> (Vec<LockGraphNode>, Vec<LockGraphEdge>) {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let Some(packages) = object(value, "packages") else {
        return (nodes, edges);
    };
    let locator_resolutions = packages
        .iter()
        .filter_map(|(locator, entry)| {
            entry
                .as_array()
                .and_then(|values| values.first())
                .and_then(Value::as_str)
                .and_then(split_locator)
                .map(|(name, version)| (locator.clone(), format!("{name}@{version}")))
        })
        .collect::<BTreeMap<_, _>>();
    for (locator, entry) in packages {
        let Some(values) = entry.as_array() else {
            continue;
        };
        let Some(resolution) = values.first().and_then(Value::as_str) else {
            continue;
        };
        let Some((name, version)) = split_locator(resolution) else {
            continue;
        };
        let parent = format!("{name}@{version}");
        nodes.push(LockGraphNode {
            locator: locator.clone(),
            name,
            version,
            integrity: values.get(3).and_then(Value::as_str).map(str::to_owned),
            platform: values
                .get(2)
                .map_or_else(PackagePlatformConstraint::default, platform_constraint),
        });
        if let Some(metadata) = values.get(2) {
            for (section, kind) in [
                ("dependencies", "dependency"),
                ("optionalDependencies", "optional"),
                ("peerDependencies", "peer"),
            ] {
                let Some(dependencies) = object(metadata, section) else {
                    continue;
                };
                for dependency in dependencies.keys() {
                    edges.push(LockGraphEdge {
                        from: parent.clone(),
                        dependency: dependency.clone(),
                        kind: kind.to_owned(),
                        target: bun_dependency_target(locator, dependency, &locator_resolutions),
                    });
                }
            }
        }
    }
    (nodes, edges)
}

fn parse_bun_checked(
    value: &Value,
) -> std::result::Result<(Vec<LockGraphNode>, Vec<LockGraphEdge>), String> {
    if !has_lockfile_version(value) {
        return Err("missing lockfileVersion".to_owned());
    }
    let packages = object(value, "packages").ok_or_else(|| "missing packages map".to_owned())?;
    for (locator, entry) in packages {
        let values = entry
            .as_array()
            .ok_or_else(|| format!("unsupported Bun package entry {locator}"))?;
        let resolution = values
            .first()
            .and_then(Value::as_str)
            .ok_or_else(|| format!("missing Bun resolution for {locator}"))?;
        if split_locator(resolution).is_none()
            && !resolution.contains("workspace:")
            && !resolution.contains("file:")
            && !resolution.contains("link:")
        {
            return Err(format!("unsupported Bun resolution {resolution}"));
        }
    }
    Ok(parse_bun(value))
}

fn parse_deno_entry(
    locator: &str,
    metadata: &Value,
    nodes: &mut Vec<LockGraphNode>,
    edges: &mut Vec<LockGraphEdge>,
) -> std::result::Result<(), String> {
    let Some((name, version)) = split_deno_locator(locator) else {
        return Err(format!("unsupported Deno package locator {locator}"));
    };
    let metadata = metadata
        .as_object()
        .ok_or_else(|| format!("unsupported Deno package entry {locator}"))?;
    let parent = format!("{name}@{version}");
    nodes.push(LockGraphNode {
        locator: locator.to_owned(),
        name,
        version,
        integrity: metadata
            .get("integrity")
            .and_then(Value::as_str)
            .map(str::to_owned),
        platform: platform_constraint(&Value::Object(metadata.clone())),
    });
    for (section, kind) in [
        ("dependencies", "dependency"),
        ("optionalDependencies", "optional"),
    ] {
        let Some(dependencies) = metadata.get(section) else {
            continue;
        };
        if let Some(dependencies) = dependencies.as_array() {
            edges.extend(
                dependencies
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|dependency| LockGraphEdge {
                        from: parent.clone(),
                        dependency: split_deno_locator(dependency)
                            .map_or_else(|| dependency.to_owned(), |(name, _)| name),
                        kind: kind.to_owned(),
                        target: split_deno_locator(dependency)
                            .map(|(name, version)| format!("{name}@{version}")),
                    }),
            );
        } else if let Some(dependencies) = dependencies.as_object() {
            edges.extend(dependencies.keys().map(|dependency| LockGraphEdge {
                from: parent.clone(),
                dependency: dependency.clone(),
                kind: kind.to_owned(),
                target: None,
            }));
        } else {
            return Err(format!("unsupported Deno {section} entry for {locator}"));
        }
    }
    Ok(())
}

fn split_deno_locator(locator: &str) -> Option<(String, String)> {
    let locator = locator
        .trim()
        .trim_matches(|character| character == '\'' || character == '"');
    let locator = locator
        .strip_prefix("npm:")
        .or_else(|| locator.strip_prefix("jsr:"))
        .unwrap_or(locator);
    let separator = if locator.starts_with('@') {
        let slash = locator.find('/')?;
        locator[slash + 1..]
            .find('@')
            .map(|index| slash + 1 + index)?
    } else {
        locator.find('@')?
    };
    let name = locator[..separator].to_owned();
    let version = locator[separator + 1..]
        .split('_')
        .next()
        .unwrap_or_default()
        .to_owned();
    (!name.is_empty() && !version.is_empty()).then_some((name, version))
}

fn parse_deno_checked(
    value: &Value,
) -> std::result::Result<(Vec<LockGraphNode>, Vec<LockGraphEdge>), String> {
    if !value
        .get("version")
        .is_some_and(|version| version.is_number() || version.is_string())
    {
        return Err("missing Deno lock version".to_owned());
    }
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    if let Some(npm) = value.get("npm") {
        let npm = npm
            .as_object()
            .ok_or_else(|| "Deno npm entries must be a map".to_owned())?;
        for (locator, metadata) in npm {
            parse_deno_entry(locator, metadata, &mut nodes, &mut edges)?;
        }
    }
    if let Some(jsr) = object(value, "jsr") {
        for (locator, metadata) in jsr {
            parse_deno_entry(locator, metadata, &mut nodes, &mut edges)?;
        }
    }
    Ok((nodes, edges))
}

fn vlt_locator_version(locator: &str) -> Option<String> {
    let separator = locator.rfind('@')?;
    let version = locator[separator + 1..].split('~').next()?.trim();
    (!version.is_empty()).then(|| version.to_owned())
}

fn parse_vlt_checked(
    value: &Value,
) -> std::result::Result<(Vec<LockGraphNode>, Vec<LockGraphEdge>), String> {
    if !has_lockfile_version(value) {
        return Err("missing vlt lockfileVersion".to_owned());
    }
    let entries = object(value, "nodes").ok_or_else(|| "missing vlt nodes map".to_owned())?;
    let mut nodes = Vec::new();
    for (locator, entry) in entries {
        let Some(values) = entry.as_array() else {
            return Err(format!("unsupported vlt node {locator}"));
        };
        let Some(name) = values.get(1).and_then(Value::as_str) else {
            continue;
        };
        let Some(version) = vlt_locator_version(locator) else {
            continue;
        };
        nodes.push(LockGraphNode {
            locator: locator.clone(),
            name: name.to_owned(),
            version,
            integrity: values.get(2).and_then(Value::as_str).map(str::to_owned),
            platform: PackagePlatformConstraint::default(),
        });
    }
    Ok((nodes, Vec::new()))
}

fn incomplete_graph(
    manager: PackageManagerId,
    path: &str,
    digest: String,
    format: &str,
    diagnostic: Diagnostic,
) -> Result<LockGraph> {
    let graph_id = short_digest(
        "lockgraph_",
        &(SCHEMA_VERSION, manager, path, &digest, format, false),
    )?;
    Ok(LockGraph {
        schema_version: SCHEMA_VERSION.to_owned(),
        graph_id,
        manager,
        lockfile_path: path.to_owned(),
        lockfile_digest: digest,
        format: format.to_owned(),
        complete: false,
        nodes: Vec::new(),
        edges: Vec::new(),
        diagnostics: vec![diagnostic],
    })
}

pub fn extract_lock_graph(root: &Path, manager: PackageManagerId) -> Result<Option<LockGraph>> {
    let definition = get_package_manager(manager);
    let Some(relative) = definition
        .lockfiles
        .iter()
        .find(|path| root.join(path).is_file())
        .copied()
    else {
        return Ok(None);
    };
    let absolute = root.join(relative);
    let bytes = fs::read(&absolute).map_err(|source| PkgshiftError::Io {
        path: absolute.clone(),
        source,
    })?;
    let digest = digest_bytes(&bytes);
    if relative == "bun.lockb" {
        return incomplete_graph(
            manager,
            relative,
            digest,
            "bun-binary",
            graph_diagnostic(
                "LOCK_GRAPH_FORMAT_UNSUPPORTED",
                "Binary bun.lockb dependency graphs cannot be proven safely.",
                relative,
                true,
                vec![
                    "Convert bun.lockb to the current text bun.lock format before planning."
                        .to_owned(),
                ],
            ),
        )
        .map(Some);
    }
    let content = match std::str::from_utf8(&bytes) {
        Ok(content) => content,
        Err(error) => {
            return incomplete_graph(
                manager,
                relative,
                digest,
                "unknown",
                graph_diagnostic(
                    "LOCK_GRAPH_ENCODING_INVALID",
                    format!("The source lockfile is not UTF-8: {error}"),
                    relative,
                    true,
                    vec!["Regenerate the lockfile with its owning package manager.".to_owned()],
                ),
            )
            .map(Some);
        }
    };

    let parsed = match manager {
        PackageManagerId::Npm => serde_json::from_str::<Value>(content)
            .map_err(|error| error.to_string())
            .and_then(|value| parse_npm_checked(&value))
            .map(|graph| ("npm-package-lock", graph)),
        PackageManagerId::Pnpm => json_from_yaml(content)
            .and_then(|value| parse_pnpm_checked(&value))
            .map(|graph| ("pnpm-lock", graph)),
        PackageManagerId::YarnClassic => {
            parse_yarn_classic_checked(content).map(|graph| ("yarn-classic-lock", graph))
        }
        PackageManagerId::YarnModern => json_from_yaml(content)
            .and_then(|value| parse_yarn_modern_checked(&value))
            .map(|graph| ("yarn-modern-lock", graph)),
        PackageManagerId::Bun => json5::from_str::<Value>(content)
            .map_err(|error| error.to_string())
            .and_then(|value| parse_bun_checked(&value))
            .map(|graph| ("bun-text-lock", graph)),
        PackageManagerId::Vlt => serde_json::from_str::<Value>(content)
            .map_err(|error| error.to_string())
            .and_then(|value| parse_vlt_checked(&value))
            .map(|graph| ("vlt-lock-v1", graph)),
        PackageManagerId::Deno => serde_json::from_str::<Value>(content)
            .map_err(|error| error.to_string())
            .and_then(|value| parse_deno_checked(&value))
            .map(|graph| ("deno-lock-v5", graph)),
    };
    let (format, (mut nodes, mut edges)) = match parsed {
        Ok(parsed) => parsed,
        Err(error) => {
            return incomplete_graph(
                manager,
                relative,
                digest,
                "unknown",
                graph_diagnostic(
                    "LOCK_GRAPH_PARSE_FAILED",
                    format!("The source lockfile graph could not be parsed: {error}"),
                    relative,
                    true,
                    vec!["Regenerate the lockfile with its pinned package manager.".to_owned()],
                ),
            )
            .map(Some);
        }
    };
    nodes.sort_by(|left, right| {
        (&left.name, &left.version, &left.locator).cmp(&(
            &right.name,
            &right.version,
            &right.locator,
        ))
    });
    edges.sort();
    edges.dedup();
    let graph_id = short_digest(
        "lockgraph_",
        &(
            SCHEMA_VERSION,
            manager,
            relative,
            &digest,
            format,
            &nodes,
            &edges,
        ),
    )?;
    Ok(Some(LockGraph {
        schema_version: SCHEMA_VERSION.to_owned(),
        graph_id,
        manager,
        lockfile_path: relative.to_owned(),
        lockfile_digest: digest,
        format: format.to_owned(),
        complete: true,
        nodes,
        edges,
        diagnostics: Vec::new(),
    }))
}
