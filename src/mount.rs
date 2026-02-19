//! NAS share mounting — replaces the external `mount-nas` shell script.
//!
//! # How it works
//!
//! 1. Runs `mount | grep <share>` to check whether the share is already mounted.  If so, returns a
//!    success outcome immediately.
//! 2. Creates the mountpoint (`/home/<user>/nfs/<share>`) with `mkdir -p`.
//! 3. Calls `doas mount -t nfs <server>:<export> <mountpoint>`.
//!
//! The server and NFS export path are looked up from the [`SHARES`] table,
//! which mirrors the mapping in the original `mount-nas` shell script.
//!
//! # Config
//!
//! ```toml
//! [mount]
//! share = "new-backups"   # name of the NFS share to mount
//! user  = "yonas"         # optional; defaults to $USER / $LOGNAME
//! ```
//!
//! Omit the `[mount]` section entirely (or omit `share`) to skip mounting.

use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::{config::MountConfig, ui::StageOutcome};

// ─── Share map ────────────────────────────────────────────────────────────────

/// Full NFS source string (`server:/export/path`) for `name`.
fn nfs_source(name: &str) -> Option<String> {
    match name {
        "new-documents" => Some("documents.lan:/documents".into()),
        "new-backups" => Some("nas.lan:/mnt/vol2/backups".into()),
        "isos" | "pictures" | "movies" | "videos" | "backups" | "owncloud" | "lan-share"
        | "repos" | "documents" => Some(format!("nas.lan:/mnt/vol1/{name}")),
        _ => None,
    }
}

// ─── Public entry point ───────────────────────────────────────────────────────

/// Mount the configured NAS share, returning a [`StageOutcome`].
///
/// Equivalent to running `mount-nas <share>` but implemented natively:
///
/// 1. If the share is already mounted, returns success immediately.
/// 2. Creates `/home/<user>/nfs/<share>` with `mkdir -p`.
/// 3. Runs `doas mount -t nfs <server>:<export> <mountpoint>`.
///
/// Returns a failed outcome (without panicking) if:
/// - `[mount].share` is not set in the config
/// - the share name is not in the known share map
/// - any subprocess fails
pub fn mount_share(cfg: &MountConfig) -> StageOutcome {
    match try_mount(cfg) {
        Ok(msg) => StageOutcome {
            label: "Mount".into(),
            success: true,
            stdout: msg,
            stderr: String::new(),
            error: None,
        },
        Err(e) => StageOutcome {
            label: "Mount".into(),
            success: false,
            stdout: String::new(),
            stderr: String::new(),
            error: Some(e.to_string()),
        },
    }
}

// ─── Implementation ───────────────────────────────────────────────────────────

fn try_mount(cfg: &MountConfig) -> Result<String> {
    let share = cfg
        .share
        .as_deref()
        .context("[mount].share is not set — add `share = \"new-backups\"` to backup.toml")?;

    let user = effective_user(cfg);
    let mountpoint = format!("/home/{user}/nfs/{share}");

    // ── 1. Already mounted? ───────────────────────────────────────────────────
    if is_mounted(share)? {
        return Ok(format!("{share} already mounted at {mountpoint}"));
    }

    // ── 2. Create mountpoint ──────────────────────────────────────────────────
    std::fs::create_dir_all(&mountpoint).with_context(|| format!("mkdir -p {mountpoint}"))?;

    // ── 3. Mount ──────────────────────────────────────────────────────────────
    let source = nfs_source(share).with_context(|| format!("unknown share name: '{share}'"))?;

    let status = Command::new("doas")
        .args(["mount", "-t", "nfs", &source, &mountpoint])
        .status()
        .context("failed to spawn doas mount")?;

    if !status.success() {
        bail!("doas mount -t nfs {source} {mountpoint} exited non-zero");
    }

    Ok(format!("mounted {source} → {mountpoint}"))
}

/// Check whether `share` appears in the output of `mount`.
///
/// Replicates `doas mount | grep "$1" | wc -l` and tests that the count is 1.
/// We use `doas mount` to match the original script's behaviour on systems
/// where unprivileged users cannot run `mount`.
fn is_mounted(share: &str) -> Result<bool> {
    let output = Command::new("doas")
        .arg("mount")
        .output()
        .context("failed to run doas mount")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let count = stdout.lines().filter(|l| l.contains(share)).count();
    Ok(count >= 1)
}

/// Resolve the effective username from config, `$USER`, or `$LOGNAME`.
fn effective_user(cfg: &MountConfig) -> String {
    cfg.user
        .clone()
        .or_else(|| std::env::var("USER").ok())
        .or_else(|| std::env::var("LOGNAME").ok())
        .unwrap_or_else(|| "user".into())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── nfs_source ────────────────────────────────────────────────────────────

    #[test]
    fn new_backups_points_to_vol2() {
        assert_eq!(
            nfs_source("new-backups").unwrap(),
            "nas.lan:/mnt/vol2/backups"
        );
    }

    #[test]
    fn new_documents_points_to_documents_lan() {
        assert_eq!(
            nfs_source("new-documents").unwrap(),
            "documents.lan:/documents"
        );
    }

    #[test]
    fn generic_shares_use_vol1() {
        for share in &[
            "isos",
            "pictures",
            "movies",
            "videos",
            "backups",
            "owncloud",
            "lan-share",
            "repos",
            "documents",
        ] {
            let src = nfs_source(share).unwrap_or_else(|| panic!("missing: {share}"));
            assert!(
                src.starts_with("nas.lan:/mnt/vol1/"),
                "{share} should be on vol1, got {src}"
            );
            assert!(
                src.ends_with(share),
                "{share} path should end with share name"
            );
        }
    }

    #[test]
    fn unknown_share_returns_none() {
        assert!(nfs_source("not-a-real-share").is_none());
    }

    // ── effective_user ────────────────────────────────────────────────────────

    #[test]
    fn config_user_takes_priority() {
        let cfg = MountConfig {
            share: Some("new-backups".into()),
            user: Some("alice".into()),
        };
        assert_eq!(effective_user(&cfg), "alice");
    }

    #[test]
    fn falls_back_to_env_when_no_config_user() {
        let cfg = MountConfig {
            share: Some("new-backups".into()),
            user: None,
        };
        let got = effective_user(&cfg);
        // Should be non-empty (either $USER, $LOGNAME, or the "user" fallback).
        assert!(!got.is_empty());
    }

    // ── mount_share error paths ───────────────────────────────────────────────

    #[test]
    fn mount_share_fails_when_share_not_set() {
        let cfg = MountConfig {
            share: None,
            user: None,
        };
        let outcome = mount_share(&cfg);
        assert!(!outcome.success);
        assert!(
            outcome
                .error
                .as_deref()
                .unwrap_or("")
                .contains("[mount].share")
        );
    }

    // ── insta snapshots ───────────────────────────────────────────────────────

    #[test]
    fn snapshot_nfs_sources() {
        let shares = [
            "new-backups",
            "new-documents",
            "isos",
            "pictures",
            "movies",
            "videos",
            "backups",
            "owncloud",
            "lan-share",
            "repos",
            "documents",
        ];
        let map: Vec<(&str, String)> = shares
            .iter()
            .map(|&s| (s, nfs_source(s).unwrap()))
            .collect();
        insta::assert_debug_snapshot!(map);
    }
}
