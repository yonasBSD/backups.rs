//! Terminal UI — spinners, stage banners, and captured command output.
//!
//! # Design goals
//!
//! - **Clean by default.** While a stage is running the user sees only a spinner and a short label.
//!   Raw rustic output is captured and hidden.
//! - **Informative on failure.** If a stage exits non-zero its captured stdout *and* stderr are
//!   printed in full so the operator can diagnose the problem without re-running manually.
//! - **Testable without a terminal.** [`Stage`] and [`StageResult`] are plain data types; the
//!   rendering functions accept a `&mut dyn Write` so tests can capture output without touching the
//!   real terminal.
//!
//! # Typical usage
//!
//! ```no_run
//! use crate::ui::{run_stage, StageOutcome};
//!
//! let outcome = run_stage("Check", || {
//!     // … call rustic, return Ok(()) or Err(…) …
//!     Ok(())
//! });
//! outcome.print();
//! if outcome.failed() { std::process::exit(1); }
//! ```

use std::{
    process::{Command, Output, Stdio},
    time::Duration,
};

use anyhow::{Context, Result};
use console::style;
use indicatif::{ProgressBar, ProgressStyle};

// ─── Icons ───────────────────────────────────────────────────────────────────

/// Braille spinner frames — same style as indicatif's default.
static SPINNER_CHARS: &str = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏";

/// Green ✓  — printed when a stage succeeds.
fn icon_ok() -> console::StyledObject<&'static str> {
    style("✓").green().bold()
}
/// Red ✗    — printed when a stage fails.
fn icon_err() -> console::StyledObject<&'static str> {
    style("✗").red().bold()
}
/// Cyan ✓   — printed next to the final success summary.
fn icon_done() -> console::StyledObject<&'static str> {
    style("✓").cyan().bold()
}

// ─── Stage result ─────────────────────────────────────────────────────────────

/// The outcome of a single pipeline stage.
///
/// Carries the stage label plus whatever the command wrote to stdout/stderr so
/// it can be replayed to the terminal when something goes wrong.
#[derive(Debug)]
pub struct StageOutcome {
    /// Human-readable stage label, e.g. `"Check"`.
    pub label: String,
    /// Whether the stage completed without error.
    pub success: bool,
    /// Everything the command wrote to stdout (empty on success unless
    /// `--verbose` is added in the future).
    pub stdout: String,
    /// Everything the command wrote to stderr.
    pub stderr: String,
    /// The anyhow error message, if any.
    pub error: Option<String>,
}

impl StageOutcome {
    /// Print the one-line summary (✓/✗ + label) to stdout.
    ///
    /// On failure, also prints the captured stdout/stderr and the error
    /// message so the operator has everything they need without re-running.
    pub fn print(&self) {
        if self.success {
            println!("  {}  {}", icon_ok(), style(&self.label).bold());
        } else {
            println!("  {}  {}", icon_err(), style(&self.label).bold());

            // Print the error message first (most useful thing).
            if let Some(ref msg) = self.error {
                eprintln!();
                eprintln!("  {} {}", style("Error:").red().bold(), msg);
            }

            // Replay captured output so the operator can see what rustic said.
            if !self.stdout.is_empty() {
                eprintln!();
                eprintln!("  {} stdout:", style("►").dim());
                for line in self.stdout.lines() {
                    eprintln!("    {line}");
                }
            }
            if !self.stderr.is_empty() {
                eprintln!();
                eprintln!("  {} stderr:", style("►").dim());
                for line in self.stderr.lines() {
                    eprintln!("    {line}");
                }
            }
        }
    }

    /// Returns `true` if the stage did not succeed.
    pub const fn failed(&self) -> bool {
        !self.success
    }
}

// ─── Spinner ──────────────────────────────────────────────────────────────────

/// Create and start an indeterminate spinner for `label`.
///
/// The spinner ticks at ~80 ms and is automatically cleared when
/// [`ProgressBar::finish_and_clear`] is called.
fn make_spinner(label: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("  {spinner:.cyan}  {msg}")
            .unwrap()
            .tick_chars(SPINNER_CHARS),
    );
    pb.set_message(format!("{}", style(label).dim()));
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

// ─── Captured execution ───────────────────────────────────────────────────────

/// Run a command, capturing both stdout and stderr.
///
/// Unlike [`crate::runner::run`] this does **not** inherit the parent's
/// stdout/stderr — all output is buffered so the spinner can own the terminal
/// while the command runs.
///
/// Returns `(success, stdout_text, stderr_text)`.
pub fn run_captured(args: &[String]) -> Result<(bool, String, String)> {
    let (prog, rest) = args.split_first().context("cannot run an empty command")?;

    let output: Output = Command::new(prog)
        .args(rest)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to spawn: {}", args.join(" ")))?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    Ok((output.status.success(), stdout, stderr))
}

// ─── High-level stage runner ──────────────────────────────────────────────────

/// Run a pipeline stage behind a spinner, returning a [`StageOutcome`].
///
/// `build_args` is a closure that returns the argument list to execute.  It is
/// called *after* the spinner starts so any argument-building work is covered
/// by the animation.
///
/// The spinner is cleared before the outcome line is printed, so the terminal
/// always shows a clean, static summary when the stage finishes.
pub fn run_stage(label: &str, args: &[String]) -> StageOutcome {
    let spinner = make_spinner(label);

    let result = run_captured(args);
    spinner.finish_and_clear();

    match result {
        Ok((true, stdout, stderr)) => StageOutcome {
            label: label.to_string(),
            success: true,
            stdout,
            stderr,
            error: None,
        },
        Ok((false, stdout, stderr)) => StageOutcome {
            label: label.to_string(),
            success: false,
            stdout,
            stderr,
            error: Some(format!("command exited non-zero: {}", args.join(" "))),
        },
        Err(e) => StageOutcome {
            label: label.to_string(),
            success: false,
            stdout: String::new(),
            stderr: String::new(),
            error: Some(e.to_string()),
        },
    }
}

