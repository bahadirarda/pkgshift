use std::str::FromStr;

use crate::model::{PackageManagerId, SupportTier};

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
        lockfiles: &["package-lock.json", "npm-shrinkwrap.json"],
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
        install_command: &["yarn", "install", "--mode=skip-builds"],
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
        tier: SupportTier::PreviewTarget,
        aliases: &["vlt"],
        lockfiles: &["vlt-lock.json"],
        configuration_files: &["vlt.json", ".npmrc"],
        install_command: &["vlt", "install", "--ignore-scripts"],
        package_manager_pin: "vlt@1.0.2",
    },
    PackageManagerDefinition {
        id: PackageManagerId::Deno,
        display_name: "Deno dependency mode",
        tier: SupportTier::PreviewTarget,
        aliases: &["deno"],
        lockfiles: &["deno.lock"],
        configuration_files: &["deno.json", "deno.jsonc"],
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

pub fn normalize_package_manager_id(value: &str) -> Option<PackageManagerId> {
    PackageManagerId::from_str(value).ok()
}
