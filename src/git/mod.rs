use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::process::Command;

const EXCLUDED_DIFF_PATHS: &[&str] = &[
    ":(exclude)Cargo.lock",
    ":(exclude)package-lock.json",
    ":(exclude)pnpm-lock.yaml",
    ":(exclude)yarn.lock",
    ":(exclude)go.sum",
    ":(exclude)dist/**",
    ":(exclude)build/**",
    ":(exclude)target/**",
    ":(exclude)node_modules/**",
    ":(exclude)coverage/**",
    ":(exclude).next/**",
    ":(exclude)out/**",
    ":(exclude)vendor/**",
    ":(exclude)tmp/**",
    ":(exclude)temp/**",
    ":(exclude)storybook-static/**",
    ":(exclude).turbo/**",
    ":(exclude).parcel-cache/**",
    ":(exclude).svelte-kit/**",
    ":(exclude).nuxt/**",
    ":(exclude)public/build/**",
    ":(exclude)bin/**",
];

#[derive(Debug, Clone, Serialize)]
pub struct CommandResult {
    pub command: String,
    pub output: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitChangeSet {
    pub branch: CommandResult,
    pub status_short: CommandResult,
    pub diff_stat: CommandResult,
    pub diff_numstat: CommandResult,
    pub diff_name_only: CommandResult,
    pub diff: CommandResult,
    pub file_diffs: Vec<FileDiff>,
    pub skipped_diff_paths: Vec<String>,

    /// True when the diff came from staged changes:
    /// git diff --staged
    pub staged: bool,

    /// True when the diff was too large and Paladin truncated it
    /// before sending it to the local model.
    pub diff_truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileDiff {
    pub path: String,
    pub diff: String,
}

/// Collects the current Git changes.
///
/// Behavior:
/// - If `staged` is true:
///   - only read staged changes using `git diff --staged`.
///
/// - If `staged` is false:
///   - first check staged changes.
///   - if staged changes exist, use them.
///   - otherwise, fallback to unstaged changes using `git diff`.
///
/// This makes `paladin commit` work after:
///
/// ```powershell
/// git add .
/// paladin commit
/// ```
///
/// And it also works when files are modified but not staged yet.
pub fn collect_changes(staged: bool, max_diff_chars: usize) -> Result<GitChangeSet> {
    ensure_git_repo()?;

    let use_staged = if staged {
        true
    } else {
        let staged_diff = run_git(&["diff", "--staged"])?;
        !staged_diff.output.trim().is_empty()
    };

    let branch = run_git(&["branch", "--show-current"])?;
    let status_short = run_git(&["status", "--short"])?;

    let diff_stat = if use_staged {
        run_git(&["diff", "--staged", "--stat"])?
    } else {
        run_git(&["diff", "--stat"])?
    };

    let diff_numstat = if use_staged {
        run_git(&["diff", "--staged", "--numstat"])?
    } else {
        run_git(&["diff", "--numstat"])?
    };

    let diff_name_only = if use_staged {
        run_git(&["diff", "--staged", "--name-only"])?
    } else {
        run_git(&["diff", "--name-only"])?
    };

    let (mut diff, skipped_diff_paths) = if use_staged {
        run_git_filtered_diff(true)?
    } else {
        run_git_filtered_diff(false)?
    };

    let diff_truncated = diff.output.chars().count() > max_diff_chars;

    if diff_truncated {
        diff.output = truncate_chars(&diff.output, max_diff_chars);
        diff.output.push_str("\n\n[diff truncated by paladin]");
    }

    Ok(GitChangeSet {
        branch,
        status_short,
        diff_stat,
        diff_numstat,
        diff_name_only,
        diff,
        file_diffs: collect_file_diffs(use_staged)?,
        skipped_diff_paths,
        staged: use_staged,
        diff_truncated,
    })
}

/// Returns true when Paladin has an actual diff to send to the model.
pub fn has_changes(changes: &GitChangeSet) -> bool {
    !changes.diff_name_only.output.trim().is_empty()
}

pub fn commit_paths(message: &str, body: &[String], paths: &[String]) -> Result<()> {
    if paths.is_empty() {
        return Err(anyhow!("cannot create a path-based commit without files"));
    }

    let mut args = vec!["commit", "-m", message];
    let joined_body;

    if !body.is_empty() {
        joined_body = body
            .iter()
            .map(|line| format!("- {}", line.trim().trim_start_matches("- ")))
            .collect::<Vec<_>>()
            .join("\n");

        args.push("-m");
        args.push(joined_body.as_str());
    }

    args.push("--");

    for path in paths {
        args.push(path.as_str());
    }

    run_git_checked(&args)
}

fn collect_file_diffs(staged: bool) -> Result<Vec<FileDiff>> {
    let diff_name_only = if staged {
        run_git(&["diff", "--staged", "--name-only"])?
    } else {
        run_git(&["diff", "--name-only"])?
    };

    let mut files = Vec::new();

    for path in diff_name_only
        .output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if should_skip_diff_path(path) {
            continue;
        }

        let diff = if staged {
            run_git(&["diff", "--staged", "--", path])?
        } else {
            run_git(&["diff", "--", path])?
        };

        files.push(FileDiff {
            path: path.to_string(),
            diff: diff.output,
        });
    }

    Ok(files)
}

/// Ensures the current directory is inside a Git repository.
fn ensure_git_repo() -> Result<()> {
    let output = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .context("failed to run git rev-parse")?;

    if !output.status.success() {
        return Err(anyhow!("current directory is not inside a Git repository"));
    }

    Ok(())
}

/// Runs a Git command and returns stdout/stderr without failing on Git exit status.
///
/// This is useful for read-only Git commands where we want to inspect the output.
fn run_git(args: &[&str]) -> Result<CommandResult> {
    let command = format!("git {}", args.join(" "));

    let output = Command::new("git")
        .args(args)
        .output()
        .with_context(|| format!("failed to run `{}`", command))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    Ok(CommandResult {
        command,
        output: stdout,
        error: if stderr.trim().is_empty() {
            None
        } else {
            Some(stderr)
        },
    })
}

fn run_git_filtered_diff(staged: bool) -> Result<(CommandResult, Vec<String>)> {
    let excluded_paths = collect_excluded_paths(staged)?;
    let mut args = vec!["diff"];

    if staged {
        args.push("--staged");
    }

    args.push("--");
    args.push(".");
    args.extend(EXCLUDED_DIFF_PATHS.iter().copied());

    let diff = run_git(&args)?;

    Ok((diff, excluded_paths))
}

fn collect_excluded_paths(staged: bool) -> Result<Vec<String>> {
    let diff_name_only = if staged {
        run_git(&["diff", "--staged", "--name-only"])?
    } else {
        run_git(&["diff", "--name-only"])?
    };

    let mut excluded_paths = Vec::new();

    for path in diff_name_only
        .output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if should_skip_diff_path(path) {
            excluded_paths.push(path.to_string());
        }
    }

