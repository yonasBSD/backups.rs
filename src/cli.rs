//! Command-line interface definition.
//!
//! All argument parsing lives here so the rest of the codebase can stay
//! agnostic to `clap`.  The `Cli` struct is parsed once in `main` and then
//! passed (by reference) into the command handlers.

use std::path::PathBuf;

use clap::Parser;

/// Top-level CLI arguments, shared across every subcommand.
#[derive(Parser, Debug)]
#[command(
    name    = "backup",
    about   = "A rustic backup wrapper driven by backup.toml",
    version,
    // Show a compact two-column help layout.
    help_template = "\
{before-help}{name} {version}
{about}

{usage-heading} {usage}

{all-args}{after-help}"
)]
#[allow(clippy::struct_excessive_bools)]
pub struct Cli {
    /// Path to the configuration file.
    ///
    /// Defaults to `backup.toml` in the current working directory.  Use
    /// `--config /path/to/other.toml` to point at a project-specific config
    /// stored elsewhere (useful when running from a cron job or a different
    /// working directory).
    #[arg(short, long, default_value = "backup.toml")]
    pub config: PathBuf,

    /// Subcommand to run.  Omit to run the full backup pipeline.
    #[command(subcommand)]
    pub command: Option<Subcommand>,

    /// Print the parsed configuration and exit without running anything.
    ///
    /// Handy for verifying that the TOML was loaded correctly before
    /// committing to a long backup run.
    #[arg(long)]
    pub print_config: bool,

    /// Skip the NAS mount step even if `[mount]` is configured.
    ///
    /// Useful when the share is already mounted, or when running on a machine
    /// where the mount helper is not available.
    #[arg(long)]
    pub no_mount: bool,

    /// Skip the `forget` and `prune` (compaction) steps.
    ///
    /// All snapshots are kept; no disk space is reclaimed.  Useful when you
    /// want a fast incremental backup without the overhead of pruning.
    #[arg(long)]
    pub no_prune: bool,

    /// Skip the repository integrity check before backing up.
    ///
    /// The check step reads every pack file index and verifies pack-file
    /// counts.  Skipping it speeds up runs on slow NAS links.
    #[arg(long)]
    pub no_check: bool,

    /// Elevate commands via `doas`.
    ///
    /// When set, `rustic` (and any mount commands) are prefixed with `doas`.
    /// Mirrors the convention used in the original shell script that this tool
    /// replaces.
    #[arg(long)]
    pub sudo: bool,
}

/// Explicit subcommands.  Running `backup` with no subcommand triggers the
/// default backup pipeline.
#[derive(clap::Subcommand, Debug, PartialEq)]
pub enum Subcommand {
    /// Scaffold a `backup.toml` in the current directory.
    ///
    /// The generated file is pre-populated with sensible defaults:
    /// - `[backup].sources` is set to the current working directory.
    /// - `[repo].path` uses `~/nfs/new-backups/rustic/<dirname>` as the target.
    /// - Common exclusion globs (`.git`, `target/`, `node_modules/`, ...) are included and ready to
    ///   be uncommented or removed.
    ///
    /// Exits with an error if `backup.toml` already exists to avoid
    /// accidental overwrites.
    Init,
}