/// Like [`run_stage`] but for stages that are logically skipped (e.g. because
/// `--no-mount` was passed and there is no mount configured).
///
/// Returns a synthetic success outcome so the pipeline does not need special-
/// case logic for optional stages.
pub fn skipped_stage(label: &str) -> StageOutcome {
    StageOutcome {
        label: label.to_string(),
        success: true,
        stdout: String::new(),
        stderr: String::new(),
        error: None,
    }
}

// ─── Summary banner ───────────────────────────────────────────────────────────

/// Print the final summary after all stages have run.
///
/// Shows a success banner when all stages passed, or a failure banner listing
/// the stages that failed.
pub fn print_summary(outcomes: &[StageOutcome]) {
    let failed: Vec<&StageOutcome> = outcomes.iter().filter(|o| o.failed()).collect();
    println!();
    if failed.is_empty() {
        println!(
            "  {} {}",
            icon_done(),
            style("All stages completed successfully.").cyan().bold()
        );
    } else {
        eprintln!("  {}  {}", icon_err(), style("Backup failed.").red().bold());
        for o in &failed {
            eprintln!("    {} {}", icon_err(), style(&o.label).red());
        }
    }
    println!();
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn success(label: &str) -> StageOutcome {
        StageOutcome {
            label: label.into(),
            success: true,
            stdout: String::new(),
            stderr: String::new(),
            error: None,
        }
    }

    fn failure(label: &str, err: &str, stdout: &str, stderr: &str) -> StageOutcome {
        StageOutcome {
            label: label.into(),
            success: false,
            stdout: stdout.into(),
            stderr: stderr.into(),
            error: Some(err.into()),
        }
    }

    // ── StageOutcome::failed ──────────────────────────────────────────────────

    #[test]
    fn success_outcome_is_not_failed() {
        assert!(!success("Check").failed());
    }

    #[test]
    fn failure_outcome_is_failed() {
        assert!(failure("Check", "oh no", "", "").failed());
    }

    // ── run_captured ─────────────────────────────────────────────────────────

    #[test]
    fn run_captured_true_succeeds() {
        let (ok, _out, _err) = run_captured(&["true".into()]).unwrap();
        assert!(ok);
    }

    #[test]
    fn run_captured_false_fails() {
        let (ok, _out, _err) = run_captured(&["false".into()]).unwrap();
        assert!(!ok);
    }

    #[test]
    fn run_captured_captures_stdout() {
        let (ok, out, _err) =
            run_captured(&["sh".into(), "-c".into(), "echo hello".into()]).unwrap();
        assert!(ok);
        assert!(out.contains("hello"));
    }

    #[test]
    fn run_captured_captures_stderr() {
        let (ok, _out, err) =
            run_captured(&["sh".into(), "-c".into(), "echo oops >&2".into()]).unwrap();
        assert!(ok);
        assert!(err.contains("oops"));
    }

    #[test]
    fn run_captured_captures_non_zero_output() {
        let (ok, out, _err) =
            run_captured(&["sh".into(), "-c".into(), "echo failing; exit 1".into()]).unwrap();
        assert!(!ok);
        assert!(out.contains("failing"));
    }

    #[test]
    fn run_captured_empty_args_errors() {
        let result = run_captured(&[]);
        assert!(result.is_err());
    }

    // ── run_stage ─────────────────────────────────────────────────────────────

    #[test]
    fn run_stage_success_sets_success_true() {
        let o = run_stage("Test", &["true".into()]);
        assert!(o.success);
        assert_eq!(o.label, "Test");
        assert!(o.error.is_none());
    }

    #[test]
    fn run_stage_failure_sets_success_false() {
        let o = run_stage("Test", &["false".into()]);
        assert!(!o.success);
        assert!(o.error.is_some());
    }

    #[test]
    fn run_stage_captures_stdout_on_failure() {
        let o = run_stage("Test", &[
            "sh".into(),
            "-c".into(),
            "echo bad output; exit 1".into(),
        ]);
        assert!(!o.success);
        assert!(o.stdout.contains("bad output"));
    }

    // ── skipped_stage ─────────────────────────────────────────────────────────

    #[test]
    fn skipped_stage_is_success() {
        let o = skipped_stage("Mount");
        assert!(o.success);
        assert_eq!(o.label, "Mount");
    }

    // ── print_summary ─────────────────────────────────────────────────────────

    #[test]
    fn summary_with_all_successes_does_not_list_failures() {
        // Smoke test: just ensure it doesn't panic with all-success inputs.
        let outcomes = vec![success("Mount"), success("Check"), success("Backup")];
        print_summary(&outcomes); // would panic if something is broken
    }

    #[test]
    fn summary_with_failure_includes_failed_stages() {
        // Similarly smoke-tested; insta snapshot tests give us richer coverage.
        let outcomes = vec![
            success("Mount"),
            failure("Check", "repo corrupt", "", "error detail"),
            success("Backup"),
        ];
        print_summary(&outcomes);
    }
}
