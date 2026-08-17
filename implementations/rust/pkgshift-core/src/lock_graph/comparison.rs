use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::model::{
    DependencyProtocol, LockGraph, LockGraphComparison, LockGraphEdge, ProjectIr, SCHEMA_VERSION,
    VerificationStatus,
};
use crate::util::{Result, short_digest};
use crate::verification_policy::{
    EdgeEquivalencePolicy, PackagePlatformConstraint, VerificationPolicy,
};

use super::extraction::split_locator;

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

fn resolution_platforms(graph: &LockGraph) -> BTreeMap<String, Vec<&PackagePlatformConstraint>> {
    let mut values = BTreeMap::<String, Vec<&PackagePlatformConstraint>>::new();
    for node in &graph.nodes {
        values
            .entry(format!("{}@{}", node.name, node.version))
            .or_default()
            .push(&node.platform);
    }
    values
}

fn compatible_with_matrix(
    constraints: &[&PackagePlatformConstraint],
    policy: &VerificationPolicy,
) -> bool {
    constraints.iter().any(|constraint| {
        policy
            .target_platforms
            .iter()
            .any(|target| constraint.allows(target))
    })
}

fn integrity_family(value: &str) -> &str {
    value.split(['-', ':']).next().unwrap_or(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Reachability {
    Optional,
    Required,
}

struct ReachableGraph {
    resolutions: BTreeMap<String, BTreeSet<String>>,
    optional: BTreeSet<String>,
    pruned: Vec<String>,
    issues: Vec<String>,
    edges: BTreeSet<LockGraphEdge>,
}

fn graph_supports_reachability(graph: &LockGraph) -> bool {
    matches!(
        graph.format.as_str(),
        "npm-package-lock"
            | "pnpm-lock"
            | "yarn-classic-lock"
            | "yarn-modern-lock"
            | "bun-text-lock"
            | "deno-lock-v5"
            | "absent-empty"
    )
}

fn alias_dependency_name(name: &str, specifier: &str) -> String {
    specifier
        .strip_prefix("npm:")
        .and_then(split_locator)
        .map_or_else(|| name.to_owned(), |(target, _)| target)
}

fn project_roots(project_ir: &ProjectIr) -> BTreeMap<String, Reachability> {
    let workspace_packages = project_ir
        .packages
        .iter()
        .filter_map(|package| package.name.clone())
        .collect::<BTreeSet<_>>();
    let mut roots = BTreeMap::<String, Reachability>::new();
    for dependency in project_ir
        .packages
        .iter()
        .flat_map(|package| &package.dependencies)
    {
        if matches!(
            dependency.protocol,
            DependencyProtocol::Workspace
                | DependencyProtocol::File
                | DependencyProtocol::Link
                | DependencyProtocol::Portal
        ) {
            continue;
        }
        let name = alias_dependency_name(&dependency.name, &dependency.specifier);
        if workspace_packages.contains(&name) {
            continue;
        }
        let reachability = if matches!(
            dependency.section.as_str(),
            "optionalDependencies" | "peerDependencies"
        ) {
            Reachability::Optional
        } else {
            Reachability::Required
        };
        roots
            .entry(name)
            .and_modify(|current| *current = (*current).max(reachability))
            .or_insert(reachability);
    }
    roots
}

fn resolution_name(resolution: &str) -> &str {
    if resolution.starts_with('@') {
        resolution
            .find('/')
            .and_then(|slash| {
                resolution[slash + 1..]
                    .find('@')
                    .map(|index| slash + index + 1)
            })
            .map_or(resolution, |separator| &resolution[..separator])
    } else {
        resolution
            .rfind('@')
            .map_or(resolution, |separator| &resolution[..separator])
    }
}

fn reachable_graph(
    label: &str,
    graph: &LockGraph,
    roots: &BTreeMap<String, Reachability>,
    exact_bun_roots: bool,
) -> ReachableGraph {
    let all_resolutions = resolution_integrities(graph);
    let mut by_name = BTreeMap::<String, BTreeSet<String>>::new();
    for node in &graph.nodes {
        by_name
            .entry(node.name.clone())
            .or_default()
            .insert(format!("{}@{}", node.name, node.version));
    }
    let mut edges_by_parent = BTreeMap::<String, Vec<&LockGraphEdge>>::new();
    for edge in &graph.edges {
        edges_by_parent
            .entry(edge.from.clone())
            .or_default()
            .push(edge);
    }

    let mut states = BTreeMap::<String, Reachability>::new();
    let mut queue = VecDeque::new();
    let mut issues = BTreeSet::new();
    let enqueue = |resolution: &str,
                   reachability: Reachability,
                   states: &mut BTreeMap<String, Reachability>,
                   queue: &mut VecDeque<(String, Reachability)>| {
        let current = states.get(resolution).copied();
        if current.is_none_or(|value| value < reachability) {
            states.insert(resolution.to_owned(), reachability);
            queue.push_back((resolution.to_owned(), reachability));
        }
    };

    for (name, reachability) in roots {
        let exact_bun_root = (exact_bun_roots && graph.format == "bun-text-lock")
            .then(|| {
                graph
                    .nodes
                    .iter()
                    .filter(|node| node.locator == *name)
                    .map(|node| format!("{}@{}", node.name, node.version))
                    .collect::<BTreeSet<_>>()
            })
            .filter(|candidates| !candidates.is_empty());
        if let Some(candidates) = exact_bun_root.as_ref().or_else(|| by_name.get(name)) {
            for resolution in candidates {
                enqueue(resolution, *reachability, &mut states, &mut queue);
            }
        } else if *reachability == Reachability::Required {
            issues.insert(format!("{label}: unresolved required root {name}"));
        }
    }

    while let Some((parent, parent_reachability)) = queue.pop_front() {
        if states.get(&parent).copied() != Some(parent_reachability) {
            continue;
        }
        for edge in edges_by_parent.get(&parent).into_iter().flatten() {
            let edge_reachability = if edge.kind == "dependency" {
                parent_reachability
            } else {
                Reachability::Optional
            };
            if let Some(target) = edge.target.as_ref() {
                if all_resolutions.contains_key(target) {
                    enqueue(target, edge_reachability, &mut states, &mut queue);
                } else if edge_reachability == Reachability::Required {
                    issues.insert(format!(
                        "{label}: unresolved required target {parent} -> {target}"
                    ));
                }
            } else if let Some(candidates) = by_name.get(&edge.dependency) {
                for resolution in candidates {
                    enqueue(resolution, edge_reachability, &mut states, &mut queue);
                }
            } else if edge_reachability == Reachability::Required {
                issues.insert(format!(
                    "{label}: unresolved required edge {parent} -> {}",
                    edge.dependency
                ));
            }
        }
    }

    let resolutions = all_resolutions
        .iter()
        .filter(|(resolution, _)| states.contains_key(*resolution))
        .map(|(resolution, integrities)| (resolution.clone(), integrities.clone()))
        .collect::<BTreeMap<_, _>>();
    let optional = states
        .iter()
        .filter(|(_, reachability)| **reachability == Reachability::Optional)
        .map(|(resolution, _)| resolution.clone())
        .collect::<BTreeSet<_>>();
    let pruned = all_resolutions
        .keys()
        .filter(|resolution| !states.contains_key(*resolution))
        .cloned()
        .collect::<Vec<_>>();
    let edges = graph
        .edges
        .iter()
        .filter(|edge| states.contains_key(&edge.from))
        .cloned()
        .collect::<BTreeSet<_>>();
    ReachableGraph {
        resolutions,
        optional,
        pruned,
        issues: issues.into_iter().collect(),
        edges,
    }
}

fn compare_resolution_maps(
    source: &LockGraph,
    target: &LockGraph,
    policy: &str,
    source_map: &BTreeMap<String, BTreeSet<String>>,
    target_map: &BTreeMap<String, BTreeSet<String>>,
    source_edges: &BTreeSet<LockGraphEdge>,
    target_edges: &BTreeSet<LockGraphEdge>,
    pruned_source_resolutions: Vec<String>,
    pruned_target_resolutions: Vec<String>,
    optional_platform_differences: Vec<String>,
    reachability_issues: Vec<String>,
    verification_policy: &VerificationPolicy,
) -> Result<LockGraphComparison> {
    const MAX_REPORTED_EDGE_CHANGES: usize = 100;

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

    let normalized_edges = |edges: &BTreeSet<LockGraphEdge>| {
        edges
            .iter()
            .map(|edge| {
                (
                    edge.from.clone(),
                    edge.dependency.clone(),
                    edge.kind.clone(),
                )
            })
            .collect::<BTreeSet<_>>()
    };
    let source_edges = normalized_edges(source_edges);
    let target_edges = normalized_edges(target_edges);
    let mut edge_changes = source_edges
        .symmetric_difference(&target_edges)
        .map(|(from, dependency, kind)| format!("{from} -> {dependency} ({kind})"))
        .collect::<Vec<_>>();
    if edge_changes.len() > MAX_REPORTED_EDGE_CHANGES {
        let omitted = edge_changes.len() - MAX_REPORTED_EDGE_CHANGES;
        edge_changes.truncate(MAX_REPORTED_EDGE_CHANGES);
        edge_changes.push(format!("{omitted} additional edge changes omitted"));
    }

    let status = if source.complete
        && target.complete
        && reachability_issues.is_empty()
        && added_resolutions.is_empty()
        && removed_resolutions.is_empty()
        && integrity_mismatches.is_empty()
        && (verification_policy.edge_equivalence != EdgeEquivalencePolicy::Strict
            || edge_changes.is_empty())
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
            policy,
            status,
            &added_resolutions,
            &removed_resolutions,
            &integrity_mismatches,
            &edge_changes,
            &pruned_source_resolutions,
            &pruned_target_resolutions,
            &optional_platform_differences,
            &reachability_issues,
            verification_policy,
        ),
    )?;
    Ok(LockGraphComparison {
        comparison_id,
        policy: policy.to_owned(),
        status,
        source_graph_id: source.graph_id.clone(),
        target_graph_id: Some(target.graph_id.clone()),
        source_resolutions: source_resolutions.len(),
        target_resolutions: target_resolutions.len(),
        added_resolutions,
        removed_resolutions,
        integrity_mismatches,
        edge_changes,
        verification_policy: verification_policy.clone(),
        pruned_source_resolutions,
        pruned_target_resolutions,
        optional_platform_differences,
        reachability_issues,
    })
}

