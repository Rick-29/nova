//! Release automation for nova-ta. Run as `cargo xtask <command>` from the repository root.
//!
//! * `check`   - git preflight, version consistency, fmt/clippy/test/doc gate, publish dry-run
//! * `build`   - build every target in `release.toml` and pack `dist/`
//! * `publish` - check + build, then tag, push, publish to registries and create releases
//! * `bump`    - set a new version in `Cargo.toml` and every binding manifest

mod config;
mod git;
mod shell;
mod steps;
mod version;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};

use config::{Archive, Config, Publisher, Target};
use shell::{Vars, header, note, run, runs_here, tool_on_path, warn};

#[derive(Parser)]
#[command(name = "cargo xtask", about = "nova-ta release pipeline", version)]
struct Cli {
    /// Path to the release manifest.
    #[arg(long, default_value = "release.toml", global = true)]
    config: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Verify the tree is releasable: git state, versions, tests, warnings, publish dry-run.
    Check(CheckArgs),
    /// Build the language bindings and pack them into `dist/`.
    Build(BuildArgs),
    /// Full release: check, build, tag, push, upload to registries, create GitHub/GitLab releases.
    Publish(PublishArgs),
    /// Set a new version everywhere (Cargo.toml, lock file, binding manifests).
    Bump { version: String },
    /// List targets and publishers with their status on this machine.
    List,
}

#[derive(Args, Clone, Default)]
struct CheckArgs {
    /// Rewrite binding manifests to the Cargo.toml version instead of failing.
    #[arg(long)]
    sync_versions: bool,
    /// Skip the git preflight (useful while iterating locally).
    #[arg(long)]
    skip_git: bool,
    /// Skip fmt/clippy/test/doc (only git + versions).
    #[arg(long)]
    skip_quality: bool,
}

#[derive(Args, Clone, Default)]
struct BuildArgs {
    /// Build only these targets (repeatable). Also builds targets that are `enabled = false`.
    #[arg(long = "target", short = 't')]
    targets: Vec<String>,
    /// Do not clear `dist/` before building.
    #[arg(long)]
    keep_dist: bool,
}

#[derive(Args)]
struct PublishArgs {
    /// Print tag/push/upload commands instead of running them.
    #[arg(long)]
    dry_run: bool,
    /// Assume `check` already passed.
    #[arg(long)]
    skip_check: bool,
    /// Assume `build` already ran and `dist/` is complete.
    #[arg(long)]
    skip_build: bool,
    /// Skip these publishers by name (repeatable), e.g. `--skip gitlab`.
    #[arg(long = "skip")]
    skip: Vec<String>,
}

