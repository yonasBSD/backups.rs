//! Default backup pipeline — runs when no subcommand is given.
//!
//! # Pipeline stages (in order)
//!
//! | # | Stage    | Flag to skip   | Description                              |
//! |---|----------|----------------|------------------------------------------|
//! | 1 | Mount    | `--no-mount`   | Mount the NAS share                      |
//! | 2 | Init     | —              | Create repo on first run                 |
//! | 3 | Check    | `--no-check`   | Verify repository integrity              |
//! | 4 | Backup   | —              | Snapshot sources → repo                  |
//! | 5 | Forget   | `--no-prune`   | Apply retention policy, prune dead packs |
//! | 6 | Compact  | `--no-prune`   | Final `rustic prune` for disk reclaim    |
//!
//! Each stage runs behind a spinner.  Raw rustic output is captured and hidden
//! unless the stage fails, in which case stdout + stderr are replayed so the
//! operator can diagnose the issue.
//!
//! ## Sources default
//!
//! If `[backup].sources` is empty the current directory (`"."`) is used.

use std::path::Path;

use anyhow::Result;

use crate::{
    cli::Cli,
    config::Config,
    mount,
    runner::{prefix, rustic_base},
    ui::{StageOutcome, print_summary, run_stage, skipped_stage},
};

// ─── Entry point ──────────────────────────────────────────────────────────────

/// Execute the full backup pipeline.
///
/// Stages are run sequentially.  Every stage always runs (so the summary shows
/// every result), but if any stage fails the function returns an error after
/// printing the summary.
pub fn run(cli: &Cli, cfg: &Config) -> Result<()> {
    println!();

    let mut outcomes: Vec<StageOutcome> = Vec::new();

    // 1. Mount
    let mount = if !cli.no_mount && cfg.mount.share.is_some() {
        mount::mount_share(&cfg.mount)
    } else {
        skipped_stage("Mount")
    };
    mount.print();
    let mount_failed = mount.failed();
    outcomes.push(mount);

    // Abort early on mount failure — nothing else can proceed.
    if mount_failed {
        print_summary(&outcomes);
        anyhow::bail!("pipeline aborted: mount failed");
    }

    // 2. Init (only when repo does not yet exist)
    if !Path::new(&cfg.repo.path).exists() {
        // mkdir -p
        let mkdir = run_stage("Init (mkdir)", &build_mkdir_args(cli, cfg));
        mkdir.print();
        let failed = mkdir.failed();
        outcomes.push(mkdir);
        if failed {
            print_summary(&outcomes);
            anyhow::bail!("pipeline aborted: could not create repo directory");
        }

        // rustic init
        let init = run_stage("Init (repo)", &build_init_args(cli, cfg));
        init.print();
        let failed = init.failed();
        outcomes.push(init);
        if failed {
            print_summary(&outcomes);
            anyhow::bail!("pipeline aborted: rustic init failed");
        }
    }

    // 3. Check
    if !cli.no_check {
        let check = run_stage("Check", &build_check_args(cli, cfg));
        check.print();
        let failed = check.failed();
        outcomes.push(check);
        if failed {
            print_summary(&outcomes);
            anyhow::bail!("pipeline aborted: check failed");
        }
    }

    // 4. Backup
    let backup = run_stage("Backup", &build_backup_args(cli, cfg));
    backup.print();
    let backup_failed = backup.failed();
    outcomes.push(backup);
    if backup_failed {
        print_summary(&outcomes);
        anyhow::bail!("pipeline aborted: backup failed");
    }

    // 5 & 6. Forget + Compact
    if !cli.no_prune {
        let forget = run_stage("Forget", &build_forget_args(cli, cfg));
        forget.print();
        let failed = forget.failed();
        outcomes.push(forget);
        if failed {
            print_summary(&outcomes);
            anyhow::bail!("pipeline aborted: forget failed");
        }

        let compact = run_stage("Compact", &build_compact_args(cli, cfg));
        compact.print();
        let failed = compact.failed();
        outcomes.push(compact);
        if failed {
            print_summary(&outcomes);
            anyhow::bail!("pipeline aborted: compact failed");
        }
    }

    print_summary(&outcomes);
    Ok(())
}

