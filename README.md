# 🛡️ backup.rs

**A lightweight, configuration-driven [rustic] wrapper that turns complex backup pipelines into a single command.**

[![Rust 1.85+](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Stop wrestling with brittle, hand-edited shell scripts. `backup.rs` brings structure to your backups. Drop a `backup.toml` into any project, run `backup init`, and you're protected.

---

## ✨ Features

* **Atomic Pipelines:** Runs the full lifecycle: Mount → Init → Check → Backup → Forget → Compact.
* **Clean UI:** Hides the "noise" of raw `rustic` output behind elegant [indicatif] spinners.
* **Intelligent Failures:** If a stage fails, the full logs are captured and displayed immediately for debugging.
* **Built-in NFS Mounting:** Knows how to mount your NAS shares natively — no external `mount-nas` script required.
* **Zero-Overhead:** A single binary you can drop anywhere — perfect for cron jobs.
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
  ✓  Mount
  ⠼  Checking Integrity...
  ✓  Check
  ⠴  Backing Up...
  ✓  Backup
  ⠧  Applying Retention Policy...
  ✓  Forget

  ✓ All stages completed successfully.
```

---

## 🛠️ Configuration

The `backup.toml` file is designed to be readable and flexible. Every section is optional, falling back to sensible defaults.

```toml
# backup.toml - Configuration for "myapp"
# ---------------------------------------

[repo]
# Path to the rustic repository (local path or rclone/sftp URI)
path     = "/home/alice/nfs/new-backups/rustic/myapp"
# Encryption password. Leave empty ("") for no encryption.
password = ""

[mount]
# Optional: mount a NAS share before backing up.
# The share name is resolved to the correct NFS server and export path
# automatically — no external script required.
# Supported: new-backups, new-documents, isos, pictures, movies, videos,
#            backups, owncloud, lan-share, repos, documents
share = "new-backups"
# user = "alice"   # defaults to $USER if omitted

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
daily   = 7
weekly  = 4
monthly = 6
```

---

## ⚙️ Usage & Pipeline

| Stage | What it does | Skip Flag |
| :--- | :--- | :--- |
| **Mount** | Mounts the configured NAS share natively via NFS | `--no-mount` |
| **Init** | Initialises the repo (auto-skipped if it already exists) | *Auto-skip* |
| **Check** | Verifies repository integrity (`rustic check`) | `--no-check` |
| **Backup** | Creates a new snapshot (`rustic backup`) | — |
| **Forget** | Applies retention policy (`rustic forget --prune`) | `--no-prune` |
| **Compact** | Reclaims disk space (`rustic prune`) | `--no-prune` |

> [!TIP]
> Use `--sudo` to prefix `rustic` commands with `doas` for privileged operations like accessing restricted system files.

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