pub fn compare_lock_graphs(source: &LockGraph, target: &LockGraph) -> Result<LockGraphComparison> {
    let policy = VerificationPolicy::default();
    let source_map = resolution_integrities(source);
    let target_map = resolution_integrities(target);
    let source_edges = source.edges.iter().cloned().collect::<BTreeSet<_>>();
    let target_edges = target.edges.iter().cloned().collect::<BTreeSet<_>>();
    compare_resolution_maps(
        source,
        target,
        "resolution-set-v1",
        &source_map,
        &target_map,
        &source_edges,
        &target_edges,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        &policy,
    )
}

pub fn compare_lock_graphs_for_project(
    source: &LockGraph,
    target: &LockGraph,
    project_ir: &ProjectIr,
) -> Result<LockGraphComparison> {
    compare_lock_graphs_for_project_with_policy(
        source,
        target,
        project_ir,
        &VerificationPolicy::default(),
    )
}

pub fn compare_lock_graphs_for_project_with_policy(
    source: &LockGraph,
    target: &LockGraph,
    project_ir: &ProjectIr,
    verification_policy: &VerificationPolicy,
) -> Result<LockGraphComparison> {
    if !graph_supports_reachability(source) || !graph_supports_reachability(target) {
        let source_map = resolution_integrities(source);
        let target_map = resolution_integrities(target);
        let source_edges = source.edges.iter().cloned().collect::<BTreeSet<_>>();
        let target_edges = target.edges.iter().cloned().collect::<BTreeSet<_>>();
        return compare_resolution_maps(
            source,
            target,
            "resolution-set-v1",
            &source_map,
            &target_map,
            &source_edges,
            &target_edges,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            verification_policy,
        );
    }

    let roots = project_roots(project_ir);
    let exact_bun_roots = project_ir.packages.len() == 1;
    let source_reachable = reachable_graph("source", source, &roots, exact_bun_roots);
    let target_reachable = reachable_graph("target", target, &roots, exact_bun_roots);
    let source_names = source_reachable
        .resolutions
        .keys()
        .map(|resolution| resolution_name(resolution).to_owned())
        .collect::<BTreeSet<_>>();
    let target_names = target_reachable
        .resolutions
        .keys()
        .map(|resolution| resolution_name(resolution).to_owned())
        .collect::<BTreeSet<_>>();
    let mut source_map = source_reachable.resolutions;
    let mut target_map = target_reachable.resolutions;
    let mut optional_platform_differences = Vec::new();
    let source_platforms = resolution_platforms(source);
    let target_platforms = resolution_platforms(target);

    let source_optional_only = source_reachable
        .optional
        .iter()
        .filter(|resolution| !target_names.contains(resolution_name(resolution)))
        .cloned()
        .collect::<Vec<_>>();
    for resolution in source_optional_only {
        let tolerated = verification_policy.target_platforms.is_empty()
            || source_platforms
                .get(&resolution)
                .is_some_and(|constraints| {
                    !compatible_with_matrix(constraints, verification_policy)
                });
        if tolerated {
            source_map.remove(&resolution);
            optional_platform_differences.push(format!("source-only:{resolution}"));
        }
    }
    let target_optional_only = target_reachable
        .optional
        .iter()
        .filter(|resolution| !source_names.contains(resolution_name(resolution)))
        .cloned()
        .collect::<Vec<_>>();
    for resolution in target_optional_only {
        let tolerated = verification_policy.target_platforms.is_empty()
            || target_platforms
                .get(&resolution)
                .is_some_and(|constraints| {
                    !compatible_with_matrix(constraints, verification_policy)
                });
        if tolerated {
            target_map.remove(&resolution);
            optional_platform_differences.push(format!("target-only:{resolution}"));
        }
    }
    optional_platform_differences.sort();

    let mut reachability_issues = source_reachable.issues;
    reachability_issues.extend(target_reachable.issues);
    reachability_issues.sort();
    reachability_issues.dedup();
    compare_resolution_maps(
        source,
        target,
        "reachable-resolution-set-v3",
        &source_map,
        &target_map,
        &source_reachable.edges,
        &target_reachable.edges,
        source_reachable.pruned,
        target_reachable.pruned,
        optional_platform_differences,
        reachability_issues,
        verification_policy,
    )
}