// ─── Argument builders ────────────────────────────────────────────────────────
//
// Each function returns the full `Vec<String>` that will be passed to
// `run_stage`.  They are `pub` so that unit tests (and the snapshot tests
// below) can call them directly without needing `rustic` installed.

/// Arguments for `mkdir -p <repo>`.
pub fn build_mkdir_args(cli: &Cli, cfg: &Config) -> Vec<String> {
    let mut args = prefix(cli);
    args.extend(["mkdir".into(), "-p".into(), cfg.repo.path.clone()]);
    args
}

/// Arguments for `rustic init`.
pub fn build_init_args(cli: &Cli, cfg: &Config) -> Vec<String> {
    let mut cmd = rustic_base(cli, cfg);
    cmd.push("init".into());
    cmd
}

/// Arguments for `rustic check`.
pub fn build_check_args(cli: &Cli, cfg: &Config) -> Vec<String> {
    let mut cmd = rustic_base(cli, cfg);
    cmd.push("check".into());
    cmd
}

/// Arguments for `rustic backup …`.
///
/// Falls back to `"."` when `[backup].sources` is empty.
pub fn build_backup_args(cli: &Cli, cfg: &Config) -> Vec<String> {
    let mut cmd = rustic_base(cli, cfg);
    cmd.push("backup".into());
    cmd.extend([
        "--set-compression".into(),
        cfg.backup.compression.to_string(),
        "--exclude-if-present".into(),
        cfg.backup.exclude_if_present.clone(),
    ]);
    for glob in &cfg.backup.globs {
        cmd.push(format!("--glob={glob}"));
    }
    let sources: Vec<String> = if cfg.backup.sources.is_empty() {
        vec![".".into()]
    } else {
        cfg.backup.sources.clone()
    };
    cmd.extend(sources);
    cmd
}

/// Arguments for `rustic forget --prune …`.
pub fn build_forget_args(cli: &Cli, cfg: &Config) -> Vec<String> {
    let r = &cfg.retention;
    let mut cmd = rustic_base(cli, cfg);
    cmd.extend([
        "forget".into(),
        "--prune".into(),
        "--keep-daily".into(),
        r.daily.to_string(),
        "--keep-weekly".into(),
        r.weekly.to_string(),
        "--keep-monthly".into(),
        r.monthly.to_string(),
    ]);
    cmd
}

