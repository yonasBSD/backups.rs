//! Subcommand handlers.
//!
//! Each file in this module corresponds to one user-facing command:
//!
//! | File          | Invocation          | Description                        |
//! |---------------|---------------------|------------------------------------|
//! | `init.rs`     | `backup init`       | Scaffold a `backup.toml`           |
//! | `run.rs`      | `backup` (default)  | Full backup pipeline               |

pub mod init;
pub mod run;
