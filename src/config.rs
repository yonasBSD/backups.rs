//! Configuration types and loading logic.
//!
//! `Config` is a direct 1-to-1 mapping of `backup.toml`.  Every field has a
//! `Default` impl so the file is entirely optional — running `backup` without
//! any config file falls back to safe, minimal defaults and backs up the
//! current directory.
//!
//! # File format
//!
//! ```toml
//! [repo]
//! path     = "/home/alice/nfs/new-backups/rustic/my-project"
//! password = ""          # empty = no encryption
//!
//! [mount]
//! share = "new-backups"  # NFS share name
//! user  = "alice"        # optional; defaults to $USER
//!
//! [backup]
//! sources            = ["/home/alice/my-project"]
//! compression        = 3        # zstd level 1–22
//! exclude_if_present = "ignore" # skip dirs containing this sentinel file
//! globs              = ["!**/.git", "!tmp/", "!**/target/", "!**/node_modules/"]
//!
//! [retention]
//! keep_daily   = 2
//! keep_weekly  = 1
//! keep_monthly = 1
//! ```

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

// ─── Top-level ────────────────────────────────────────────────────────────────

/// Root configuration object, deserialised from `backup.toml`.
///
/// All four sections are optional; missing sections fall back to their
/// `Default` implementations.
#[derive(Debug, Deserialize, Serialize, Default)]
pub struct Config {
    /// rustic repository settings.
    #[serde(default)]
    pub repo: RepoConfig,

    /// Files and directories to include, and exclusion rules.
    #[serde(default)]
    pub backup: BackupConfig,

    /// Snapshot retention policy applied during `forget --prune`.
    #[serde(default)]
    pub retention: RetentionConfig,

    /// Optional NAS mount step that runs before everything else.
    #[serde(default)]
    pub mount: MountConfig,
}

// ─── [repo] ───────────────────────────────────────────────────────────────────

/// Settings for the rustic repository itself.
#[derive(Debug, Deserialize, Serialize)]
pub struct RepoConfig {
    /// Filesystem path (or `sftp:…` / `rclone:…` URI) for the repository.
    ///
    /// rustic will read and write pack files here.  The directory is created
    /// automatically on the first run if it does not exist.
    pub path: String,

    /// Encryption password.
    ///
    /// Set to `""` (empty string) to create an unencrypted repository.
    /// **Do not store real passwords in plain-text config files that are
    /// committed to version control.**  Consider using an environment
    /// variable or a secrets manager instead.
    pub password: String,
}

impl Default for RepoConfig {
    fn default() -> Self {
        Self {
            path: String::from("./.backup"),
            password: String::new(),
        }
    }
}

// ─── [backup] ─────────────────────────────────────────────────────────────────

/// What to back up and what to exclude.
#[derive(Debug, Deserialize, Serialize)]
pub struct BackupConfig {
    /// Paths to include in the snapshot.
    ///
    /// When empty (or omitted entirely), `backup` defaults to the current
    /// working directory (`.`), making it safe to run `backup` anywhere
    /// without editing the config.
    pub sources: Vec<String>,

    /// zstd compression level, 1 (fastest) – 22 (smallest).
    ///
    /// Level 3 is a good balance between speed and space; higher levels give
    /// diminishing returns on already-compressed data (e.g. media files).
    #[serde(default = "default_compression")]
    pub compression: u8,

    /// Glob patterns forwarded to rustic's `--glob` flag.
    ///
    /// Patterns starting with `!` exclude matching paths.  Evaluated in
    /// order; the last matching rule wins.
    #[serde(default = "default_globs")]
    pub globs: Vec<String>,

    /// If a directory contains a file with this name it is skipped entirely.
    ///
    /// Create an empty file called `ignore` (the default) inside any
    /// directory you never want backed up — build caches, scratch space, etc.
    #[serde(default = "default_exclude_marker")]
    pub exclude_if_present: String,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            sources: vec![],
            compression: default_compression(),
            globs: default_globs(),
            exclude_if_present: default_exclude_marker(),
        }
    }
}

