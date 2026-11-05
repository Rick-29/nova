//! Running external commands and `{placeholder}` expansion.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

pub fn header(msg: &str) {
    println!("\n==> {msg}");
}

pub fn note(msg: &str) {
    println!("    {msg}");
}

pub fn warn(msg: &str) {
    eprintln!("    warning: {msg}");
}

/// Placeholders available to every command in `release.toml`.
#[derive(Clone, Debug, Default)]
pub struct Vars(pub BTreeMap<String, String>);

impl Vars {
    pub fn set(&mut self, key: &str, value: impl Into<String>) -> &mut Self {
        self.0.insert(key.to_string(), value.into());
        self
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    /// Replaces every `{name}`; unknown names are an error so typos surface early.
    pub fn expand(&self, text: &str) -> Result<String> {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = rest.find('{') {
            out.push_str(&rest[..start]);
            let after = &rest[start + 1..];
            let Some(end) = after.find('}') else {
                bail!("unbalanced `{{` in `{text}`");
            };
            let key = &after[..end];
            match self.0.get(key) {
                Some(v) => out.push_str(v),
                None => bail!("unknown placeholder `{{{key}}}` in `{text}`"),
            }
            rest = &after[end + 1..];
        }
        out.push_str(rest);
        Ok(out)
    }
}

fn build(cmdline: &str, env: &BTreeMap<String, String>, cwd: Option<&Path>) -> Result<Command> {
    let argv =
        shlex::split(cmdline).with_context(|| format!("cannot parse command `{cmdline}`"))?;
    anyhow::ensure!(!argv.is_empty(), "empty command");
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..]).envs(env);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    Ok(cmd)
}

/// Runs a command with inherited stdio; fails on non-zero exit.
pub fn run(cmdline: &str, env: &BTreeMap<String, String>, cwd: Option<&Path>) -> Result<()> {
    note(&format!("$ {cmdline}"));
    let mut cmd = build(cmdline, env, cwd)?;
    let status = match cmd.status() {
        Ok(s) => s,
        // Windows: `.cmd`/`.bat` shims (npm, gradlew.bat, ...) need a shell.
        Err(e) if cfg!(windows) && e.kind() == std::io::ErrorKind::NotFound => {
            let mut sh = Command::new("cmd");
            sh.arg("/C").arg(cmdline).envs(env);
            if let Some(dir) = cwd {
                sh.current_dir(dir);
            }
            sh.status()
                .with_context(|| format!("cannot start `{cmdline}`"))?
        }
        Err(e) => return Err(e).with_context(|| format!("cannot start `{cmdline}`")),
    };
    if !status.success() {
        bail!("`{cmdline}` failed with {status}");
    }
    Ok(())
}

/// Runs a command and returns trimmed stdout; fails on non-zero exit.
pub fn output(cmdline: &str) -> Result<String> {
    let out = build(cmdline, &BTreeMap::new(), None)?
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("cannot start `{cmdline}`"))?;
    if !out.status.success() {
        bail!("`{cmdline}` failed with {}", out.status);
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Like `output`, but a failing command yields `None` instead of an error.
pub fn try_output(cmdline: &str) -> Option<String> {
    let out = build(cmdline, &BTreeMap::new(), None)
        .ok()?
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn tool_on_path(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .map(|e| e.to_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };
    std::env::split_paths(&path).any(|dir| {
        exts.iter()
            .any(|ext| dir.join(format!("{name}{ext}")).is_file())
            || dir.join(name).is_file()
    })
}

pub fn host_os() -> &'static str {
    std::env::consts::OS
}

pub fn runs_here(only_on: &[String]) -> bool {
    only_on.is_empty() || only_on.iter().any(|o| o == host_os())
}
