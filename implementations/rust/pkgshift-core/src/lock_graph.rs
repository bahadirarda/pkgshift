mod comparison;
mod extraction;

pub use comparison::{compare_lock_graphs, compare_lock_graphs_for_project};
pub use extraction::extract_lock_graph;

#[cfg(test)]
use crate::model::{
    DependencyProtocol, LockGraph, LockGraphEdge, LockGraphNode, PackageManagerId, ProjectIr,
    SCHEMA_VERSION, VerificationStatus,
};

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    fn project_ir_with_dependencies(dependencies: &[(&str, &str)]) -> ProjectIr {
        ProjectIr {
            schema_version: SCHEMA_VERSION.to_owned(),
            project_ir_id: "ir_fixture".to_owned(),
            repository_fingerprint: "sha256:fixture".to_owned(),
            source: Some(PackageManagerId::Npm),
            root_package_path: ".".to_owned(),
            packages: vec![crate::model::PackageIr {
                path: ".".to_owned(),
                manifest_path: "package.json".to_owned(),
                name: Some("fixture".to_owned()),
                version: Some("1.0.0".to_owned()),
                private: Some(true),
                dependencies: dependencies
                    .iter()
                    .map(|(section, name)| crate::model::DependencyIr {
                        package_path: ".".to_owned(),
                        section: (*section).to_owned(),
                        name: (*name).to_owned(),
                        specifier: "*".to_owned(),
                        protocol: DependencyProtocol::Semver,
                        location: format!("package.json#/{section}/{name}"),
                    })
                    .collect(),
                script_names: Vec::new(),
            }],
            workspace_patterns: Vec::new(),
            features: Vec::new(),
            integrations: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn graph(
        id: &str,
        manager: PackageManagerId,
        nodes: &[(&str, &str)],
        edges: &[(&str, &str, &str)],
    ) -> LockGraph {
        LockGraph {
            schema_version: SCHEMA_VERSION.to_owned(),
            graph_id: id.to_owned(),
            manager,
            lockfile_path: "fixture.lock".to_owned(),
            lockfile_digest: format!("sha256:{id}"),
            format: "npm-package-lock".to_owned(),
            complete: true,
            nodes: nodes
                .iter()
                .map(|(name, version)| LockGraphNode {
                    locator: format!("node_modules/{name}"),
                    name: (*name).to_owned(),
                    version: (*version).to_owned(),
                    integrity: Some(format!("sha512-{name}-{version}")),
                })
                .collect(),
            edges: edges
                .iter()
                .map(|(from, dependency, kind)| LockGraphEdge {
                    from: (*from).to_owned(),
                    dependency: (*dependency).to_owned(),
                    kind: (*kind).to_owned(),
                    target: None,
                })
                .collect(),
            diagnostics: Vec::new(),
        }
    }

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
    fn excludes_local_pnpm_package_locators_from_the_registry_graph() {
        let directory = tempdir().expect("pnpm fixture");
        fs::write(
            directory.path().join("pnpm-lock.yaml"),
            "lockfileVersion: '9.0'\npackages:\n  example@1.2.3:\n    resolution: {integrity: sha512-example}\n  '@fixture/local@file:packages/local':\n    resolution: {directory: packages/local, type: directory}\nsnapshots:\n  example@1.2.3: {}\n  '@fixture/local@file:packages/local': {}\n",
        )
        .expect("pnpm lockfile");

        let graph = extract_lock_graph(directory.path(), PackageManagerId::Pnpm)
            .expect("pnpm extraction")
            .expect("pnpm graph");

        assert!(graph.complete);
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.nodes[0].name, "example");
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
    fn reachable_policy_prunes_stale_lockfile_resolutions() {
        let project_ir = project_ir_with_dependencies(&[("dependencies", "live")]);
        let source = graph(
            "lockgraph_source",
            PackageManagerId::Npm,
            &[("child", "2.0.0"), ("live", "1.0.0"), ("stale", "9.0.0")],
            &[("live@1.0.0", "child", "dependency")],
        );
        let target = graph(
            "lockgraph_target",
            PackageManagerId::Deno,
            &[("child", "2.0.0"), ("live", "1.0.0")],
            &[("live@1.0.0", "child", "dependency")],
        );

        let comparison = compare_lock_graphs_for_project(&source, &target, &project_ir)
            .expect("reachable comparison");

        assert_eq!(comparison.policy, "reachable-resolution-set-v2");
        assert_eq!(comparison.status, VerificationStatus::Passed);
        assert_eq!(comparison.source_resolutions, 2);
        assert_eq!(comparison.pruned_source_resolutions, ["stale@9.0.0"]);
        assert!(comparison.removed_resolutions.is_empty());
    }

    #[test]
    fn reachable_policy_tolerates_absent_optional_platform_branches() {
        let project_ir = project_ir_with_dependencies(&[("dependencies", "live")]);
        let source = graph(
            "lockgraph_source",
            PackageManagerId::Npm,
            &[("live", "1.0.0"), ("platform-package", "1.0.0")],
            &[("live@1.0.0", "platform-package", "optional")],
        );
        let target = graph(
            "lockgraph_target",
            PackageManagerId::Deno,
            &[("live", "1.0.0")],
            &[],
        );

        let comparison = compare_lock_graphs_for_project(&source, &target, &project_ir)
            .expect("optional comparison");

        assert_eq!(comparison.status, VerificationStatus::Passed);
        assert_eq!(
            comparison.optional_platform_differences,
            ["source-only:platform-package@1.0.0"]
        );
    }

    #[test]
    fn reachable_policy_still_blocks_optional_version_drift() {
        let project_ir = project_ir_with_dependencies(&[("dependencies", "live")]);
        let source = graph(
            "lockgraph_source",
            PackageManagerId::Npm,
            &[("live", "1.0.0"), ("platform-package", "1.0.0")],
            &[("live@1.0.0", "platform-package", "optional")],
        );
        let target = graph(
            "lockgraph_target",
            PackageManagerId::Deno,
            &[("live", "1.0.0"), ("platform-package", "2.0.0")],
            &[("live@1.0.0", "platform-package", "optional")],
        );

        let comparison = compare_lock_graphs_for_project(&source, &target, &project_ir)
            .expect("optional drift comparison");

        assert_eq!(comparison.status, VerificationStatus::Failed);
        assert_eq!(comparison.added_resolutions, ["platform-package@2.0.0"]);
        assert_eq!(comparison.removed_resolutions, ["platform-package@1.0.0"]);
        assert!(comparison.optional_platform_differences.is_empty());
    }

    #[test]
    fn reachable_policy_fails_closed_for_unresolved_required_edges() {
        let project_ir = project_ir_with_dependencies(&[("dependencies", "live")]);
        let source = graph(
            "lockgraph_source",
            PackageManagerId::Npm,
            &[("live", "1.0.0")],
            &[("live@1.0.0", "missing", "dependency")],
        );
        let target = source.clone();

        let comparison = compare_lock_graphs_for_project(&source, &target, &project_ir)
            .expect("unresolved comparison");

        assert_eq!(comparison.status, VerificationStatus::Failed);
        assert_eq!(comparison.reachability_issues.len(), 2);
        assert!(
            comparison
                .reachability_issues
                .iter()
                .all(|issue| issue.contains("unresolved required edge"))
        );
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
    "repeat-string": ["repeat-string@1.6.1", "", {}, "sha512-child"],
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
        assert_eq!(bun.edges[0].target.as_deref(), Some("repeat-string@1.6.1"));
    }

    #[test]
    fn reachable_policy_uses_exact_bun_edge_targets() {
        let source_directory = tempdir().expect("Bun source fixture");
        fs::write(
            source_directory.path().join("bun.lock"),
            r#"{
  "lockfileVersion": 1,
  "workspaces": { "": { "dependencies": { "live": "1.0.0" } } },
  "packages": {
    "live": ["live@1.0.0", "", { "dependencies": { "child": "^2.0.0" } }, "sha512-live"],
    "child": ["child@2.0.0", "", {}, "sha512-child-current"],
    "stale-parent/child": ["child@1.0.0", "", {}, "sha512-child-stale"],
  },
}
"#,
        )
        .expect("Bun source lockfile");
        let target_directory = tempdir().expect("Deno target fixture");
        fs::write(
            target_directory.path().join("deno.lock"),
            r#"{
  "version": "5",
  "npm": {
    "live@1.0.0": { "integrity": "sha512-live", "dependencies": ["child@2.0.0"] },
    "child@2.0.0": { "integrity": "sha512-child-current" }
  }
}
"#,
        )
        .expect("Deno target lockfile");
        let source = extract_lock_graph(source_directory.path(), PackageManagerId::Bun)
            .expect("Bun extraction")
            .expect("Bun graph");
        let target = extract_lock_graph(target_directory.path(), PackageManagerId::Deno)
            .expect("Deno extraction")
            .expect("Deno graph");
        let project_ir = project_ir_with_dependencies(&[("dependencies", "live")]);

        let comparison = compare_lock_graphs_for_project(&source, &target, &project_ir)
            .expect("exact Bun comparison");

        assert_eq!(comparison.status, VerificationStatus::Passed);
        assert_eq!(comparison.source_resolutions, 2);
        assert_eq!(comparison.pruned_source_resolutions, ["child@1.0.0"]);
    }

    #[test]
    fn extracts_vlt_and_deno_lock_graphs() {
        let vlt = tempdir().expect("vlt fixture");
        fs::write(
            vlt.path().join("vlt-lock.json"),
            r#"{
  "lockfileVersion": 1,
  "nodes": {
    "~npm~left-pad@1.3.0": [0, "left-pad", "sha512-example", "https://registry.npmjs.org/left-pad/-/left-pad-1.3.0.tgz"],
    "~npm~@scope+package@2.1.0~peer.1": [0, "@scope/package", "sha512-scoped", "https://registry.npmjs.org/@scope/package/-/package-2.1.0.tgz"]
  },
  "edges": {}
}"#,
        )
        .expect("vlt lockfile");
        let vlt = extract_lock_graph(vlt.path(), PackageManagerId::Vlt)
            .expect("vlt extraction")
            .expect("vlt graph");
        assert!(vlt.complete);
        assert_eq!(vlt.nodes.len(), 2);
        assert!(
            vlt.nodes
                .iter()
                .any(|node| node.name == "@scope/package" && node.version == "2.1.0")
        );

        let deno = tempdir().expect("Deno fixture");
        fs::write(
            deno.path().join("deno.lock"),
            r#"{
  "version": "5",
  "specifiers": {"npm:left-pad@^1.0.0": "1.3.0"},
  "npm": {
    "left-pad@1.3.0": {"integrity": "sha512-example", "dependencies": ["repeat-string@1.6.1"]},
    "repeat-string@1.6.1": {"integrity": "sha512-child"}
  }
}"#,
        )
        .expect("Deno lockfile");
        let deno = extract_lock_graph(deno.path(), PackageManagerId::Deno)
            .expect("Deno extraction")
            .expect("Deno graph");
        assert!(deno.complete);
        assert_eq!(deno.nodes.len(), 2);
        assert_eq!(deno.edges[0].dependency, "repeat-string");
    }

    #[test]
    fn normalizes_deno_peer_contexts_to_registry_versions() {
        let directory = tempdir().expect("Deno fixture");
        fs::write(
            directory.path().join("deno.lock"),
            r#"{
  "version": "5",
  "npm": {
    "eslint-plugin-example@1.2.3_eslint@9.0.0": {
      "integrity": "sha512-example",
      "dependencies": ["eslint@9.0.0"]
    }
  }
}"#,
        )
        .expect("Deno lockfile");

        let graph = extract_lock_graph(directory.path(), PackageManagerId::Deno)
            .expect("Deno extraction")
            .expect("Deno graph");

        assert!(graph.complete);
        assert_eq!(graph.nodes[0].name, "eslint-plugin-example");
        assert_eq!(graph.nodes[0].version, "1.2.3");
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
