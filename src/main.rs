//! `backup` — a rustic backup wrapper driven by `backup.toml`.
//!
//! # Overview
//!
//! This binary is a thin orchestration layer around [`rustic`](https://rustic.cli.rs).
//! It replaces a family of hand-edited shell scripts with a single, config-
//! driven tool: drop a `backup.toml` next to a project, run `backup`, done.
//!
//! # Usage
//!
//! ```text
//! backup                 # run the full backup pipeline using backup.toml
//! backup init            # scaffold a backup.toml in the current directory
//! backup --print-config  # show parsed config without running anything
//! backup --no-prune      # skip forget/prune (fast incremental snapshot)
//! backup --sudo          # prefix all commands with doas
//! ```
//!
//! # Module layout
//!
//! | Module                   | Responsibility                              |
//! |--------------------------|---------------------------------------------|
//! | [`cli`]                  | Argument types parsed by clap               |
//! | [`config`]               | `Config` struct + TOML loader               |
//! | [`runner`]               | Argument construction helpers               |
//! | [`ui`]                   | Spinner, captured execution, stage output   |
//! | [`commands::init`]       | `backup init` subcommand                    |
//! | [`commands::run`]        | Default backup pipeline                     |
//! | [`mount`]                | Built-in NFS share mounting                 |

mod cli;
mod commands;
mod config;
mod mount;
mod runner;
mod ui;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Subcommand};
use config::{PartialConfig, parse_partial};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        // ── backup init ───────────────────────────────────────────────────────
        Some(Subcommand::Init) => {
            commands::init::run(&cli.config)?;
        },

        // ── backup (default pipeline) ─────────────────────────────────────────
        None => {
            let cfg = load_merged_config(&cli.config)?;

            if cli.print_config {
                println!("{cfg:#?}");
                return Ok(());
            }

            commands::run::run(&cli, &cfg)?;
        },
    }

    Ok(())
}

/// Load configuration from two sources and merge them.
///
/// 1. `~/.config/backup.rs/config.toml` — global defaults (e.g. `[mount]` share/user)
/// 2. `local_path` (default: `./backup.toml`) — per-project overrides
///
/// Local values win on a per-field basis.  Either file may be absent.
fn load_merged_config(local_path: &std::path::Path) -> Result<config::Config> {
    let global_path = dirs_next::config_dir().map(|d| d.join("backup.rs").join("config.toml"));

    let global: PartialConfig = global_path
        .as_deref()
        .and_then(|p| parse_partial(p).ok().flatten())
        .unwrap_or_default();

    let local: PartialConfig = if let Some(p) = parse_partial(local_path)? {
        p
    } else {
        eprintln!(
            "Warning: config file '{}' not found, using defaults.\n\
             Run 'backup init' to generate a starter config.",
            local_path.display()
        );
        PartialConfig::default()
    };

    Ok(global.merge(local).resolve())
}