fn main() {
    if let Err(e) = real_main() {
        eprintln!("\nerror: {e:#}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<()> {
    let cli = Cli::parse();
    let root = workspace_root()?;
    std::env::set_current_dir(&root)?;
    load_dotenv(&root.join(".env"))?;
    let cfg = Config::load(&cli.config)?;
    let version = version::cargo_version("Cargo.toml")?;
    let vars = base_vars(&cfg, &version)?;

    match cli.cmd {
        Cmd::Check(args) => check(&cfg, &vars, &args),
        Cmd::Build(args) => build(&cfg, &vars, &args),
        Cmd::Publish(args) => publish(&cfg, &vars, &args),
        Cmd::Bump { version: new } => bump(&cfg, &new),
        Cmd::List => list(&cfg, &vars),
    }
}

/// Loads `KEY=VALUE` lines from `.env` (gitignored) into the environment without
/// overriding variables that are already set. Quotes around the value are stripped.
fn load_dotenv(path: &Path) -> Result<()> {
    let Ok(text) = fs::read_to_string(path) else {
        return Ok(());
    };
    let mut loaded = 0;
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            bail!("{}:{}: expected KEY=VALUE", path.display(), n + 1);
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if std::env::var_os(key).is_none() {
            // SAFETY: called once at startup before any other thread exists.
            unsafe { std::env::set_var(key, value) };
            loaded += 1;
        }
    }
    if loaded > 0 {
        note(&format!("loaded {loaded} variable(s) from .env"));
    }
    Ok(())
}

fn workspace_root() -> Result<PathBuf> {
    // `cargo xtask` runs from the workspace root; running the binary by hand may not.
    let mut dir = std::env::current_dir()?;
    loop {
        if dir.join("release.toml").is_file() {
            return Ok(dir);
        }
        if !dir.pop() {
            bail!("release.toml not found in the current directory or any parent");
        }
    }
}

/// Placeholders shared by every target: `{version}`, `{tag}`, `{lib}`, `{platform}`, ...
fn base_vars(cfg: &Config, version: &str) -> Result<Vars> {
    let (prefix, ext, static_ext, import) = match shell::host_os() {
        "windows" => ("", "dll", "lib", true),
        "macos" => ("lib", "dylib", "a", false),
        _ => ("lib", "so", "a", false),
    };
    let arch = std::env::consts::ARCH;
    let jna_os = match shell::host_os() {
        "windows" => "win32",
        "macos" => "darwin",
        other => other,
    };
    let jna_arch = match arch {
        "x86_64" => "x86-64",
        "x86" => "x86",
        other => other,
    };
    let lib_file = format!("{prefix}{}.{ext}", cfg.project.lib_name);
    let mut v = Vars::default();
    v.set("version", version)
        .set("crate", cfg.project.crate_name.as_str())
        .set("lib_name", cfg.project.lib_name.as_str())
        .set("lib_file", lib_file.as_str())
        .set("lib", format!("target/release/{lib_file}"))
        .set(
            "lib_import",
            if import {
                format!("target/release/{lib_file}.lib")
            } else {
                format!("target/release/{lib_file}")
            },
        )
        .set(
            "static_lib",
            format!(
                "target/release/{prefix}{}.{static_ext}",
                cfg.project.lib_name
            ),
        )
        .set("os", shell::host_os())
        .set("arch", arch)
        .set("platform", format!("{}-{arch}", shell::host_os()))
        .set("jna_platform", format!("{jna_os}-{jna_arch}"))
        .set("dist", cfg.project.dist.as_str());
    let tag = v.expand(&cfg.project.tag)?;
    v.set("tag", tag);
    Ok(v)
}

fn target_vars(base: &Vars, target: &Target) -> Vars {
    let mut v = base.clone();
    v.set("target", target.name.as_str())
        .set("out", format!("bindings/{}", target.name));
    v
}

// ---------------------------------------------------------------- check

fn check(cfg: &Config, vars: &Vars, args: &CheckArgs) -> Result<()> {
    let version = vars.get("version").unwrap();
    let tag = vars.get("tag").unwrap();
    println!(
        "nova-ta release check: version {version}, tag {tag}, host {}",
        vars.get("platform").unwrap()
    );

    if args.skip_git {
        warn("git preflight skipped");
    } else {
        git::preflight(&cfg.project, tag)?;
    }

    header("version consistency");
    note(&format!("Cargo.toml: {version} (source of truth)"));
    let mut files: Vec<&config::VersionFile> = cfg.version_files.iter().collect();
    for t in &cfg.targets {
        files.extend(t.version_files.iter());
    }
    let owned: Vec<config::VersionFile> = files
        .iter()
        .map(|f| config::VersionFile {
            path: f.path.clone(),
            pattern: f.pattern.clone(),
        })
        .collect();
    let fixed = version::check_files(&owned, version, args.sync_versions)?;
    if fixed > 0 && !args.skip_git {
        bail!("{fixed} manifest(s) were updated; commit them and run check again");
    }
    if let Some(published) = version::crates_io_version(&cfg.project.crate_name) {
        if published == version {
            let msg = format!(
                "{} {version} is already on crates.io; bump the version first (`cargo xtask bump <version>`)",
                cfg.project.crate_name
            );
            if args.skip_git {
                warn(&msg);
            } else {
                bail!(msg);
            }
        }
        note(&format!(
            "crates.io has {} {published}",
            cfg.project.crate_name
        ));
    }

    if args.skip_quality {
        warn("quality gate skipped");
        return Ok(());
    }
    let no_env = BTreeMap::new();
    if cfg.check.fmt {
        header("rustfmt");
        run("cargo fmt --all --check", &no_env, None)
            .context("formatting differs; run `cargo fmt --all`")?;
    }
    header("clippy (warnings are errors)");
    run(&cfg.check.clippy, &no_env, None)?;
    header("tests");
    run(&cfg.check.test, &no_env, None)?;
    header("rustdoc (warnings are errors)");
    let mut doc_env = BTreeMap::new();
    doc_env.insert("RUSTDOCFLAGS".to_string(), "-D warnings".to_string());
    run(&cfg.check.doc, &doc_env, None)?;
    if cfg.check.publish_dry_run {
        header("cargo publish --dry-run");
        let allow_dirty = if args.skip_git { " --allow-dirty" } else { "" };
        run(
            &format!("cargo publish --workspace --dry-run{allow_dirty}"),
            &no_env,
            None,
        )?;
    }
    if !cfg.check.extra.is_empty() {
        header("extra checks");
        steps::run_steps(&cfg.check.extra, vars)?;
    }
    header("check passed");
    Ok(())
}

// ---------------------------------------------------------------- build

fn selected_targets<'a>(cfg: &'a Config, names: &[String]) -> Result<Vec<&'a Target>> {
    if names.is_empty() {
        return Ok(cfg.targets.iter().filter(|t| t.enabled).collect());
    }
    names
        .iter()
        .map(|n| {
            cfg.targets
                .iter()
                .find(|t| &t.name == n)
                .with_context(|| format!("no target named `{n}` in release.toml"))
        })
        .collect()
}

