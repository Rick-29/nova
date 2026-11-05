//! `release.toml` schema.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub project: Project,
    #[serde(default)]
    pub check: Check,
    /// Extra files whose version must match `Cargo.toml` (not tied to a target).
    #[serde(default, rename = "version_file")]
    pub version_files: Vec<VersionFile>,
    #[serde(default, rename = "target")]
    pub targets: Vec<Target>,
    #[serde(default, rename = "publisher")]
    pub publishers: Vec<Publisher>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    /// Crate whose `Cargo.toml` version is the single source of truth.
    #[serde(rename = "crate")]
    pub crate_name: String,
    /// Base name of the native library (`nova_ta` -> `nova_ta.dll`, `libnova_ta.so`, ...).
    pub lib_name: String,
    #[serde(default = "default_branch")]
    pub branch: String,
    /// Every remote listed here must contain the exact commit being released.
    pub remotes: Vec<String>,
    #[serde(default = "default_tag")]
    pub tag: String,
    #[serde(default = "default_dist")]
    pub dist: String,
    /// Optional CHANGELOG with `## x.y.z` / `## [x.y.z]` headings used for release notes.
    pub changelog: Option<String>,
}

fn default_branch() -> String {
    "master".into()
}
fn default_tag() -> String {
    "v{version}".into()
}
fn default_dist() -> String {
    "dist".into()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    #[serde(default = "yes")]
    pub fmt: bool,
    #[serde(default = "default_clippy")]
    pub clippy: String,
    #[serde(default = "default_test")]
    pub test: String,
    #[serde(default = "default_doc")]
    pub doc: String,
    /// `cargo publish --workspace --dry-run` as part of `check`.
    #[serde(default = "yes")]
    pub publish_dry_run: bool,
    /// Extra commands run after the built-in gate (each must exit 0).
    #[serde(default)]
    pub extra: Vec<Step>,
}

impl Default for Check {
    fn default() -> Self {
        Self {
            fmt: true,
            clippy: default_clippy(),
            test: default_test(),
            doc: default_doc(),
            publish_dry_run: true,
            extra: Vec::new(),
        }
    }
}

fn yes() -> bool {
    true
}
fn default_clippy() -> String {
    "cargo clippy --workspace --all-targets --all-features -- -D warnings".into()
}
fn default_test() -> String {
    "cargo test --workspace --all-features".into()
}
fn default_doc() -> String {
    "cargo doc --workspace --no-deps --all-features --lib".into()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionFile {
    pub path: String,
    /// Literal text containing `{version}`, e.g. `version = "{version}"`.
    pub pattern: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub name: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Subset of `windows`, `linux`, `macos`. Empty = everywhere.
    #[serde(default)]
    pub only_on: Vec<String>,
    /// Executables that must be on PATH for this target.
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub version_files: Vec<VersionFile>,
    #[serde(default)]
    pub steps: Vec<Step>,
    /// Globs that must each match at least one file after the steps ran.
    #[serde(default)]
    pub artifacts: Vec<String>,
    /// Directory zipped into `dist/`. Defaults to `{out}`; `false` disables.
    #[serde(default)]
    pub archive: Archive,
    /// Globs of files copied as-is into `dist/` (wheels, jars, ...).
    #[serde(default)]
    pub dist: Vec<String>,
    /// Commands run by `publish` for this target (registry uploads).
    #[serde(default)]
    pub publish: Vec<Step>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Archive {
    Toggle(bool),
    Dir(String),
}

impl Default for Archive {
    fn default() -> Self {
        Archive::Toggle(true)
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Step {
    Run(String),
    Detailed(StepSpec),
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct StepSpec {
    pub run: Option<String>,
    /// Copy a file or directory (recursively) to `to`. A trailing `/` on `to` means "into".
    pub copy: Option<String>,
    pub to: Option<String>,
    pub mkdir: Option<String>,
    pub remove: Option<String>,
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub only_on: Vec<String>,
    /// Missing source for `copy` / non-zero exit for `run` is a warning, not an error.
    #[serde(default)]
    pub optional: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Publisher {
    pub name: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub only_on: Vec<String>,
    /// Executables that must be on PATH.
    #[serde(default)]
    pub requires: Vec<String>,
    /// Environment variables that must be set.
    #[serde(default)]
    pub env: Vec<String>,
    /// Optional glob; its matches are exposed to the steps as `{files}`.
    pub files: Option<String>,
    pub steps: Vec<Step>,
}

impl Config {
    pub fn load(path: &str) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
        let cfg: Config = toml::from_str(&text).with_context(|| format!("parsing {path}"))?;
        let mut seen = std::collections::BTreeSet::new();
        for t in &cfg.targets {
            anyhow::ensure!(seen.insert(&t.name), "duplicate target `{}`", t.name);
        }
        Ok(cfg)
    }
}
