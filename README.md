# 🛡️ backup.rs

**A lightweight, configuration-driven [rustic] wrapper that turns complex backup pipelines into a single command.**

[![Rust 1.85+](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Stop wrestling with brittle, hand-edited shell scripts. `backup.rs` brings structure to your backups. Drop a `backup.toml` into any project, run `backup init`, and you’re protected.

---

## ✨ Features

* **Atomic Pipelines:** Runs the full lifecycle: Mount → Init → Check → Backup → Forget → Compact.
* **Clean UI:** Hides the "noise" of raw `rustic` output behind elegant [indicatif] spinners.
* **Intelligent Failures:** If a stage fails, the full logs are captured and displayed immediately for debugging.
* **Zero-Overhead:** A single binary you can drop anywhere—perfect for cron jobs.
* **Modern Rust:** Built with Rust 1.85 (Edition 2024) for safety and speed.

---

## 🚀 Quick Start

```sh
# 1. Install the binary globally
cargo install --path .

# 2. Setup your project directory
cd ~/projects/myapp
backup init          # Generates a smart backup.toml based on your environment

# 3. Tweak & Run
$EDITOR backup.toml  # Set your repo path and password
backup               # Execute the full pipeline
```

### The Experience
```text
  ⠹  Mounting NAS...
  ✓  Mount Complete
  ⠼  Checking Integrity...
  ✓  Check Passed
  ⠴  Backing Up...
  ✓  Snapshot Created
  ⠧  Applying Retention Policy...
  ✓  Forget/Prune Complete

  ✨ All stages completed successfully.
```

---

## 🛠️ Configuration

The `backup.toml` file is designed to be readable and flexible. Every section is optional, falling back to sensible defaults.

```toml
# backup.toml - Configuration for "myapp"
# ---------------------------------------

[repo]
# Path to the rustic repository (local path or rclone/sftp URI)
path     = "/home/yonas/nfs/backups/rustic/myapp"
# Encryption password. Leave empty ("") for no encryption.
password = ""

[mount]
# Optional helper to ensure your backup target is reachable.
# Executed as: [command] [target]
command = "mount-nas"
target  = "new-backups"

[backup]
# Paths to include in the snapshot.
sources = ["."]
# Zstd compression level (1-22). 3 is a balanced default.
compression = 3
# Skip any directory containing a file with this name.
exclude_if_present = "ignore"
# Glob patterns. "!" prefix denotes exclusion.
globs = [
    "!**/.git",
    "!**/target/",
    "!**/node_modules/",
]

[retention]
# Snapshot retention policy
keep_daily   = 7
keep_weekly  = 4
keep_monthly = 6
```

---

## ⚙️ Usage & Pipeline

| Stage | Command | Purpose | Skip Flag |
| :--- | :--- | :--- | :--- |
| **Mount** | `mount-nas` | Mounts your remote storage | `--no-mount` |
| **Init** | `rustic init` | Initializes repo (if missing) | *Auto-skip* |
| **Check** | `rustic check` | Verifies repository health | `--no-check` |
| **Backup**| `rustic backup`| Performs the actual snapshot | — |
| **Forget**| `rustic forget`| Prunes old snapshots | `--no-prune` |
| **Compact**| `rustic prune` | Reclaims physical disk space | `--no-prune` |

> [!TIP]
> Use `--sudo` to prefix commands with `doas` for privileged operations like mounting or accessing restricted system files.

---

## 🧪 Testing

We take reliability seriously. The test suite ensures that your backup logic is sound without needing to touch your real data.

* **Unit Tests:** Verify argument generation and config parsing without spawning `rustic`.
* **Snapshot Testing:** Powered by [insta] to ensure CLI arguments never drift unexpectedly.
* **E2E Tests:** Real-world integration tests that spawn `rustic` (run with `just e2e`).

```sh
# Run the core unit and integration suite
cargo test

# Review any changes to command-line argument snapshots
cargo insta review

# Run full end-to-end tests (requires rustic on PATH)
just e2e
```

---

## 📦 Dependencies

| Crate | Role |
| :--- | :--- |
| **Clap** | CLI parsing with a human touch |
| **Indicatif** | Smooth terminal animations & spinners |
| **Anyhow** | Ergonomic error reporting |
| **Serde/Toml** | Robust configuration handling |

---

## License

MIT

[rustic]: https://rustic.cli.rs
[indicatif]: https://docs.rs/indicatif
[insta]: https://insta.rs