fn build(cfg: &Config, vars: &Vars, args: &BuildArgs) -> Result<()> {
    let dist = Path::new(&cfg.project.dist);
    if !args.keep_dist && args.targets.is_empty() && dist.exists() {
        fs::remove_dir_all(dist).context("clearing dist/")?;
    }
    fs::create_dir_all(dist)?;

    let mut built = Vec::new();
    let mut skipped = Vec::new();
    for target in selected_targets(cfg, &args.targets)? {
        header(&format!("target {}", target.name));
        if !runs_here(&target.only_on) {
            note(&format!(
                "skipped: only builds on {}",
                target.only_on.join(", ")
            ));
            skipped.push(target.name.clone());
            continue;
        }
        let missing: Vec<&str> = target
            .requires
            .iter()
            .map(String::as_str)
            .filter(|t| !tool_on_path(t))
            .collect();
        if !missing.is_empty() {
            bail!(
                "target `{}` needs {} on PATH",
                target.name,
                missing.join(", ")
            );
        }
        build_target(target, &target_vars(vars, target))?;
        built.push(target.name.clone());
    }

    header("dist");
    for entry in fs::read_dir(dist)? {
        let entry = entry?;
        note(&format!(
            "{} ({} KiB)",
            entry.file_name().to_string_lossy(),
            entry.metadata()?.len() / 1024
        ));
    }
    println!("\nbuilt: {}", built.join(", "));
    if !skipped.is_empty() {
        println!("skipped on this host: {}", skipped.join(", "));
    }
    Ok(())
}

