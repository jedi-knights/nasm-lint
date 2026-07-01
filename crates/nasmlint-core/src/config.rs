//! `.nasmlint.toml` — per-rule enable/disable, severity overrides, and ignore
//! globs. Modeled after ruff/clippy: rules are on by default and the config
//! selectively silences or re-grades them.
//!
//! Example:
//! ```toml
//! # Silence a rule entirely, and downgrade another.
//! [rules]
//! NL053 = "off"       # stop flagging trailing whitespace
//! NL050 = "consider"  # tabs/spaces mix is only advisory in this repo
//!
//! # Paths excluded from analysis (glob syntax).
//! ignore = ["vendor/**", "third_party/**"]
//! ```

use std::collections::HashMap;

use serde::Deserialize;

use crate::diagnostics::Severity;

/// How a single rule is configured. `Off` disables it; the others pin its
/// severity, overriding the rule's built-in default.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RuleSetting {
    Off,
    #[serde(rename = "must-fix", alias = "error")]
    MustFix,
    #[serde(rename = "should-fix", alias = "warning")]
    ShouldFix,
    #[serde(rename = "consider", alias = "note")]
    Consider,
}

impl RuleSetting {
    fn as_severity(self) -> Option<Severity> {
        match self {
            RuleSetting::Off => None,
            RuleSetting::MustFix => Some(Severity::MustFix),
            RuleSetting::ShouldFix => Some(Severity::ShouldFix),
            RuleSetting::Consider => Some(Severity::Consider),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Per-rule overrides keyed by `NL0xx` code. Absent codes use their default.
    pub rules: HashMap<String, RuleSetting>,
    /// Glob patterns for files to skip entirely.
    pub ignore: Vec<String>,
}

impl Config {
    /// Parse a config from TOML text.
    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// Whether a rule should run. Rules are enabled unless explicitly `off`.
    pub fn is_enabled(&self, code: &str) -> bool {
        self.rules.get(code) != Some(&RuleSetting::Off)
    }

    /// Effective severity for a rule's findings: the configured override if any,
    /// otherwise the rule's built-in `default`.
    pub fn severity_for(&self, code: &str, default: Severity) -> Severity {
        self.rules
            .get(code)
            .and_then(|s| s.as_severity())
            .unwrap_or(default)
    }
}
