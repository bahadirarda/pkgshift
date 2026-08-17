use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EdgeEquivalencePolicy {
    #[default]
    Compatible,
    Strict,
}

impl fmt::Display for EdgeEquivalencePolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Compatible => "compatible",
            Self::Strict => "strict",
        })
    }
}

impl FromStr for EdgeEquivalencePolicy {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "compatible" => Ok(Self::Compatible),
            "strict" => Ok(Self::Strict),
            _ => Err("edge equivalence must be 'compatible' or 'strict'".to_owned()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetPlatform {
    pub os: String,
    pub cpu: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub libc: Option<String>,
}

impl fmt::Display for TargetPlatform {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.os, self.cpu)?;
        if let Some(libc) = &self.libc {
            write!(formatter, "/{libc}")?;
        }
        Ok(())
    }
}

fn supported(value: &str, values: &[&str], kind: &str) -> Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    if values.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        Err(format!(
            "unsupported target {kind} '{value}'; supported values: {}",
            values.join(", ")
        ))
    }
}

impl FromStr for TargetPlatform {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        const OS: &[&str] = &[
            "aix", "android", "darwin", "freebsd", "linux", "openbsd", "sunos", "win32",
        ];
        const CPU: &[&str] = &[
            "arm", "arm64", "ia32", "loong64", "ppc64", "riscv64", "s390x", "x64",
        ];
        const LIBC: &[&str] = &["glibc", "musl"];
        let components = value.split('/').collect::<Vec<_>>();
        if !(2..=3).contains(&components.len()) {
            return Err("target platform must use OS/CPU or OS/CPU/LIBC".to_owned());
        }
        let os = supported(components[0], OS, "operating system")?;
        let cpu = supported(components[1], CPU, "CPU")?;
        let libc = components
            .get(2)
            .map(|value| supported(value, LIBC, "libc"))
            .transpose()?;
        if libc.is_some() && os != "linux" {
            return Err("a libc may only be selected for a Linux target".to_owned());
        }
        Ok(Self { os, cpu, libc })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationPolicy {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_platforms: Vec<TargetPlatform>,
    #[serde(default)]
    pub edge_equivalence: EdgeEquivalencePolicy,
}

impl VerificationPolicy {
    pub fn normalized(
        target_platforms: impl IntoIterator<Item = TargetPlatform>,
        edge_equivalence: EdgeEquivalencePolicy,
    ) -> Self {
        let mut target_platforms = target_platforms.into_iter().collect::<Vec<_>>();
        target_platforms.sort();
        target_platforms.dedup();
        Self {
            target_platforms,
            edge_equivalence,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackagePlatformConstraint {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub os: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cpu: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub libc: Vec<String>,
}

fn value_allowed(value: Option<&str>, constraints: &[String]) -> bool {
    if constraints.is_empty() {
        return true;
    }
    if value.is_some_and(|value| {
        constraints
            .iter()
            .any(|constraint| constraint.strip_prefix('!') == Some(value))
    }) {
        return false;
    }
    let positive = constraints
        .iter()
        .filter(|constraint| !constraint.starts_with('!'))
        .collect::<Vec<_>>();
    positive.is_empty()
        || value.is_some_and(|value| {
            positive
                .iter()
                .any(|constraint| constraint.as_str() == value)
        })
}

impl PackagePlatformConstraint {
    pub(crate) fn is_empty(&self) -> bool {
        self.os.is_empty() && self.cpu.is_empty() && self.libc.is_empty()
    }

    pub(crate) fn allows(&self, target: &TargetPlatform) -> bool {
        value_allowed(Some(&target.os), &self.os)
            && value_allowed(Some(&target.cpu), &self.cpu)
            && value_allowed(target.libc.as_deref(), &self.libc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_normalizes_and_deduplicates_target_platforms() {
        let linux: TargetPlatform = "linux/x64/glibc".parse().expect("Linux target");
        let policy = VerificationPolicy::normalized(
            [
                linux.clone(),
                "darwin/arm64".parse().expect("macOS target"),
                linux,
            ],
            EdgeEquivalencePolicy::Strict,
        );
        assert_eq!(policy.target_platforms.len(), 2);
        assert_eq!(policy.edge_equivalence, EdgeEquivalencePolicy::Strict);
        assert_eq!(policy.target_platforms[1].to_string(), "linux/x64/glibc");
    }

    #[test]
    fn applies_positive_and_negative_package_constraints() {
        let constraint = PackagePlatformConstraint {
            os: vec!["linux".to_owned(), "!android".to_owned()],
            cpu: vec!["x64".to_owned()],
            libc: vec!["!musl".to_owned()],
        };
        assert!(constraint.allows(&"linux/x64/glibc".parse().expect("target")));
        assert!(!constraint.allows(&"linux/arm64/glibc".parse().expect("target")));
        assert!(!constraint.allows(&"linux/x64/musl".parse().expect("target")));
    }
}
