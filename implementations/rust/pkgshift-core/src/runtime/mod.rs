mod command;
mod inspect;
mod lex;
mod model;
mod plan;
mod recipe;
mod transaction;

pub(crate) use command::{rollback_command, to_command};
pub use model::DenoPermission;
pub(crate) use model::RuntimeRunArtifact;

pub(crate) fn load_run_artifact(
    state_directory: &std::path::Path,
    run_id: &str,
) -> crate::util::Result<RuntimeRunArtifact> {
    transaction::load_run(state_directory, run_id).map(|run| RuntimeRunArtifact::from(&run))
}
