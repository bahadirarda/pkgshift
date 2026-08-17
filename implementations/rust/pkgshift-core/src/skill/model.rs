use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::model::Diagnostic;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkillScope {
    Project,
    User,
}

impl fmt::Display for SkillScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Project => "project",
            Self::User => "user",
        })
    }
}

impl FromStr for SkillScope {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "project" => Ok(Self::Project),
            "user" => Ok(Self::User),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkillClient {
    Codex,
    Claude,
}

impl SkillClient {
    pub(crate) fn directory(self) -> &'static str {
        match self {
            Self::Codex => ".agents",
            Self::Claude => ".claude",
        }
    }
}

impl fmt::Display for SkillClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
        })
    }
}

impl FromStr for SkillClient {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "codex" => Ok(Self::Codex),
            "claude" => Ok(Self::Claude),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkillInstallMode {
    Copy,
    Link,
}

impl fmt::Display for SkillInstallMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Copy => "copy",
            Self::Link => "link",
        })
    }
}

impl FromStr for SkillInstallMode {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "copy" => Ok(Self::Copy),
            "link" => Ok(Self::Link),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillStatus {
    pub schema_version: String,
    pub name: String,
    pub client: SkillClient,
    pub scope: SkillScope,
    pub source_path: String,
    pub target_path: String,
    pub source_digest: Option<String>,
    pub installed_digest: Option<String>,
    pub installed: bool,
    pub mode: Option<SkillInstallMode>,
    pub healthy: bool,
    pub modified: bool,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkillOperation {
    Install,
    Status,
    Doctor,
    Uninstall,
}

impl fmt::Display for SkillOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Install => "install",
            Self::Status => "status",
            Self::Doctor => "doctor",
            Self::Uninstall => "uninstall",
        })
    }
}
