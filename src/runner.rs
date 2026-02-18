//! Command argument construction helpers.
//!
//! This module is responsible for *building* the argument lists that will be
//! passed to rustic.  It deliberately does **not** execute anything — process
//! execution lives in [`crate::ui`] so that the spinner can own the terminal
//! while commands run.
//!
//! Keeping arg-building separate from execution means every function here is
//! pure and trivially unit-testable without spawning any child processes.
//!
//! # Privilege escalation
//!
//! [`prefix`] returns a zero- or one-element `Vec` that is prepended to every
//! command.  When `--sudo` is set it contains `["doas"]`; otherwise it is
//! empty.  We use `doas` rather than `sudo` because it has a simpler
//! configuration model and matches what the original shell script used.

use crate::{cli::Cli, config::Config};

// ─── Privilege prefix ─────────────────────────────────────────────────────────

/// Returns `["doas"]` when `--sudo` is set, otherwise an empty `Vec`.
///
/// Prepend this to any command that needs elevated privileges.
pub fn prefix(cli: &Cli) -> Vec<String> {
    if cli.sudo {
        vec!["doas".into()]
    } else {
        vec![]
    }
}

// ─── rustic base command ──────────────────────────────────────────────────────

/// Builds the argument list shared by every `rustic` invocation:
///
/// ```text
/// [doas]  rustic  -r <repo.path>  --password <repo.password>
/// ```
///
/// Callers append the subcommand and extra flags to the returned `Vec` before
/// passing it to [`crate::ui::run_stage`].
pub fn rustic_base(cli: &Cli, cfg: &Config) -> Vec<String> {
    let mut cmd: Vec<String> = prefix(cli);
    cmd.push("rustic".into());
    cmd.extend([
        "-r".into(),
        cfg.repo.path.clone(),
        "--password".into(),
        cfg.repo.password.clone(),
    ]);
    cmd
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;
    use crate::config::{BackupConfig, MountConfig, RepoConfig, RetentionConfig};

    fn make_cfg(repo_path: &str, password: &str) -> Config {
        Config {
            repo: RepoConfig {
                path: repo_path.into(),
                password: password.into(),
            },
            backup: BackupConfig::default(),
            retention: RetentionConfig::default(),
            mount: MountConfig::default(),
        }
    }

    fn make_cli(extra: &[&str]) -> Cli {
        Cli::parse_from(std::iter::once("backup").chain(extra.iter().copied()))
    }

    // ── prefix ────────────────────────────────────────────────────────────────

    #[test]
    fn prefix_empty_without_sudo() {
        assert!(prefix(&make_cli(&[])).is_empty());
    }

    #[test]
    fn prefix_doas_with_sudo() {
        assert_eq!(prefix(&make_cli(&["--sudo"])), vec!["doas"]);
    }

    // ── rustic_base ───────────────────────────────────────────────────────────

    #[test]
    fn rustic_base_without_sudo() {
        let cmd = rustic_base(&make_cli(&[]), &make_cfg("/tmp/repo", ""));
        assert_eq!(cmd, vec!["rustic", "-r", "/tmp/repo", "--password", ""]);
    }

    #[test]
    fn rustic_base_with_sudo_prepends_doas() {
        let cmd = rustic_base(&make_cli(&["--sudo"]), &make_cfg("/tmp/repo", "s3cr3t"));
        assert_eq!(cmd, vec![
            "doas",
            "rustic",
            "-r",
            "/tmp/repo",
            "--password",
            "s3cr3t"
        ]);
    }

    #[test]
    fn rustic_base_preserves_paths_with_spaces() {
        let cmd = rustic_base(&make_cli(&[]), &make_cfg("/mnt/my nas/repo", "p@ss"));
        assert_eq!(cmd[2], "/mnt/my nas/repo");
        assert_eq!(cmd[4], "p@ss");
    }

    // ── insta snapshots ───────────────────────────────────────────────────────

    #[test]
    fn snapshot_rustic_base_no_sudo() {
        let cmd = rustic_base(&make_cli(&[]), &make_cfg("/tmp/repo", "hunter2"));
        insta::assert_debug_snapshot!(cmd);
    }

    #[test]
    fn snapshot_rustic_base_with_sudo() {
        let cmd = rustic_base(&make_cli(&["--sudo"]), &make_cfg("/tmp/repo", "hunter2"));
        insta::assert_debug_snapshot!(cmd);
    }

    #[test]
    fn snapshot_prefix_no_sudo() {
        insta::assert_debug_snapshot!(prefix(&make_cli(&[])));
    }

    #[test]
    fn snapshot_prefix_with_sudo() {
        insta::assert_debug_snapshot!(prefix(&make_cli(&["--sudo"])));
    }
}
