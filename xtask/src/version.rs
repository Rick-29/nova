//! Single-source versioning: `Cargo.toml` is the truth, every other manifest must agree.

use std::fs;

use anyhow::{Context, Result, bail};
use regex::Regex;

use crate::config::VersionFile;
use crate::shell::note;

const SEMVER: &str = r"([0-9A-Za-z.+-]+)";

pub fn cargo_version(manifest: &str) -> Result<String> {
    let text = fs::read_to_string(manifest).with_context(|| format!("reading {manifest}"))?;
    let doc: toml::Value = toml::from_str(&text).with_context(|| format!("parsing {manifest}"))?;
    doc.get("package")
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .with_context(|| format!("{manifest} has no [package].version"))
}

/// Rewrites `[package].version` in `manifest`, leaving the rest of the file untouched.
pub fn set_cargo_version(manifest: &str, version: &str) -> Result<()> {
    let text = fs::read_to_string(manifest)?;
    let re = Regex::new(r#"(?m)^(\[package\][^\[]*?^version\s*=\s*")([^"]+)(")"#)?;
    anyhow::ensure!(
        re.is_match(&text),
        "cannot find [package].version in {manifest}"
    );
    let out = re.replace(&text, |c: &regex::Captures| {
        format!("{}{version}{}", &c[1], &c[3])
    });
    fs::write(manifest, out.as_ref())?;
    note(&format!("{manifest}: version = \"{version}\""));
    Ok(())
}

fn pattern_regex(pattern: &str) -> Result<Regex> {
    anyhow::ensure!(
        pattern.contains("{version}"),
        "pattern `{pattern}` lacks `{{version}}`"
    );
    let parts: Vec<String> = pattern.split("{version}").map(regex::escape).collect();
    Regex::new(&parts.join(SEMVER)).context("building version pattern")
}

/// Checks (or, with `sync`, rewrites) every version file. Returns the number of mismatches fixed.
pub fn check_files(files: &[VersionFile], version: &str, sync: bool) -> Result<usize> {
    let mut problems = Vec::new();
    let mut fixed = 0;
    for vf in files {
        let re = pattern_regex(&vf.pattern)?;
        let text = fs::read_to_string(&vf.path).with_context(|| format!("reading {}", vf.path))?;
        let found: Vec<&str> = re
            .captures_iter(&text)
            .map(|c| c.get(1).unwrap().as_str())
            .collect();
        if found.is_empty() {
            problems.push(format!("{}: pattern `{}` not found", vf.path, vf.pattern));
            continue;
        }
        let wrong: Vec<&str> = found.iter().copied().filter(|v| *v != version).collect();
        if wrong.is_empty() {
            note(&format!("{}: {version}", vf.path));
        } else if sync {
            let replacement = vf.pattern.replace("{version}", version);
            let out = re.replace_all(&text, replacement.as_str());
            fs::write(&vf.path, out.as_ref())?;
            note(&format!(
                "{}: {} -> {version} (updated)",
                vf.path,
                wrong.join(", ")
            ));
            fixed += 1;
        } else {
            problems.push(format!(
                "{}: found {} but Cargo.toml says {version}",
                vf.path,
                wrong.join(", ")
            ));
        }
    }
    if !problems.is_empty() {
        bail!(
            "version mismatch (run `cargo xtask check --sync-versions` to fix):\n  {}",
            problems.join("\n  ")
        );
    }
    Ok(fixed)
}

/// Published version on crates.io, if the crate exists there.
pub fn crates_io_version(name: &str) -> Option<String> {
    let out = crate::shell::try_output(&format!("cargo search {name} --limit 1"))?;
    let line = out.lines().next()?;
    let (found, rest) = line.split_once(" = \"")?;
    (found == name).then(|| rest.split('"').next().unwrap_or_default().to_string())
}
