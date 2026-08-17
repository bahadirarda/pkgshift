use std::str::FromStr;

use crate::model::{
    ExecutableRequirement, NativeImportMode, NativeImportStrategy, PackageManagerId, SupportTier,
};

#[derive(Debug, Clone, Copy)]
pub struct PackageManagerDefinition {
    pub id: PackageManagerId,
    pub display_name: &'static str,
    pub tier: SupportTier,
    pub aliases: &'static [&'static str],
    pub lockfiles: &'static [&'static str],
    pub configuration_files: &'static [&'static str],
    pub install_command: &'static [&'static str],
    pub package_manager_pin: &'static str,
}

pub const PACKAGE_MANAGERS: &[PackageManagerDefinition] = &[
    PackageManagerDefinition {
        id: PackageManagerId::Npm,
        display_name: "npm",
        tier: SupportTier::ProductionTarget,
        aliases: &["npm"],
        lockfiles: &["npm-shrinkwrap.json", "package-lock.json"],
        configuration_files: &[".npmrc"],
        install_command: &["npm", "install", "--ignore-scripts"],
        package_manager_pin: "npm@12.0.2",
    },
    PackageManagerDefinition {
        id: PackageManagerId::Pnpm,
        display_name: "pnpm",
        tier: SupportTier::ProductionTarget,
        aliases: &["pnpm"],
        lockfiles: &["pnpm-lock.yaml"],
        configuration_files: &["pnpm-workspace.yaml", ".npmrc", ".pnpmfile.cjs"],
        install_command: &["pnpm", "install", "--ignore-scripts"],
        package_manager_pin: "pnpm@11.21.0",
    },
    PackageManagerDefinition {
        id: PackageManagerId::YarnClassic,
        display_name: "Yarn Classic",
        tier: SupportTier::ProductionTarget,
        aliases: &["yarn-classic", "yarn@1"],
        lockfiles: &["yarn.lock"],
        configuration_files: &[".yarnrc", ".npmrc"],
        install_command: &["yarn", "install", "--ignore-scripts"],
        package_manager_pin: "yarn@1.22.22",
    },
    PackageManagerDefinition {
        id: PackageManagerId::YarnModern,
        display_name: "Yarn Modern",
        tier: SupportTier::ProductionTarget,
        aliases: &["yarn-modern", "yarn-berry"],
        lockfiles: &["yarn.lock"],
        configuration_files: &[".yarnrc.yml", ".pnp.cjs", ".pnp.loader.mjs"],
        install_command: &["yarn", "install", "--mode=skip-build"],
        package_manager_pin: "yarn@4.18.0",
    },
    PackageManagerDefinition {
        id: PackageManagerId::Bun,
        display_name: "Bun",
        tier: SupportTier::ProductionTarget,
        aliases: &["bun"],
        lockfiles: &["bun.lock", "bun.lockb"],
        configuration_files: &["bunfig.toml", ".npmrc"],
        install_command: &["bun", "install", "--ignore-scripts"],
        package_manager_pin: "bun@1.3.14",
    },
    PackageManagerDefinition {
        id: PackageManagerId::Vlt,
        display_name: "vlt",
        tier: SupportTier::ProductionTarget,
        aliases: &["vlt"],
        lockfiles: &["vlt-lock.json"],
        configuration_files: &["vlt.json"],
        install_command: &["vlt", "install"],
        package_manager_pin: "vlt@1.0.2",
    },
    PackageManagerDefinition {
        id: PackageManagerId::Deno,
        display_name: "Deno dependency mode",
        tier: SupportTier::ProductionTarget,
        aliases: &["deno"],
        lockfiles: &["deno.lock"],
        configuration_files: &["deno.json", "deno.jsonc", ".npmrc"],
        install_command: &["deno", "install"],
        package_manager_pin: "deno@2.9.5",
    },
];

pub fn get_package_manager(id: PackageManagerId) -> &'static PackageManagerDefinition {
    PACKAGE_MANAGERS
        .iter()
        .find(|definition| definition.id == id)
        .expect("every package manager identifier has a catalog entry")
}

pub fn executable_requirement(id: PackageManagerId) -> ExecutableRequirement {
    let definition = get_package_manager(id);
    let program = definition
        .install_command
        .first()
        .expect("every package manager install command has a program");
    let (_, required_version) = definition
        .package_manager_pin
        .rsplit_once('@')
        .expect("every package manager pin includes an exact version");
    ExecutableRequirement {
        program: (*program).to_owned(),
        required_version: required_version.to_owned(),
        version_command: vec![(*program).to_owned(), "--version".to_owned()],
        package_manager_pin: definition.package_manager_pin.to_owned(),
    }
}

pub fn normalize_package_manager_id(value: &str) -> Option<PackageManagerId> {
    PackageManagerId::from_str(value).ok()
}

pub fn native_import_strategy(
    source: PackageManagerId,
    target: PackageManagerId,
    source_lockfile_present: bool,
) -> Option<NativeImportStrategy> {
    if !source_lockfile_present {
        return None;
    }
    let (id, mode, command, summary): (&str, NativeImportMode, &[&str], &str) =
        match (source, target) {
            (
                PackageManagerId::Npm
                | PackageManagerId::YarnClassic
                | PackageManagerId::YarnModern,
                PackageManagerId::Pnpm,
            ) => (
                "pnpm-import",
                NativeImportMode::DedicatedCommand,
                &["pnpm", "import"],
                "Generate pnpm dependency state with pnpm's native lockfile importer.",
            ),
            (
                PackageManagerId::Npm
                | PackageManagerId::YarnClassic
                | PackageManagerId::YarnModern,
                PackageManagerId::Bun,
            ) => (
                "bun-pm-migrate",
                NativeImportMode::DedicatedCommand,
                &["bun", "pm", "migrate"],
                "Generate Bun dependency state with bun pm migrate.",
            ),
            (PackageManagerId::Pnpm, PackageManagerId::Bun) => (
                "bun-pnpm-install-migration",
                NativeImportMode::InstallIntegrated,
                get_package_manager(target).install_command,
                "Use Bun's install-integrated pnpm lockfile migration path.",
            ),
            (PackageManagerId::Npm, PackageManagerId::YarnClassic) => (
                "yarn-classic-import",
                NativeImportMode::DedicatedCommand,
                &["yarn", "import"],
                "Generate Yarn Classic dependency state with yarn import.",
            ),
            (PackageManagerId::YarnClassic, PackageManagerId::YarnModern) => (
                "yarn-modern-install-migration",
                NativeImportMode::InstallIntegrated,
                get_package_manager(target).install_command,
                "Use Yarn Modern's install-integrated Yarn Classic migration path.",
            ),
            (PackageManagerId::YarnClassic, PackageManagerId::Npm) => (
                "npm-yarn-lock-install",
                NativeImportMode::InstallIntegrated,
                get_package_manager(target).install_command,
                "Use npm's yarn.lock-aware installation path.",
            ),
            (PackageManagerId::Npm, PackageManagerId::Deno) => (
                "deno-install-migration",
                NativeImportMode::InstallIntegrated,
                get_package_manager(target).install_command,
                "Use Deno's install-integrated Node dependency migration path.",
            ),
            _ => return None,
        };
    Some(NativeImportStrategy {
        id: id.to_owned(),
        source,
        target,
        mode,
        command: command.iter().map(ToString::to_string).collect(),
        summary: summary.to_owned(),
    })
}
