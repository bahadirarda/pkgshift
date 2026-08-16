use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::catalog::get_package_manager;
use crate::model::{
    Diagnostic, DiagnosticSeverity, EvidenceDetail, LockGraph, LockGraphComparison, LockGraphEdge,
    LockGraphNode, PackageManagerId, SCHEMA_VERSION, VerificationStatus,
};
use crate::util::{PkgshiftError, Result, digest_bytes, short_digest};

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
            });
        }
    }
}

fn split_locator(locator: &str) -> Option<(String, String)> {
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
            if split_locator(locator).is_none() {
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
    });
    edges.extend(
        entry
            .edges
            .drain(..)
            .map(|(dependency, kind)| LockGraphEdge {
                from: parent.clone(),
                dependency,
                kind,
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

fn parse_bun(value: &Value) -> (Vec<LockGraphNode>, Vec<LockGraphEdge>) {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let Some(packages) = object(value, "packages") else {
        return (nodes, edges);
    };
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
        });
        if let Some(metadata) = values.get(2) {
            dependency_edges(metadata, &parent, &mut edges);
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
        PackageManagerId::Vlt | PackageManagerId::Deno => Err(format!(
            "{manager} lock graph extraction is not implemented"
        )),
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

fn resolution_integrities(graph: &LockGraph) -> BTreeMap<String, BTreeSet<String>> {
    let mut values = BTreeMap::<String, BTreeSet<String>>::new();
    for node in &graph.nodes {
        let resolution = format!("{}@{}", node.name, node.version);
        let integrities = values.entry(resolution).or_default();
        if let Some(integrity) = &node.integrity {
            integrities.insert(integrity.clone());
        }
    }
    values
}

fn integrity_family(value: &str) -> &str {
    value.split(['-', ':']).next().unwrap_or(value)
}

pub fn compare_lock_graphs(source: &LockGraph, target: &LockGraph) -> Result<LockGraphComparison> {
    const MAX_REPORTED_EDGE_CHANGES: usize = 100;

    let source_map = resolution_integrities(source);
    let target_map = resolution_integrities(target);
    let source_resolutions = source_map.keys().cloned().collect::<BTreeSet<_>>();
    let target_resolutions = target_map.keys().cloned().collect::<BTreeSet<_>>();
    let added_resolutions = target_resolutions
        .difference(&source_resolutions)
        .cloned()
        .collect::<Vec<_>>();
    let removed_resolutions = source_resolutions
        .difference(&target_resolutions)
        .cloned()
        .collect::<Vec<_>>();
    let mut integrity_mismatches = Vec::new();
    for resolution in source_resolutions.intersection(&target_resolutions) {
        let source_integrities = &source_map[resolution];
        let target_integrities = &target_map[resolution];
        if source_integrities.is_empty() || target_integrities.is_empty() {
            continue;
        }
        let comparable = source_integrities.iter().any(|left| {
            target_integrities
                .iter()
                .any(|right| integrity_family(left) == integrity_family(right))
        });
        if comparable && source_integrities.is_disjoint(target_integrities) {
            integrity_mismatches.push(resolution.clone());
        }
    }

    let source_edges = source.edges.iter().cloned().collect::<BTreeSet<_>>();
    let target_edges = target.edges.iter().cloned().collect::<BTreeSet<_>>();
    let mut edge_changes = source_edges
        .symmetric_difference(&target_edges)
        .map(|edge| format!("{} -> {} ({})", edge.from, edge.dependency, edge.kind))
        .collect::<Vec<_>>();
    if edge_changes.len() > MAX_REPORTED_EDGE_CHANGES {
        let omitted = edge_changes.len() - MAX_REPORTED_EDGE_CHANGES;
        edge_changes.truncate(MAX_REPORTED_EDGE_CHANGES);
        edge_changes.push(format!("{omitted} additional edge changes omitted"));
    }

    let status = if source.complete
        && target.complete
        && added_resolutions.is_empty()
        && removed_resolutions.is_empty()
        && integrity_mismatches.is_empty()
    {
        VerificationStatus::Passed
    } else {
        VerificationStatus::Failed
    };
    let comparison_id = short_digest(
        "lockdiff_",
        &(
            SCHEMA_VERSION,
            &source.graph_id,
            &target.graph_id,
            "resolution-set-v1",
            status,
            &added_resolutions,
            &removed_resolutions,
            &integrity_mismatches,
            &edge_changes,
        ),
    )?;
    Ok(LockGraphComparison {
        comparison_id,
        policy: "resolution-set-v1".to_owned(),
        status,
        source_graph_id: source.graph_id.clone(),
        target_graph_id: Some(target.graph_id.clone()),
        source_resolutions: source_resolutions.len(),
        target_resolutions: target_resolutions.len(),
        added_resolutions,
        removed_resolutions,
        integrity_mismatches,
        edge_changes,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn compares_npm_and_pnpm_resolution_sets() {
        let npm = tempdir().expect("npm fixture");
        fs::write(
            npm.path().join("package-lock.json"),
            r#"{
  "lockfileVersion": 3,
  "packages": {
    "": { "dependencies": { "example": "^1.0.0" } },
    "node_modules/example": {
      "version": "1.2.3",
      "integrity": "sha512-example",
      "dependencies": { "child": "^2.0.0" }
    },
    "node_modules/example/node_modules/child": {
      "version": "2.1.0",
      "integrity": "sha512-child"
    }
  }
}"#,
        )
        .expect("npm lockfile");
        let pnpm = tempdir().expect("pnpm fixture");
        fs::write(
            pnpm.path().join("pnpm-lock.yaml"),
            "lockfileVersion: '9.0'\npackages:\n  example@1.2.3:\n    resolution: {integrity: sha512-example}\n  child@2.1.0:\n    resolution: {integrity: sha512-child}\nsnapshots:\n  example@1.2.3:\n    dependencies:\n      child: 2.1.0\n  child@2.1.0: {}\n",
        )
        .expect("pnpm lockfile");

        let source = extract_lock_graph(npm.path(), PackageManagerId::Npm)
            .expect("npm extraction")
            .expect("npm graph");
        let target = extract_lock_graph(pnpm.path(), PackageManagerId::Pnpm)
            .expect("pnpm extraction")
            .expect("pnpm graph");
        let comparison = compare_lock_graphs(&source, &target).expect("comparison");
        assert_eq!(comparison.status, VerificationStatus::Passed);
        assert!(comparison.added_resolutions.is_empty());
        assert!(comparison.removed_resolutions.is_empty());
    }

    #[test]
    fn reports_resolution_drift() {
        let source = LockGraph {
            schema_version: SCHEMA_VERSION.to_owned(),
            graph_id: "lockgraph_source".to_owned(),
            manager: PackageManagerId::Npm,
            lockfile_path: "package-lock.json".to_owned(),
            lockfile_digest: "sha256:source".to_owned(),
            format: "npm-package-lock".to_owned(),
            complete: true,
            nodes: vec![LockGraphNode {
                locator: "node_modules/example".to_owned(),
                name: "example".to_owned(),
                version: "1.0.0".to_owned(),
                integrity: Some("sha512-old".to_owned()),
            }],
            edges: Vec::new(),
            diagnostics: Vec::new(),
        };
        let mut target = source.clone();
        target.graph_id = "lockgraph_target".to_owned();
        target.manager = PackageManagerId::Pnpm;
        target.nodes[0].version = "1.1.0".to_owned();
        let comparison = compare_lock_graphs(&source, &target).expect("comparison");
        assert_eq!(comparison.status, VerificationStatus::Failed);
        assert_eq!(comparison.added_resolutions, ["example@1.1.0"]);
        assert_eq!(comparison.removed_resolutions, ["example@1.0.0"]);

        let mut integrity_target = source.clone();
        integrity_target.graph_id = "lockgraph_integrity_target".to_owned();
        integrity_target.manager = PackageManagerId::Pnpm;
        integrity_target.nodes[0].integrity = Some("sha512-new".to_owned());
        let integrity_comparison =
            compare_lock_graphs(&source, &integrity_target).expect("integrity comparison");
        assert_eq!(integrity_comparison.status, VerificationStatus::Failed);
        assert_eq!(integrity_comparison.integrity_mismatches, ["example@1.0.0"]);
    }

    #[test]
    fn extracts_yarn_and_bun_text_lockfiles() {
        let yarn_classic = tempdir().expect("Yarn Classic fixture");
        fs::write(
            yarn_classic.path().join("yarn.lock"),
            "# yarn lockfile v1\n\nleft-pad@^1.0.0:\n  version \"1.3.0\"\n  integrity sha512-example\n  dependencies:\n    repeat-string \"^1.6.1\"\n",
        )
        .expect("Yarn Classic lockfile");
        let classic = extract_lock_graph(yarn_classic.path(), PackageManagerId::YarnClassic)
            .expect("Yarn Classic extraction")
            .expect("Yarn Classic graph");
        assert!(classic.complete);
        assert_eq!(classic.nodes[0].name, "left-pad");
        assert_eq!(classic.nodes[0].version, "1.3.0");
        assert_eq!(classic.edges[0].dependency, "repeat-string");

        let yarn_modern = tempdir().expect("Yarn Modern fixture");
        fs::write(
            yarn_modern.path().join("yarn.lock"),
            "__metadata:\n  version: 8\n\n\"left-pad@npm:^1.0.0\":\n  version: 1.3.0\n  resolution: \"left-pad@npm:1.3.0\"\n  checksum: 10/example\n  dependencies:\n    repeat-string: \"npm:^1.6.1\"\n",
        )
        .expect("Yarn Modern lockfile");
        let modern = extract_lock_graph(yarn_modern.path(), PackageManagerId::YarnModern)
            .expect("Yarn Modern extraction")
            .expect("Yarn Modern graph");
        assert!(modern.complete);
        assert_eq!(modern.nodes[0].name, "left-pad");
        assert_eq!(modern.nodes[0].version, "1.3.0");

        let bun = tempdir().expect("Bun fixture");
        fs::write(
            bun.path().join("bun.lock"),
            r#"{
  "lockfileVersion": 1,
  "packages": {
    "left-pad": ["left-pad@1.3.0", "", { "dependencies": { "repeat-string": "^1.6.1" } }, "sha512-example"],
  },
}
"#,
        )
        .expect("Bun lockfile");
        let bun = extract_lock_graph(bun.path(), PackageManagerId::Bun)
            .expect("Bun extraction")
            .expect("Bun graph");
        assert!(bun.complete);
        assert_eq!(bun.nodes[0].name, "left-pad");
        assert_eq!(bun.nodes[0].version, "1.3.0");
        assert_eq!(bun.edges[0].dependency, "repeat-string");
    }

    #[test]
    fn fails_closed_for_invalid_or_binary_lockfiles() {
        let invalid = tempdir().expect("invalid fixture");
        fs::write(invalid.path().join("pnpm-lock.yaml"), "packages: [").expect("invalid lockfile");
        let invalid = extract_lock_graph(invalid.path(), PackageManagerId::Pnpm)
            .expect("invalid extraction")
            .expect("invalid graph");
        assert!(!invalid.complete);
        assert_eq!(invalid.diagnostics[0].code, "LOCK_GRAPH_PARSE_FAILED");
        assert!(invalid.diagnostics[0].blocking);

        let incomplete = tempdir().expect("incomplete fixture");
        fs::write(
            incomplete.path().join("package-lock.json"),
            r#"{"lockfileVersion":3}"#,
        )
        .expect("incomplete lockfile");
        let incomplete = extract_lock_graph(incomplete.path(), PackageManagerId::Npm)
            .expect("incomplete extraction")
            .expect("incomplete graph");
        assert!(!incomplete.complete);
        assert_eq!(incomplete.diagnostics[0].code, "LOCK_GRAPH_PARSE_FAILED");

        let binary = tempdir().expect("binary fixture");
        fs::write(binary.path().join("bun.lockb"), [0, 1, 2, 3]).expect("binary lockfile");
        let binary = extract_lock_graph(binary.path(), PackageManagerId::Bun)
            .expect("binary extraction")
            .expect("binary graph");
        assert!(!binary.complete);
        assert_eq!(binary.diagnostics[0].code, "LOCK_GRAPH_FORMAT_UNSUPPORTED");
        assert!(binary.diagnostics[0].blocking);
    }
}