/// Arguments for `rustic prune`.
pub fn build_compact_args(cli: &Cli, cfg: &Config) -> Vec<String> {
    let mut cmd = rustic_base(cli, cfg);
    cmd.push("prune".into());
    cmd
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;
    use crate::config::{BackupConfig, MountConfig, RepoConfig, RetentionConfig};

    fn make_cli(extra: &[&str]) -> Cli {
        Cli::parse_from(std::iter::once("backup").chain(extra.iter().copied()))
    }

    fn make_cfg() -> Config {
        Config {
            repo: RepoConfig {
                path: "/tmp/repo".into(),
                password: "pw".into(),
            },
            backup: BackupConfig {
                sources: vec!["/home/alice/project".into()],
                compression: 3,
                globs: vec![
                    "!**/.git".into(),
                    "!tmp/".into(),
                    "!**/target/".into(),
                    "!**/node_modules/".into(),
                ],
                exclude_if_present: "ignore".into(),
            },
            retention: RetentionConfig {
                daily: 2,
                weekly: 1,
                monthly: 1,
            },
            mount: MountConfig {
                share: Some("new-backups".into()),
                user: None,
            },
        }
    }

    // ── unit assertions ───────────────────────────────────────────────────────

    #[test]
    fn backup_args_contain_compression() {
        let args = build_backup_args(&make_cli(&[]), &make_cfg());
        let idx = args.iter().position(|a| a == "--set-compression").unwrap();
        assert_eq!(args[idx + 1], "3");
    }

    #[test]
    fn backup_args_contain_exclude_marker() {
        let args = build_backup_args(&make_cli(&[]), &make_cfg());
        let idx = args
            .iter()
            .position(|a| a == "--exclude-if-present")
            .unwrap();
        assert_eq!(args[idx + 1], "ignore");
    }

    #[test]
    fn backup_args_globs_in_order() {
        let args = build_backup_args(&make_cli(&[]), &make_cfg());
        let globs: Vec<_> = args.iter().filter(|a| a.starts_with("--glob=")).collect();
        assert_eq!(globs[0], "--glob=!**/.git");
        assert_eq!(globs[1], "--glob=!tmp/");
    }

    #[test]
    fn backup_args_default_source_dot_when_empty() {
        let mut cfg = make_cfg();
        cfg.backup.sources.clear();
        let args = build_backup_args(&make_cli(&[]), &cfg);
        assert!(args.contains(&".".to_string()));
    }

    #[test]
    fn forget_args_have_all_retention_flags() {
        let args = build_forget_args(&make_cli(&[]), &make_cfg());
        assert!(args.contains(&"--prune".to_string()));
        let d = args.iter().position(|a| a == "--keep-daily").unwrap();
        assert_eq!(args[d + 1], "2");
    }

    #[test]
    fn mkdir_args_contain_repo_path() {
        let args = build_mkdir_args(&make_cli(&[]), &make_cfg());
        assert!(args.contains(&"-p".to_string()));
        assert!(args.contains(&"/tmp/repo".to_string()));
    }

    #[test]
    fn check_args_end_with_check() {
        let args = build_check_args(&make_cli(&[]), &make_cfg());
        assert_eq!(args.last().unwrap(), "check");
    }

    #[test]
    fn compact_args_end_with_prune() {
        let args = build_compact_args(&make_cli(&[]), &make_cfg());
        assert_eq!(args.last().unwrap(), "prune");
    }

    // ── insta snapshot tests ──────────────────────────────────────────────────
    // These lock down the exact argument vectors so any unintended change is
    // immediately visible in the diff.

    #[test]
    fn snapshot_backup_args_default() {
        insta::assert_debug_snapshot!(build_backup_args(&make_cli(&[]), &make_cfg()));
    }

    #[test]
    fn snapshot_backup_args_sudo() {
        insta::assert_debug_snapshot!(build_backup_args(&make_cli(&["--sudo"]), &make_cfg()));
    }

    #[test]
    fn snapshot_backup_args_empty_sources() {
        let mut cfg = make_cfg();
        cfg.backup.sources.clear();
        insta::assert_debug_snapshot!(build_backup_args(&make_cli(&[]), &cfg));
    }

    #[test]
    fn snapshot_backup_args_multiple_sources() {
        let mut cfg = make_cfg();
        cfg.backup.sources = vec!["/a".into(), "/b".into(), "/c".into()];
        insta::assert_debug_snapshot!(build_backup_args(&make_cli(&[]), &cfg));
    }

    #[test]
    fn snapshot_forget_args_default() {
        insta::assert_debug_snapshot!(build_forget_args(&make_cli(&[]), &make_cfg()));
    }

    #[test]
    fn snapshot_forget_args_custom_retention() {
        let mut cfg = make_cfg();
        cfg.retention.daily = 7;
        cfg.retention.weekly = 4;
        cfg.retention.monthly = 12;
        insta::assert_debug_snapshot!(build_forget_args(&make_cli(&[]), &cfg));
    }

    #[test]
    fn snapshot_mkdir_args() {
        insta::assert_debug_snapshot!(build_mkdir_args(&make_cli(&[]), &make_cfg()));
    }

    #[test]
    fn snapshot_check_args() {
        insta::assert_debug_snapshot!(build_check_args(&make_cli(&[]), &make_cfg()));
    }

    #[test]
    fn snapshot_compact_args() {
        insta::assert_debug_snapshot!(build_compact_args(&make_cli(&[]), &make_cfg()));
    }
}
