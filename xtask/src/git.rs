//! Git preflight: clean tree, right branch, every remote has this commit, tag is free.

use anyhow::{Context, Result, bail};

use crate::config::Project;
use crate::shell::{header, note, output, try_output, warn};

pub fn preflight(project: &Project, tag: &str) -> Result<()> {
    header("git preflight");

    let dirty = output("git status --porcelain")?;
    if !dirty.is_empty() {
        bail!("working tree is not clean; commit or stash first:\n{dirty}");
    }
    note("working tree clean");

    let branch = output("git rev-parse --abbrev-ref HEAD")?;
    if branch != project.branch {
        bail!(
            "on branch `{branch}`, releases are cut from `{}`",
            project.branch
        );
    }
    let head = output("git rev-parse HEAD")?;
    note(&format!("branch {branch} @ {}", &head[..12]));

    for remote in &project.remotes {
        output(&format!("git fetch --quiet {remote}"))
            .with_context(|| format!("fetching remote `{remote}`"))?;
        let upstream = format!("{remote}/{}", project.branch);
        let Some(unpushed) = try_output(&format!("git rev-list --count {upstream}..HEAD")) else {
            bail!("`{upstream}` does not exist; push the branch to `{remote}` first");
        };
        if unpushed != "0" {
            bail!(
                "{unpushed} commit(s) not pushed to `{upstream}` (run `git push {remote} {branch}`)"
            );
        }
        let behind = output(&format!("git rev-list --count HEAD..{upstream}"))?;
        if behind != "0" {
            bail!("`{upstream}` is {behind} commit(s) ahead of HEAD; pull first");
        }
        note(&format!("{upstream} is at HEAD"));
    }

    if !output(&format!("git tag --list {tag}"))?.is_empty() {
        bail!(
            "tag `{tag}` already exists locally; bump the version first (`cargo xtask bump <version>`)"
        );
    }
    for remote in &project.remotes {
        let found = output(&format!("git ls-remote --tags {remote} refs/tags/{tag}"))?;
        if !found.is_empty() {
            bail!("tag `{tag}` already exists on `{remote}`; bump the version first");
        }
    }
    note(&format!("tag {tag} is free"));
    Ok(())
}

pub fn previous_tag() -> Option<String> {
    try_output("git describe --tags --abbrev=0").filter(|s| !s.is_empty())
}

/// `- subject` lines since the previous tag (or the whole history).
pub fn log_since(prev: Option<&str>) -> Result<String> {
    let range = prev
        .map(|p| format!("{p}..HEAD"))
        .unwrap_or_else(|| "HEAD".into());
    output(&format!(
        "git log --no-merges --pretty=format:\"- %s\" {range}"
    ))
}

pub fn create_tag(tag: &str, message: &str, dry_run: bool) -> Result<()> {
    let cmd = format!("git tag --annotate {tag} --message \"{message}\"");
    if dry_run {
        note(&format!("[dry-run] {cmd}"));
        return Ok(());
    }
    crate::shell::run(&cmd, &Default::default(), None)
}

pub fn push(project: &Project, tag: &str, dry_run: bool) -> Result<()> {
    for remote in &project.remotes {
        let cmd = format!("git push {remote} {} {tag}", project.branch);
        if dry_run {
            note(&format!("[dry-run] {cmd}"));
        } else if let Err(e) = crate::shell::run(&cmd, &Default::default(), None) {
            warn(&format!(
                "{e}. The tag exists locally; push it by hand once the remote is reachable."
            ));
            return Err(e);
        }
    }
    Ok(())
}
