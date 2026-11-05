//! Executes `release.toml` steps and packs `dist/` archives.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use zip::write::SimpleFileOptions;

use crate::config::{Step, StepSpec};
use crate::shell::{Vars, note, run, runs_here, warn};

/// Human-readable form of a step with placeholders expanded (used by `--dry-run`).
pub fn describe(step: &Step, vars: &Vars) -> String {
    let text = match step {
        Step::Run(cmd) => format!("$ {cmd}"),
        Step::Detailed(s) => [
            s.run.as_ref().map(|c| format!("$ {c}")),
            s.copy
                .as_ref()
                .map(|c| format!("copy {c} -> {}", s.to.as_deref().unwrap_or("?"))),
            s.mkdir.as_ref().map(|d| format!("mkdir {d}")),
            s.remove.as_ref().map(|r| format!("remove {r}")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("; "),
    };
    vars.expand(&text).unwrap_or(text)
}

pub fn run_steps(steps: &[Step], vars: &Vars) -> Result<()> {
    for step in steps {
        match step {
            Step::Run(cmd) => run(&vars.expand(cmd)?, &Default::default(), None)?,
            Step::Detailed(spec) => run_spec(spec, vars)?,
        }
    }
    Ok(())
}

fn run_spec(spec: &StepSpec, vars: &Vars) -> Result<()> {
    if !runs_here(&spec.only_on) {
        note(&format!(
            "skipping step (only on {})",
            spec.only_on.join(", ")
        ));
        return Ok(());
    }
    let cwd = spec
        .cwd
        .as_deref()
        .map(|c| vars.expand(c))
        .transpose()?
        .map(PathBuf::from);
    if let Some(dir) = &spec.mkdir {
        let dir = vars.expand(dir)?;
        note(&format!("mkdir {dir}"));
        fs::create_dir_all(&dir).with_context(|| format!("creating {dir}"))?;
    }
    if let Some(path) = &spec.remove {
        let path = vars.expand(path)?;
        note(&format!("remove {path}"));
        let p = Path::new(&path);
        if p.is_dir() {
            fs::remove_dir_all(p)?;
        } else if p.exists() {
            fs::remove_file(p)?;
        }
    }
    if let Some(src) = &spec.copy {
        let src = vars.expand(src)?;
        let to = vars.expand(spec.to.as_deref().context("`copy` needs `to`")?)?;
        copy_any(&src, &to, spec.optional)?;
    }
    if let Some(cmd) = &spec.run {
        let cmd = vars.expand(cmd)?;
        let mut env = std::collections::BTreeMap::new();
        for (k, v) in &spec.env {
            env.insert(k.clone(), vars.expand(v)?);
        }
        match run(&cmd, &env, cwd.as_deref()) {
            Ok(()) => {}
            Err(e) if spec.optional => warn(&format!("{e} (optional step, continuing)")),
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Copies a file or a directory tree. `to` ending in `/` means "into that directory".
pub fn copy_any(src: &str, to: &str, optional: bool) -> Result<()> {
    let src_path = Path::new(src);
    if !src_path.exists() {
        if optional {
            warn(&format!("{src} does not exist (optional copy, skipped)"));
            return Ok(());
        }
        bail!("cannot copy `{src}`: it does not exist");
    }
    let into = to.ends_with('/') || to.ends_with('\\') || Path::new(to).is_dir();
    let dest: PathBuf = if into {
        let name = src_path
            .file_name()
            .context("copy source has no file name")?;
        Path::new(to).join(name)
    } else {
        PathBuf::from(to)
    };
    note(&format!("copy {src} -> {}", dest.display()));
    if src_path.is_dir() {
        copy_tree(src_path, &dest)
    } else {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src_path, &dest).with_context(|| format!("copying {src}"))?;
        Ok(())
    }
}

fn copy_tree(src: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let target = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Expands a glob and insists on at least one match.
pub fn glob_required(pattern: &str) -> Result<Vec<PathBuf>> {
    let paths: Vec<PathBuf> = glob::glob(pattern)
        .with_context(|| format!("bad glob `{pattern}`"))?
        .filter_map(Result::ok)
        .collect();
    if paths.is_empty() {
        bail!("expected artifact `{pattern}` was not produced");
    }
    Ok(paths)
}

/// Zips `dir` (contents at the archive root) into `out`.
pub fn zip_dir(dir: &Path, out: &Path) -> Result<()> {
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = fs::File::create(out).with_context(|| format!("creating {}", out.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut count = 0;
    zip_walk(&mut zip, dir, dir, opts, &mut count)?;
    zip.finish()?;
    note(&format!("{} ({count} files)", out.display()));
    Ok(())
}

fn zip_walk(
    zip: &mut zip::ZipWriter<fs::File>,
    root: &Path,
    dir: &Path,
    opts: SimpleFileOptions,
    count: &mut usize,
) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let rel = path
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/");
        if path.is_dir() {
            if rel == "__pycache__" || rel.ends_with("/__pycache__") {
                continue;
            }
            zip.add_directory(format!("{rel}/"), opts)?;
            zip_walk(zip, root, &path, opts, count)?;
        } else {
            zip.start_file(rel, opts)?;
            zip.write_all(&fs::read(&path)?)?;
            *count += 1;
        }
    }
    Ok(())
}