// ─── [retention] ──────────────────────────────────────────────────────────────

/// How many snapshots to keep when pruning.
///
/// Passed directly to `rustic forget --prune`.  rustic selects the most
/// recent snapshot within each window, so `keep_daily = 2` keeps one
/// snapshot from each of the last two calendar days that had a backup.
#[derive(Debug, Deserialize, Serialize)]
pub struct RetentionConfig {
    /// Number of daily snapshots to retain.
    #[serde(default = "default_keep_daily")]
    pub daily: u32,

    /// Number of weekly snapshots to retain.
    #[serde(default = "default_keep_weekly")]
    pub weekly: u32,

    /// Number of monthly snapshots to retain.
    #[serde(default = "default_keep_monthly")]
    pub monthly: u32,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            daily: default_keep_daily(),
            weekly: default_keep_weekly(),
            monthly: default_keep_monthly(),
        }
    }
}

// ─── [mount] ──────────────────────────────────────────────────────────────────

/// Optional NAS share mount step.
///
/// When `share` is set, `backup` will mount the named NFS share before doing
/// anything else.  The server and export path are resolved from the built-in
/// share map in [`crate::mount`].  Omit the entire `[mount]` section (or
/// omit `share`) to skip mounting.
///
/// ```toml
/// [mount]
/// share = "new-backups"   # name of the NFS share to mount
/// user  = "yonas"         # optional; defaults to $USER / $LOGNAME
/// ```
#[derive(Debug, Deserialize, Serialize, Default)]
pub struct MountConfig {
    /// Name of the NFS share to mount, e.g. `"new-backups"`.
    #[serde(default)]
    pub share: Option<String>,

    /// Username used to build the mountpoint path (`/home/<user>/nfs/<share>`).
    /// Defaults to the `$USER` or `$LOGNAME` environment variable.
    #[serde(default)]
    pub user: Option<String>,
}

// ─── Defaults ─────────────────────────────────────────────────────────────────

// These free functions are required by `#[serde(default = "…")]` — serde
// cannot call `Default::default()` for individual fields, only for whole
// structs.

pub fn default_compression() -> u8 {
    3
}

pub fn default_globs() -> Vec<String> {
    vec![
        "!**/.git".into(),
        "!tmp/".into(),
        "!**/target/".into(),
        "!**/node_modules/".into(),
    ]
}

pub fn default_exclude_marker() -> String {
    "ignore".into()
}

pub fn default_keep_daily() -> u32 {
    2
}
pub fn default_keep_weekly() -> u32 {
    1
}
pub fn default_keep_monthly() -> u32 {
    1
}

// ─── Loader ───────────────────────────────────────────────────────────────────