    Ok(excluded_paths)
}

fn should_skip_diff_path(path: &str) -> bool {
    path == "Cargo.lock"
        || path == "package-lock.json"
        || path == "pnpm-lock.yaml"
        || path == "yarn.lock"
        || path == "go.sum"
        || is_path_in_dir(path, "dist")
        || is_path_in_dir(path, "build")
        || is_path_in_dir(path, "target")
        || is_path_in_dir(path, "node_modules")
        || is_path_in_dir(path, "coverage")
        || is_path_in_dir(path, ".next")
        || is_path_in_dir(path, "out")
        || is_path_in_dir(path, "vendor")
        || is_path_in_dir(path, "tmp")
        || is_path_in_dir(path, "temp")
        || is_path_in_dir(path, "storybook-static")
        || is_path_in_dir(path, ".turbo")
        || is_path_in_dir(path, ".parcel-cache")
        || is_path_in_dir(path, ".svelte-kit")
        || is_path_in_dir(path, ".nuxt")
        || is_path_in_dir(path, "public/build")
        || is_path_in_dir(path, "bin")
}

fn is_path_in_dir(path: &str, dir: &str) -> bool {
    path == dir
        || path
            .strip_prefix(dir)
            .is_some_and(|rest| rest.starts_with('/'))
        || path
            .strip_prefix(dir)
            .is_some_and(|rest| rest.starts_with('\\'))
}

/// Runs a Git command and fails if Git returns a non-zero status.
///
/// This is useful for write commands like:
/// - git add -A
/// - git commit
fn run_git_checked(args: &[&str]) -> Result<()> {
    let command = format!("git {}", args.join(" "));

    let output = Command::new("git")
        .args(args)
        .output()
        .with_context(|| format!("failed to run `{}`", command))?;

    if !output.status.success() {
        return Err(anyhow!(
            "`{}` failed:\n{}",
            command,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

/// Truncates a string safely by character count.
///
/// This avoids breaking UTF-8 characters in the middle.
fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}
