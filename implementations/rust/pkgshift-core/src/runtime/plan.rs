use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::model::{
    Diagnostic, DiagnosticSeverity, EvidenceDetail, MutationAction, PlannedFileMutation,
    PlannedOperation, SCHEMA_VERSION, SideEffect,
};
use crate::util::{Result, digest_text, short_digest};

use super::inspect::inspect_runtime;
use super::model::{DenoPermission, RuntimeMigrationPlan};
use super::recipe::transform_file;

fn deduplicate_diagnostics(diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut unique = BTreeMap::new();
    for diagnostic in diagnostics {
        let location = diagnostic
            .evidence
            .first()
            .map_or("", |evidence| evidence.location.as_str());
        unique
            .entry((diagnostic.code.clone(), location.to_owned()))
            .or_insert(diagnostic);
    }
    unique.into_values().collect()
}

pub(crate) fn create_runtime_plan(
    root: &Path,
    permissions: &BTreeSet<DenoPermission>,
) -> Result<RuntimeMigrationPlan> {
    let inspection = inspect_runtime(root)?;
    let mut recipes = Vec::new();
    let mut diagnostics = Vec::new();
    let mut mutations = Vec::new();
    let mut required_permissions = BTreeSet::new();
    let has_input_diagnostics = !inspection.input_diagnostics.is_empty();
    diagnostics.extend(inspection.input_diagnostics);

    if inspection.bun_evidence.is_empty() && !has_input_diagnostics {
        diagnostics.push(Diagnostic::blocking(
            "RUNTIME_BUN_SOURCE_NOT_DETECTED",
            "No Bun runtime API, import, script, type dependency, or configuration evidence was detected.",
            vec![
                "Run this command from a Bun runtime project root, or use package-manager migration commands for lockfile-only changes."
                    .to_owned(),
            ],
        ));
    }

    for file in &inspection.files {
        let output = transform_file(file, permissions);
        required_permissions.extend(output.required_permissions);
        diagnostics.extend(output.diagnostics);
        if output.content == file.content {
            continue;
        }
        let capabilities = output
            .applications
            .iter()
            .map(|application| application.recipe_id.clone())
            .collect::<Vec<_>>();
        recipes.extend(output.applications);
        mutations.push(PlannedFileMutation {
            path: file.path.clone(),
            action: MutationAction::Write,
            before_digest: Some(digest_text(&file.content)),
            after_digest: Some(digest_text(&output.content)),
            content: Some(output.content),
            reason:
                "Apply deterministic Bun-to-Deno runtime recipes and retire supported Bun residues."
                    .to_owned(),
            capabilities,
        });
    }

    for permission in required_permissions.difference(permissions) {
        diagnostics.push(Diagnostic {
            code: "DENO_PERMISSION_REQUIRED".to_owned(),
            severity: DiagnosticSeverity::Error,
            summary: format!(
                "The planned runtime recipes require the explicit Deno '{permission}' permission."
            ),
            blocking: true,
            evidence: vec![EvidenceDetail {
                location: "runtime-plan".to_owned(),
                detail: format!("required permission: {permission}"),
            }],
            remediation: vec![format!(
                "Retry with --deno-permission {permission} after reviewing the access requirement."
            )],
        });
    }
    if !inspection.bun_evidence.is_empty() && mutations.is_empty() {
        diagnostics.push(Diagnostic::blocking(
            "RUNTIME_NO_SAFE_RECIPES",
            "Bun runtime evidence was detected, but no safe deterministic mutation was available.",
            vec!["Resolve the reported unsupported APIs before retrying.".to_owned()],
        ));
    }

    diagnostics = deduplicate_diagnostics(diagnostics);
    recipes
        .sort_by(|left, right| (&left.path, &left.recipe_id).cmp(&(&right.path, &right.recipe_id)));
    mutations.sort_by(|left, right| left.path.cmp(&right.path));
    let operations = if mutations.is_empty() {
        Vec::new()
    } else {
        let paths = mutations
            .iter()
            .map(|mutation| mutation.path.clone())
            .collect::<Vec<_>>();
        vec![PlannedOperation {
            id: "runtime.apply-recipes".to_owned(),
            phase: "apply".to_owned(),
            kind: "runtime.recipe-application".to_owned(),
            description: "Apply digest-bound Bun-to-Deno source, script, and type recipes."
                .to_owned(),
            paths,
            command: Vec::new(),
            timeout_seconds: None,
            capabilities: recipes
                .iter()
                .map(|recipe| recipe.recipe_id.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            side_effect: SideEffect::RepositoryWrite,
            reversible: true,
            preconditions: vec![
                "Every mutation target still matches its reviewed beforeDigest.".to_owned(),
                "The runtime repository fingerprint still matches the plan baseline.".to_owned(),
            ],
            postconditions: vec![
                "Every mutation target matches its planned afterDigest.".to_owned(),
                "No Bun runtime residue remains in the supported inspection boundary.".to_owned(),
            ],
            mutations,
        }]
    };
    let executable = !operations.is_empty() && !diagnostics.iter().any(|entry| entry.blocking);
    let permission_values = permissions.iter().copied().collect::<Vec<_>>();
    let plan_id = short_digest(
        "runtime_plan_",
        &(
            SCHEMA_VERSION,
            "bun",
            "deno",
            &inspection.fingerprint,
            &permission_values,
            &recipes,
            &operations,
            &diagnostics,
        ),
    )?;
    Ok(RuntimeMigrationPlan {
        schema_version: SCHEMA_VERSION.to_owned(),
        plan_id,
        executable,
        source: "bun".to_owned(),
        target: "deno".to_owned(),
        repository_fingerprint: inspection.fingerprint,
        permissions: permission_values,
        recipes,
        operations,
        diagnostics,
        verification: vec![
            "planned-after-digests".to_owned(),
            "bun-runtime-residue".to_owned(),
        ],
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fs;

    use super::create_runtime_plan;
    use crate::runtime::model::DenoPermission;

    #[test]
    fn permission_is_part_of_executability_and_identity() {
        let root = tempfile::tempdir().expect("temporary directory");
        fs::create_dir(root.path().join("src")).expect("source directory");
        fs::write(
            root.path().join("src/index.ts"),
            "import { Hono } from \"hono\";\nconst app = new Hono();\nBun.serve({ port: 3000, fetch: app.fetch });\n",
        )
        .expect("source");
        let blocked = create_runtime_plan(root.path(), &BTreeSet::new()).expect("blocked plan");
        let executable = create_runtime_plan(root.path(), &BTreeSet::from([DenoPermission::Net]))
            .expect("executable plan");
        assert!(!blocked.executable);
        assert!(executable.executable);
        assert_ne!(blocked.plan_id, executable.plan_id);
    }

    #[test]
    fn oversized_inputs_block_without_claiming_no_bun_source() {
        let root = tempfile::tempdir().expect("temporary directory");
        fs::write(root.path().join("src.ts"), vec![b'x'; 512_001]).expect("large source");
        let plan = create_runtime_plan(root.path(), &BTreeSet::new()).expect("runtime plan");
        assert!(!plan.executable);
        assert!(
            plan.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "RUNTIME_SOURCE_FILE_TOO_LARGE")
        );
        assert!(
            plan.diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != "RUNTIME_BUN_SOURCE_NOT_DETECTED")
        );
    }
}
