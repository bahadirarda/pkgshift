use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use pkgshift_core::command::{CommandKind, CommandOptions};
use pkgshift_core::execute;
use pkgshift_core::model::{CommandExecution, CommandStatus};

#[derive(Debug, Parser)]
#[command(
    name = "pkgshift",
    version,
    about = "Deterministic, transactional package manager migrations",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: CliCommand,

    /// Project root. Defaults to the current directory.
    #[arg(long, global = true, default_value = ".")]
    cwd: PathBuf,

    /// Persistent migration state directory. Guided migrations default to .pkgshift/state.
    #[arg(long, global = true)]
    state_dir: Option<PathBuf>,

    /// Emit the stable JSON command result contract.
    #[arg(long, global = true)]
    json: bool,

    /// Accept reviewed lossy capability decisions during planning.
    #[arg(long, global = true)]
    accept_lossy: bool,

    /// Approve a mutating operation using its exact plan or run identifier.
    #[arg(long, global = true)]
    approve: Option<String>,

    /// Plan and report without changing repository files.
    #[arg(long, global = true)]
    dry_run: bool,

    /// Execute the accepted plan in a disposable sandbox and leave the repository unchanged.
    #[arg(long, global = true, conflicts_with = "dry_run")]
    trial: bool,

    /// Disable the guided terminal approval prompt.
    #[arg(long, global = true)]
    non_interactive: bool,

    /// Accepted for stable agent command compatibility; output is currently uncolored.
    #[arg(long, global = true)]
    no_color: bool,
}

#[derive(Debug, Subcommand)]
enum CliCommand {
    /// Inspect, plan, approve, apply, and verify a migration from the project root.
    To {
        target: String,
        /// Run a root package script after migration; repeat to select multiple scripts.
        #[arg(long, value_name = "NAME", action = clap::ArgAction::Append)]
        verify_script: Vec<String>,
    },

    /// Inspect repository package manager evidence and normalized project semantics.
    Inspect {
        #[command(subcommand)]
        subject: InspectSubject,
    },

    /// Create a deterministic, read-only migration plan.
    Plan {
        #[command(subcommand)]
        subject: PlanSubject,
    },

    /// Apply a previously persisted plan.
    Apply { plan_id: String },

    /// Verify a persisted migration run.
    Verify { run_id: String },

    /// Restore repository files from a migration snapshot.
    Rollback { run_id: String },

    /// Report package manager adapters and target tiers.
    Support,

    /// Keyword-compatible package manager planning commands.
    Pm {
        #[command(subcommand)]
        command: PackageManagerCommand,
    },
}

#[derive(Debug, Subcommand)]
enum InspectSubject {
    PackageManager,
}

#[derive(Debug, Subcommand)]
enum PlanSubject {
    PackageManager(TargetArgument),
}

#[derive(Debug, Subcommand)]
enum PackageManagerCommand {
    To {
        target: String,
        /// Run a root package script after migration; repeat to select multiple scripts.
        #[arg(long, value_name = "NAME", action = clap::ArgAction::Append)]
        verify_script: Vec<String>,
    },
}

#[derive(Debug, Args)]
struct TargetArgument {
    #[arg(long)]
    to: String,
    /// Run a root package script after migration; repeat to select multiple scripts.
    #[arg(long, value_name = "NAME", action = clap::ArgAction::Append)]
    verify_script: Vec<String>,
}

fn command_kind(command: CliCommand) -> (CommandKind, Vec<String>) {
    match command {
        CliCommand::To {
            target,
            verify_script,
        } => (CommandKind::To { target }, verify_script),
        CliCommand::Pm {
            command:
                PackageManagerCommand::To {
                    target,
                    verify_script,
                },
        } => (CommandKind::Plan { target }, verify_script),
        CliCommand::Inspect {
            subject: InspectSubject::PackageManager,
        } => (CommandKind::Inspect, Vec::new()),
        CliCommand::Plan {
            subject: PlanSubject::PackageManager(arguments),
        } => (
            CommandKind::Plan {
                target: arguments.to,
            },
            arguments.verify_script,
        ),
        CliCommand::Apply { plan_id } => (CommandKind::Apply { plan_id }, Vec::new()),
        CliCommand::Verify { run_id } => (CommandKind::Verify { run_id }, Vec::new()),
        CliCommand::Rollback { run_id } => (CommandKind::Rollback { run_id }, Vec::new()),
        CliCommand::Support => (CommandKind::Support, Vec::new()),
    }
}

fn human_report(execution: &CommandExecution) {
    let result = &execution.result;
    println!("pkgshift {}: {:?}", result.command, result.status);
    if let Some(plan_id) = &result.plan_id {
        println!("plan: {plan_id}");
    }
    if let Some(run_id) = &result.run_id {
        println!("run: {run_id}");
    }
    for (key, value) in &result.summary {
        let value = value
            .as_str()
            .map_or_else(|| value.to_string(), str::to_owned);
        println!("{key}: {value}");
    }
    for diagnostic in &result.diagnostics {
        eprintln!("{}: {}", diagnostic.code, diagnostic.summary);
        for remediation in &diagnostic.remediation {
            eprintln!("  {remediation}");
        }
    }
    if result.status == CommandStatus::Planned
        && let Some(action) = result.next_actions.first()
    {
        println!("next: {}", action.argv.join(" "));
    }
}

fn interactive_approval(execution: &CommandExecution) -> io::Result<bool> {
    let Some(plan_id) = execution.result.plan_id.as_deref() else {
        return Ok(false);
    };
    let source = execution
        .result
        .summary
        .get("source")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let target = execution
        .result
        .summary
        .get("target")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let trial = execution
        .result
        .summary
        .get("trial")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if trial {
        eprintln!("Plan {plan_id} will trial {source} to {target} in an isolated sandbox.");
        eprint!("Run this exact trial plan? [y/N] ");
    } else {
        eprintln!("Plan {plan_id} will migrate {source} to {target}.");
        eprint!("Apply this exact plan? [y/N] ");
    }
    io::stderr().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let is_guided = matches!(cli.command, CliCommand::To { .. });
    let (command, verification_scripts) = command_kind(cli.command);
    let mut options = CommandOptions::new(command, cli.cwd);
    options.state_directory = cli.state_dir;
    options.accept_lossy = cli.accept_lossy;
    options.approval = cli.approve;
    options.dry_run = cli.dry_run;
    options.trial = cli.trial;
    options.verification_scripts = verification_scripts;

    let mut execution = execute(&options);
    let can_prompt = is_guided
        && !cli.json
        && !cli.non_interactive
        && !options.dry_run
        && options.approval.is_none()
        && io::stdin().is_terminal()
        && execution.exit_code == 7;
    if can_prompt {
        if interactive_approval(&execution).unwrap_or(false) {
            options.approval = execution.result.plan_id.clone();
            execution = execute(&options);
        } else {
            eprintln!("No repository files were changed.");
        }
    }

    if cli.json {
        match serde_json::to_string_pretty(&execution.result) {
            Ok(output) => println!("{output}"),
            Err(error) => {
                eprintln!("pkgshift could not serialize its result: {error}");
                return ExitCode::from(8);
            }
        }
    } else {
        human_report(&execution);
    }
    ExitCode::from(execution.exit_code)
}
