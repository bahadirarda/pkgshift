use std::path::Path;

use crate::inspect::{build_project_ir, inspect_project};
use crate::lock_graph::extract_lock_graph;
use crate::model::{LockGraph, ProjectInspection, ProjectIr};
use crate::util::Result;

pub(crate) struct ReadinessContext {
    pub inspection: ProjectInspection,
    pub project_ir: Option<ProjectIr>,
    pub source_lock_graph: Option<LockGraph>,
}

pub(crate) fn load_context(cwd: &Path) -> Result<ReadinessContext> {
    let inspection = inspect_project(cwd)?;
    let project_ir = build_project_ir(&inspection)?;
    let source_lock_graph = match project_ir.as_ref().and_then(|project| project.source) {
        Some(source) => extract_lock_graph(Path::new(&inspection.root), source)?,
        None => None,
    };
    Ok(ReadinessContext {
        inspection,
        project_ir,
        source_lock_graph,
    })
}