fn build_target(target: &Target, vars: &Vars) -> Result<()> {
    let out = PathBuf::from(vars.get("out").unwrap());
    if out.exists() {
        fs::remove_dir_all(&out).with_context(|| format!("clearing {}", out.display()))?;
    }
    fs::create_dir_all(&out)?;

    steps::run_steps(&target.steps, vars)
        .with_context(|| format!("building target `{}`", target.name))?;

    for pattern in &target.artifacts {
        steps::glob_required(&vars.expand(pattern)?)
            .with_context(|| format!("target `{}`", target.name))?;
    }
    if !target.artifacts.is_empty() {
        note(&format!(
            "{} artifact pattern(s) present",
            target.artifacts.len()
        ));
    }

    let dist = PathBuf::from(vars.get("dist").unwrap());
    let archive_dir = match &target.archive {
        Archive::Toggle(false) => None,
        Archive::Toggle(true) => Some(out.clone()),
        Archive::Dir(d) => Some(PathBuf::from(vars.expand(d)?)),
    };
    if let Some(dir) = archive_dir {
        let name = vars.expand("{crate}-{version}-{target}-{platform}.zip")?;
        steps::zip_dir(&dir, &dist.join(name))?;
    }
    for pattern in &target.dist {
        for file in steps::glob_required(&vars.expand(pattern)?)? {
            let dest = dist.join(file.file_name().context("dist glob matched a directory")?);
            note(&format!("dist <- {}", file.display()));
            fs::copy(&file, &dest)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- publish

fn publish(cfg: &Config, vars: &Vars, args: &PublishArgs) -> Result<()> {
    let version = vars.get("version").unwrap().to_string();
    let tag = vars.get("tag").unwrap().to_string();
    let dry = args.dry_run;

    if args.skip_check {
        warn("check skipped on request");
    } else {
        check(cfg, vars, &CheckArgs::default())?;
    }
    if args.skip_build {
        warn("build skipped on request; using the existing dist/");
    } else {
        build(cfg, vars, &BuildArgs::default())?;
    }

    header("post-build sanity");
    let stale: Vec<String> = fs::read_dir(&cfg.project.dist)?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != "RELEASE_NOTES.md" && !n.contains(&version))
        .collect();
    if !stale.is_empty() {
        bail!(
            "dist/ holds files from another version: {} (run `cargo xtask build` to rebuild it)",
            stale.join(", ")
        );
    }
    let changed = shell::output("git status --porcelain")?;
    if !changed.is_empty() {
        bail!(
            "the build modified tracked files; commit them, push, and publish again:
{changed}"
        );
    }
    note("dist/ matches the version and the tree is still clean");

    header("release notes");
    let notes_path = write_release_notes(cfg, &version)?;
    note(&format!("{}", notes_path.display()));

    let dist_files: Vec<PathBuf> = fs::read_dir(&cfg.project.dist)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p != &notes_path)
        .collect();
    if dist_files.is_empty() {
        bail!("dist/ is empty; nothing to publish");
    }
    let quoted: Vec<String> = dist_files
        .iter()
        .map(|p| shlex::try_quote(&p.to_string_lossy().replace('\\', "/")).map(|s| s.into_owned()))
        .collect::<Result<_, _>>()
        .context("quoting dist paths")?;
    let mut pvars = vars.clone();
    pvars
        .set("dist_files", quoted.join(" "))
        .set("notes", notes_path.to_string_lossy().replace('\\', "/"));

    header(&format!("tag {tag}"));
    git::create_tag(&tag, &format!("{} {version}", cfg.project.crate_name), dry)?;
    git::push(&cfg.project, &tag, dry)?;

    header("crates.io");
    let cmd = "cargo publish --workspace";
    if dry {
        note(&format!("[dry-run] {cmd}"));
    } else {
        run(cmd, &BTreeMap::new(), None)?;
    }

    for target in cfg
        .targets
        .iter()
        .filter(|t| t.enabled && !t.publish.is_empty())
    {
        if !runs_here(&target.only_on) {
            continue;
        }
        header(&format!("publish target {}", target.name));
        if dry {
            let tv = target_vars(&pvars, target);
            for s in &target.publish {
                note(&format!("[dry-run] {}", steps::describe(s, &tv)));
            }
        } else {
            steps::run_steps(&target.publish, &target_vars(&pvars, target))?;
        }
    }

    for publisher in cfg.publishers.iter().filter(|p| p.enabled) {
        header(&format!("publisher {}", publisher.name));
        if args.skip.contains(&publisher.name) {
            note("skipped on request");
            continue;
        }
        if !runs_here(&publisher.only_on) {
            note(&format!(
                "skipped: only on {}",
                publisher.only_on.join(", ")
            ));
            continue;
        }
        check_publisher_prereqs(publisher)?;
        let mut pvars = pvars.clone();
        if let Some(pattern) = &publisher.files {
            let files = steps::glob_required(&pvars.expand(pattern)?)?;
            let quoted: Vec<String> = files
                .iter()
                .map(|p| {
                    shlex::try_quote(&p.to_string_lossy().replace('\\', "/"))
                        .map(|s| s.into_owned())
                })
                .collect::<Result<_, _>>()
                .context("quoting publisher files")?;
            pvars.set("files", quoted.join(" "));
        }
        if dry {
            for s in &publisher.steps {
                note(&format!("[dry-run] {}", steps::describe(s, &pvars)));
            }
        } else {
            steps::run_steps(&publisher.steps, &pvars)
                .with_context(|| format!("publisher `{}`", publisher.name))?;
        }
    }

    let done = if dry {
        "dry-run complete".to_string()
    } else {
        format!("released {tag}")
    };
    header(&done);
    Ok(())
}

fn check_publisher_prereqs(p: &Publisher) -> Result<()> {
    let missing_tools: Vec<&str> = p
        .requires
        .iter()
        .map(String::as_str)
        .filter(|t| !tool_on_path(t))
        .collect();
    if !missing_tools.is_empty() {
        bail!(
            "publisher `{}` needs {} on PATH (or pass `--skip {}`)",
            p.name,
            missing_tools.join(", "),
            p.name
        );
    }
    let missing_env: Vec<&str> = p
        .env
        .iter()
        .map(String::as_str)
        .filter(|e| std::env::var_os(e).is_none())
        .collect();
    if !missing_env.is_empty() {
        bail!(
            "publisher `{}` needs environment variable(s) {} (or pass `--skip {}`)",
            p.name,
            missing_env.join(", "),
            p.name
        );
    }
    Ok(())
}

/// Release notes: the CHANGELOG section for this version if present, else the git log since the last tag.
fn write_release_notes(cfg: &Config, version: &str) -> Result<PathBuf> {
    let mut body = None;
    if let Some(path) = &cfg.project.changelog
        && let Ok(text) = fs::read_to_string(path)
    {
        body = changelog_section(&text, version);
    }
    let body = match body {
        Some(b) => b,
        None => {
            let prev = git::previous_tag();
            let log = git::log_since(prev.as_deref())?;
            match prev {
                Some(p) => format!("Changes since {p}:\n\n{log}\n"),
                None => format!("{log}\n"),
            }
        }
    };
    let path = Path::new(&cfg.project.dist).join("RELEASE_NOTES.md");
    fs::write(
        &path,
        format!("# {} {version}\n\n{body}", cfg.project.crate_name),
    )?;
    Ok(path)
}

fn changelog_section(text: &str, version: &str) -> Option<String> {
    let mut lines = text.lines();
    let heading = lines.find(|l| {
        let l = l.trim_start_matches('#').trim();
        l.starts_with(version) || l.starts_with(&format!("[{version}]"))
    })?;
    let _ = heading;
    let section: Vec<&str> = lines.take_while(|l| !l.starts_with("## ")).collect();
    let body = section.join("\n").trim().to_string();
    (!body.is_empty()).then_some(body)
}

// ---------------------------------------------------------------- bump

fn bump(cfg: &Config, new: &str) -> Result<()> {
    anyhow::ensure!(
        new.split('.').count() >= 3 && new.chars().next().is_some_and(|c| c.is_ascii_digit()),
        "`{new}` does not look like a semver version"
    );
    header(&format!("bump to {new}"));
    version::set_cargo_version("Cargo.toml", new)?;
    let mut files: Vec<config::VersionFile> = Vec::new();
    for f in cfg
        .version_files
        .iter()
        .chain(cfg.targets.iter().flat_map(|t| t.version_files.iter()))
    {
        files.push(config::VersionFile {
            path: f.path.clone(),
            pattern: f.pattern.clone(),
        });
    }
    version::check_files(&files, new, true)?;
    run("cargo update --workspace --quiet", &BTreeMap::new(), None)
        .context("refreshing Cargo.lock")?;
    note("Cargo.lock refreshed");
    println!("\nnow commit, push to every remote, and run `cargo xtask publish`");
    Ok(())
}

// ---------------------------------------------------------------- list

fn list(cfg: &Config, vars: &Vars) -> Result<()> {
    println!(
        "version {}  tag {}  host {}\n",
        vars.get("version").unwrap(),
        vars.get("tag").unwrap(),
        vars.get("platform").unwrap()
    );
    println!("targets:");
    for t in &cfg.targets {
        let state = if !t.enabled {
            "disabled".to_string()
        } else if !runs_here(&t.only_on) {
            format!("only on {}", t.only_on.join(", "))
        } else if let Some(m) = t.requires.iter().find(|r| !tool_on_path(r)) {
            format!("missing tool {m}")
        } else {
            "ready".to_string()
        };
        println!("  {:<16} {}", t.name, state);
    }
    println!("publishers:");
    for p in &cfg.publishers {
        let state = if !p.enabled {
            "disabled".to_string()
        } else if let Some(m) = p.requires.iter().find(|r| !tool_on_path(r)) {
            format!("missing tool {m}")
        } else if let Some(e) = p.env.iter().find(|e| std::env::var_os(e).is_none()) {
            format!("missing env {e}")
        } else {
            "ready".to_string()
        };
        println!("  {:<16} {}", p.name, state);
    }
    Ok(())
}
