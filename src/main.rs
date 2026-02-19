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

mod cli;
mod commands;
mod config;
mod mount;
mod runner;
mod ui;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Subcommand};
use config::load_config;

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        // ── backup init ───────────────────────────────────────────────────────
        Some(Subcommand::Init) => {
            commands::init::run(&cli.config)?;
        },

        // ── backup (default pipeline) ─────────────────────────────────────────
        None => {
            let cfg = load_config(&cli.config)?;

            if cli.print_config {
                println!("{cfg:#?}");
                return Ok(());
            }

            commands::run::run(&cli, &cfg)?;
        },
    }

    Ok(())
}