/// Read and parse a `Config` from `path`.
///
/// If the file does not exist, a warning is printed to `stderr` and a
/// fully-defaulted `Config` is returned (backing up `.` into
/// `./.backup`).  This makes it safe to run `backup` in any directory
/// without a config file.
///
/// Returns an error if the file exists but cannot be read or is not valid
/// TOML.
pub fn load_config(path: &Path) -> Result<Config> {
    if !path.exists() {
        eprintln!(
            "Warning: config file '{}' not found, using defaults.\n\
             Run 'backup init' to generate a starter config.",
            path.display()
        );
        return Ok(Config::default());
    }

    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;

    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Defaults ─────────────────────────────────────────────────────────────

    #[test]
    fn default_config_is_safe() {
        let cfg = Config::default();
        // A default config must never point at a real or dangerous path.
        assert_eq!(cfg.repo.path, "./.backup");
        assert!(cfg.repo.password.is_empty());
    }

    #[test]
    fn default_backup_sources_are_empty() {
        // Empty sources → callers must substitute "." themselves.
        // This is tested here so the behaviour is explicit and visible.
        let cfg = BackupConfig::default();
        assert!(cfg.sources.is_empty());
    }

    #[test]
    fn default_compression_is_reasonable() {
        let cfg = BackupConfig::default();
        assert!(
            cfg.compression >= 1 && cfg.compression <= 22,
            "compression level {} is outside the valid zstd range 1–22",
            cfg.compression
        );
    }

    #[test]
    fn default_globs_exclude_git() {
        let globs = default_globs();
        assert!(
            globs.iter().any(|g| g.contains(".git")),
            "default globs should exclude .git directories"
        );
    }

    #[test]
    fn default_retention_keeps_at_least_one_snapshot() {
        let r = RetentionConfig::default();
        let total = r.daily + r.weekly + r.monthly;
        assert!(
            total >= 1,
            "retention policy must keep at least one snapshot"
        );
    }

    #[test]
    fn default_mount_is_none() {
        let m = MountConfig::default();
        assert!(m.share.is_none());
        assert!(m.user.is_none());
    }

    // ── Round-trip serialisation ──────────────────────────────────────────────

    #[test]
    fn config_roundtrips_through_toml() {
        let original = Config {
            repo: RepoConfig {
                path: "/tmp/test-repo".into(),
                password: "hunter2".into(),
            },
            backup: BackupConfig {
                sources: vec!["/home/alice/projects".into()],
                compression: 6,
                globs: vec!["!**/.git".into(), "!**/node_modules/".into()],
                exclude_if_present: "ignore".into(),
            },
            retention: RetentionConfig {
                daily: 7,
                weekly: 4,
                monthly: 3,
            },
            mount: MountConfig {
                share: Some("new-backups".into()),
                user: Some("alice".into()),
            },
        };

        let toml_str = toml::to_string(&original).expect("serialisation failed");
        let recovered: Config = toml::from_str(&toml_str).expect("deserialisation failed");

        assert_eq!(recovered.repo.path, original.repo.path);
        assert_eq!(recovered.repo.password, original.repo.password);
        assert_eq!(recovered.backup.sources, original.backup.sources);
        assert_eq!(recovered.backup.compression, original.backup.compression);
        assert_eq!(recovered.backup.globs, original.backup.globs);
        assert_eq!(recovered.retention.daily, original.retention.daily);
        assert_eq!(recovered.retention.weekly, original.retention.weekly);
        assert_eq!(recovered.retention.monthly, original.retention.monthly);
        assert_eq!(recovered.mount.share, original.mount.share);
        assert_eq!(recovered.mount.user, original.mount.user);
    }

    #[test]
    fn partial_toml_uses_defaults_for_missing_fields() {
        // A config with only [repo] should fill everything else with defaults.
        let toml_str = r#"
            [repo]
            path     = "/tmp/repo"
            password = ""
        "#;
        let cfg: Config = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(cfg.backup.compression, default_compression());
        assert_eq!(cfg.backup.globs, default_globs());
        assert_eq!(cfg.retention.daily, default_keep_daily());
        assert!(cfg.mount.share.is_none());
    }

    #[test]
    fn empty_toml_deserialises_to_defaults() {
        let cfg: Config = toml::from_str("").expect("empty toml should parse");
        assert_eq!(cfg.repo.path, "./.backup");
    }

    // ── load_config ───────────────────────────────────────────────────────────

    #[test]
    fn load_config_returns_defaults_for_missing_file() {
        let path = std::path::Path::new("/tmp/this-file-should-never-exist-abc123.toml");
        assert!(!path.exists(), "test precondition: file must not exist");

        let cfg = load_config(path).expect("should not error on missing file");
        assert_eq!(cfg.repo.path, "./.backup");
    }

    #[test]
    fn load_config_parses_valid_file() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            r#"
            [repo]
            path     = "/tmp/my-repo"
            password = "secret"
            "#
        )
        .unwrap();

        let cfg = load_config(f.path()).expect("should parse valid toml");
        assert_eq!(cfg.repo.path, "/tmp/my-repo");
        assert_eq!(cfg.repo.password, "secret");
    }

    #[test]
    fn load_config_errors_on_invalid_toml() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "not valid toml ][[[").unwrap();

        let result = load_config(f.path());
        assert!(result.is_err(), "invalid TOML should produce an error");
    }
}
