use std::collections::BTreeSet;
use std::path::Path;

pub use crate::capability::analyze_capabilities;
use crate::catalog::{get_package_manager, native_import_strategy};
use crate::cleanup;
use crate::model::{
    CapabilityAnalysis, Diagnostic, DiagnosticSeverity, LockGraph, MigrationPlan, NativeImportMode,
    PackageManagerId, PlannedFileMutation, PlannedOperation, ProjectInspection, ProjectIr,
    SCHEMA_VERSION, SideEffect, SupportTier,
};
use crate::transformation::transform_project;
use crate::util::{Result, short_digest};

fn operation(
    index: usize,
    phase: &str,
    kind: &str,
    description: String,
    mutations: Vec<PlannedFileMutation>,
) -> Option<PlannedOperation> {
    if mutations.is_empty() {
        return None;
    }
    let capabilities = mutations
        .iter()
        .flat_map(|entry| entry.capabilities.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Some(PlannedOperation {
        id: format!("op_{index:03}"),
        phase: phase.to_owned(),
        kind: kind.to_owned(),
        description,
        paths: mutations.iter().map(|entry| entry.path.clone()).collect(),
        command: Vec::new(),
        capabilities,
        side_effect: SideEffect::RepositoryWrite,
        reversible: true,
        preconditions: vec!["Current file digests match the accepted plan.".to_owned()],
        postconditions: vec!["Written file digests match the accepted plan.".to_owned()],
        mutations,
    })
}

pub fn plan_package_manager_migration(
    inspection: &ProjectInspection,
    project_ir: &ProjectIr,
    analysis: &CapabilityAnalysis,
    source_lock_graph: Option<&LockGraph>,
    target: PackageManagerId,
    accepted_lossy: bool,
) -> Result<Option<MigrationPlan>> {
    let Some(source) = inspection.package_manager.selected else {
        return Ok(None);
    };
    let target_definition = get_package_manager(target);
    let source_definition = get_package_manager(source);
    let transformation = transform_project(inspection, project_ir, analysis, target)?;
    let mut diagnostics = project_ir.diagnostics.clone();
    diagnostics.extend(analysis.diagnostics.clone());
    diagnostics.extend(transformation.diagnostics);
    diagnostics.extend(cleanup::runtime_reference_diagnostics(
        Path::new(&inspection.root),
        project_ir,
        target,
    )?);
    if let Some(graph) = source_lock_graph {
        diagnostics.extend(graph.diagnostics.clone());
    }
    let native_import = native_import_strategy(source, target, source_lock_graph.is_some());
    if source != target && source_lock_graph.is_some() && native_import.is_none() {
        diagnostics.push(Diagnostic {
            code: "NATIVE_IMPORT_UNAVAILABLE".to_owned(),
            severity: DiagnosticSeverity::Warning,
            summary: format!(
                "No verified target-native lockfile importer is registered for {source} to {target}."
            ),
            blocking: false,
            evidence: Vec::new(),
            remediation: vec![
                "pkgshift will generate target dependency state and require lock graph verification."
                    .to_owned(),
            ],
        });
    }
    if target_definition.tier == SupportTier::PreviewTarget {
        diagnostics.push(Diagnostic {
            code: "PM_TARGET_PREVIEW".to_owned(),
            severity: DiagnosticSeverity::Warning,
            summary: format!(
                "{} is a preview migration target.",
                target_definition.display_name
            ),
            blocking: false,
            evidence: Vec::new(),
            remediation: vec![
                "Use the plan for assessment; preview targets cannot be applied.".to_owned(),
            ],
        });
    }
    if source == target {
        diagnostics.push(Diagnostic {
            code: "PM_TARGET_ALREADY_SELECTED".to_owned(),
            severity: DiagnosticSeverity::Warning,
            summary: format!("{} is already selected.", target_definition.display_name),
            blocking: false,
            evidence: Vec::new(),
            remediation: vec!["Select another target or verify the current state.".to_owned()],
        });
    }
    if analysis.summary.lossy > 0 && !accepted_lossy {
        diagnostics.push(Diagnostic::blocking(
            "LOSSY_ACCEPTANCE_REQUIRED",
            "Lossy capability decisions require explicit acceptance in the plan.",
            vec!["Review the diagnostics and re-plan with --accept-lossy.".to_owned()],
        ));
    }

    let mut operations = Vec::new();
    if source != target {
        if let Some(value) = operation(
            operations.len() + 1,
            "configure",
            "manifest.render-target",
            format!(
                "Render {}-compatible package manifests.",
                target_definition.display_name
            ),
            transformation.manifest_mutations,
        ) {
            operations.push(value);
        }
        if let Some(value) = operation(
            operations.len() + 1,
            "configure",
            "configuration.render-target",
            format!(
                "Render deterministic {} configuration.",
                target_definition.display_name
            ),
            transformation.configuration_mutations,
        ) {
            operations.push(value);
        }
        if let Some(value) = operation(
            operations.len() + 1,
            "integrate",
            "integration.translate-commands",
            format!(
                "Translate recognized {} commands in repository integrations.",
                source_definition.display_name
            ),
            transformation.integration_mutations,
        ) {
            operations.push(value);
        }
        operations.push(cleanup::plan_operation(operations.len() + 1, project_ir));
        if let Some(strategy) = native_import
            .as_ref()
            .filter(|strategy| strategy.mode == NativeImportMode::DedicatedCommand)
        {
            operations.push(PlannedOperation {
                id: format!("op_{:03}", operations.len() + 1),
                phase: "install".to_owned(),
                kind: "dependency.import-target".to_owned(),
                description: strategy.summary.clone(),
                paths: target_definition
                    .lockfiles
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
                command: strategy.command.clone(),
                capabilities: analysis
                    .decisions
                    .iter()
                    .map(|decision| decision.feature_id.clone())
                    .collect(),
                side_effect: SideEffect::DependencyState,
                reversible: true,
                preconditions: vec![
                    "Source dependency state and target configuration match the accepted plan."
                        .to_owned(),
                ],
                postconditions: vec!["The target-native importer exits successfully.".to_owned()],
                mutations: Vec::new(),
            });
        }
        let install_integrates_import = native_import
            .as_ref()
            .is_some_and(|strategy| strategy.mode == NativeImportMode::InstallIntegrated);
        operations.push(PlannedOperation {
            id: format!("op_{:03}", operations.len() + 1),
            phase: "install".to_owned(),
            kind: if install_integrates_import {
                "dependency.import-and-install-target"
            } else {
                "dependency.install-target"
            }
            .to_owned(),
            description: native_import
                .as_ref()
                .filter(|strategy| strategy.mode == NativeImportMode::InstallIntegrated)
                .map_or_else(
                    || {
                        format!(
                            "Generate {} dependency state without lifecycle scripts.",
                            target_definition.display_name
                        )
                    },
                    |strategy| strategy.summary.clone(),
                ),
            paths: target_definition
                .lockfiles
                .iter()
                .map(ToString::to_string)
                .collect(),
            command: target_definition
                .install_command
                .iter()
                .map(ToString::to_string)
                .collect(),
            capabilities: analysis
                .decisions
                .iter()
                .map(|decision| decision.feature_id.clone())
                .collect(),
            side_effect: SideEffect::DependencyState,
            reversible: true,
            preconditions: vec!["Target configuration matches the plan.".to_owned()],
            postconditions: vec!["The target installer exits successfully.".to_owned()],
            mutations: Vec::new(),
        });
        if let Some(value) = operation(
            operations.len() + 1,
            "cleanup",
            "source.retire",
            format!(
                "Retire source-only {} artifacts.",
                source_definition.display_name
            ),
            transformation.cleanup_mutations,
        ) {
            operations.push(value);
        }
        operations.push(PlannedOperation {
            id: format!("op_{:03}", operations.len() + 1),
            phase: "verify".to_owned(),
            kind: "migration.verify".to_owned(),
            description: "Verify planned digests and target dependency state.".to_owned(),
            paths: Vec::new(),
            command: Vec::new(),
            capabilities: analysis
                .decisions
                .iter()
                .map(|decision| decision.feature_id.clone())
                .collect(),
            side_effect: SideEffect::None,
            reversible: false,
            preconditions: vec!["All apply operations completed.".to_owned()],
            postconditions: vec!["No blocking verification check remains.".to_owned()],
            mutations: Vec::new(),
        });
    }

    let executable = source != target
        && target_definition.tier == SupportTier::ProductionTarget
        && !diagnostics.iter().any(|entry| entry.blocking);
    let plan_id = short_digest(
        "plan_",
        &(
            SCHEMA_VERSION,
            source,
            target,
            target_definition.tier,
            &inspection.fingerprint,
            &project_ir.project_ir_id,
            &analysis.analysis_id,
            &analysis.summary,
            source_lock_graph.map(|graph| &graph.graph_id),
            &native_import,
            accepted_lossy,
            executable,
            &operations,
            &diagnostics,
        ),
    )?;
    Ok(Some(MigrationPlan {
        schema_version: SCHEMA_VERSION.to_owned(),
        plan_id,
        executable,
        accepted_lossy,
        source,
        target,
        target_tier: target_definition.tier,
        repository_fingerprint: inspection.fingerprint.clone(),
        project_ir_id: project_ir.project_ir_id.clone(),
        capability_analysis_id: analysis.analysis_id.clone(),
        capability_summary: analysis.summary.clone(),
        source_lock_graph_id: source_lock_graph.map(|graph| graph.graph_id.clone()),
        native_import,
        operations,
        diagnostics,
        verification: vec![
            "planned file digests match".to_owned(),
            "target package manager is selected".to_owned(),
            "target lockfile exists".to_owned(),
            "source-only artifacts are retired".to_owned(),
            "pre-migration local dependency state was removed before target installation"
                .to_owned(),
            "workspace membership is preserved".to_owned(),
            "target installation operation succeeded".to_owned(),
            if source_lock_graph.is_some() {
                "source and target resolution sets match".to_owned()
            } else {
                "resolved graph comparison is skipped when no source lockfile exists".to_owned()
            },
        ],
    }))
}

#[cfg(test)]
mod tests;
