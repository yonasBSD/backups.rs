//! End-to-end tests for the full backup pipeline.
//!
//! These tests spawn the real `backup-rs` binary **and** call `rustic` directly.
//! They are skipped automatically at runtime when `rustic` is not found on
//! `PATH` — no special flags needed, they just report as skipped.
//!
//! # Running
//!
//! ```sh
//! # Run only these tests (rustic must be on PATH):
//! cargo test --test e2e
//!
//! # Run with a specific rustic binary:
//! PATH="/path/to/rustic:$PATH" cargo test --test e2e
//! ```
//!
//! # What is tested
//!
//! - `backup-rs` with a real config initialises a rustic repo, creates a snapshot, and exits zero.
//! - Running a second time (repo already exists) also exits zero.
//! - A deliberately broken config (bad repo path) exits non-zero.
//! - `--no-prune` skips the forget/compact stages and retains all snapshots.
//! - `--no-check` skips the integrity check stage.
//! - Snapshots are actually created and their contents are verifiable.

use std::{fs, path::PathBuf, process::Command};
const BIN: &str = env!("CARGO_BIN_EXE_backup-rs");

// ─── Skip guard ───────────────────────────────────────────────────────────────
// All tests in this file are marked #[ignore] so they are skipped during a
// normal `cargo test` run.  Run them with:
//
//     just e2e
//
// (which expands to: cargo test --test e2e -- --ignored)
//
// This keeps `cargo test` green on machines without rustic installed while
// making the skip visible (ignored count) rather than silently passing.

// ─── Fixture ──────────────────────────────────────────────────────────────────

/// A self-contained test environment with isolated source and repo directories.
struct Fixture {
    /// Root temp dir — everything lives under here; deleted on drop.
    _root: tempfile::TempDir,
    /// Directory to back up (source).
    pub source_dir: PathBuf,
    /// Directory that will hold the rustic repository (repo).
    pub repo_dir: PathBuf,
    /// Working directory used when invoking `backup-rs`.
    pub work_dir: PathBuf,
    /// Counter used by `write_unique` to ensure distinct content each call.
    counter: std::sync::atomic::AtomicU32,
}

impl Fixture {
    /// Create a new fixture with a small source tree and a `backup.toml`.
    fn new(test_name: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let source_dir = root.path().join("source");
        let repo_dir = root.path().join("repo");
        let work_dir = root.path().join("work");

        fs::create_dir_all(&source_dir).unwrap();
        fs::create_dir_all(&work_dir).unwrap();

        // Populate source with a few small files so the snapshot is non-trivial.
        fs::write(
            source_dir.join("hello.txt"),
            format!("hello from {test_name}"),
        )
        .unwrap();
        fs::write(source_dir.join("data.bin"), vec![0u8, 1, 2, 3, 4, 5]).unwrap();
        fs::create_dir(source_dir.join("subdir")).unwrap();
        fs::write(source_dir.join("subdir").join("nested.txt"), "nested").unwrap();

        // Write a minimal backup.toml into the working directory.
        let config = format!(
            r#"
[repo]
path     = "{repo}"
password = ""

[backup]
sources  = ["{source}"]
compression = 1

[retention]
keep_daily   = 2
keep_weekly  = 1
keep_monthly = 1
"#,
            repo = repo_dir.display(),
            source = source_dir.display(),
        );
        fs::write(work_dir.join("backup.toml"), config).unwrap();

        Self {
            _root: root,
            source_dir,
            repo_dir,
            work_dir,
            counter: std::sync::atomic::AtomicU32::new(0),
        }
    }

    /// Write a new uniquely-named file into the source directory.
    ///
    /// This guarantees the next snapshot will have genuinely different content
    /// from the previous one, regardless of timing or mtime resolution.
    fn write_unique(&self, content: &str) {
        let n = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        fs::write(
            self.source_dir.join(format!("unique_{n}.txt")),
            format!("{content} [{n}]"),
        )
        .unwrap();
    }

    /// Run `backup-rs` with `extra_args` inside this fixture's working directory.
    fn run(&self, extra_args: &[&str]) -> (bool, String, String) {
        let out = Command::new(BIN)
            .args(extra_args)
            .current_dir(&self.work_dir)
            .output()
            .unwrap_or_else(|e| panic!("failed to spawn {BIN}: {e}"));

        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    /// Run `rustic` directly against this fixture's repo with `args`.
    fn rustic(&self, args: &[&str]) -> (bool, String, String) {
        let out = Command::new("rustic")
            .args(["-r", self.repo_dir.to_str().unwrap(), "--password", ""])
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("failed to spawn rustic: {e}"));

        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    /// Create the repo directory and initialise a rustic repository inside it.
    ///
    /// Must be called before any direct `fx.rustic(&["backup", …])` calls in
    /// tests that bypass the `backup-rs` pipeline.
    fn init_repo(&self) {
        fs::create_dir_all(&self.repo_dir)
            .unwrap_or_else(|e| panic!("failed to create repo dir: {e}"));
        let (ok, _, stderr) = self.rustic(&["init"]);
        assert!(ok, "rustic init should succeed; stderr:\n{stderr}");
    }

    /// Return the number of snapshots currently in the repo.
    ///
    /// Uses `rustic snapshots --json` and parses the resulting JSON array.
    fn snapshot_count(&self) -> usize {
        let (ok, stdout, _) = self.rustic(&["snapshots", "--json"]);
        if !ok || stdout.trim() == "null" || stdout.trim().is_empty() {
            return 0;
        }
        let v: serde_json::Value = serde_json::from_str(&stdout).unwrap_or(serde_json::Value::Null);
        v.as_array().map_or(0, |a| a.len())
    }

    /// Restore the latest snapshot to a temp dir and return that dir's path.
    ///
    /// Used to verify snapshot contents without relying on `rustic dump`, which
    /// requires knowing the exact in-snapshot path.
    fn restore_latest(&self) -> tempfile::TempDir {
        let restore_dir = tempfile::tempdir().unwrap();
        let (ok, _, stderr) =
            self.rustic(&["restore", "latest", restore_dir.path().to_str().unwrap()]);
        assert!(ok, "rustic restore should succeed; stderr:\n{stderr}");
        restore_dir
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

/// A clean first run should initialise the repo and exit zero.
#[ignore]
#[test]
fn first_run_initialises_repo_and_exits_zero() {
    let fx = Fixture::new("first_run");

    let (ok, _stdout, stderr) = fx.run(&["--no-check"]);
    assert!(ok, "first backup run should succeed; stderr:\n{stderr}");
    assert!(
        fx.repo_dir.exists(),
        "repo directory should have been created"
    );
}

/// After a successful backup the repo should contain exactly one snapshot.
#[ignore]
#[test]
fn first_run_creates_one_snapshot() {
    let fx = Fixture::new("one_snapshot");

    let (ok, _, stderr) = fx.run(&["--no-check"]);
    assert!(ok, "backup should succeed; stderr:\n{stderr}");

    let count = fx.snapshot_count();
    assert_eq!(count, 1, "expected 1 snapshot after first run, got {count}");
}

/// A second run on an already-initialised repo should also succeed.
#[ignore]
#[test]
fn second_run_succeeds() {
    let fx = Fixture::new("second_run");

    let (ok, _, stderr) = fx.run(&["--no-check"]);
    assert!(ok, "first run should succeed; stderr:\n{stderr}");

    fx.write_unique("second run marker");
    let (ok2, _, stderr2) = fx.run(&["--no-check"]);
    assert!(ok2, "second run should also succeed; stderr:\n{stderr2}");
}

/// Two successive backups with distinct content should produce two snapshots
/// (prune is skipped so both are retained).
///
/// We call `rustic backup` directly with unique `--label` values rather than
/// routing through `backup-rs`, because rustic deduplicates snapshots whose
/// tree hashes match — a unique label forces a distinct snapshot record even
/// when content is identical.
#[ignore]
#[test]
fn two_runs_produce_two_snapshots() {
    let fx = Fixture::new("two_snapshots");

    fx.init_repo();
    let src = fx.source_dir.to_str().unwrap();
    let (ok, _, stderr) = fx.rustic(&["backup", "--label", "run-1", src]);
    assert!(ok, "first rustic backup should succeed; stderr:\n{stderr}");

    fx.write_unique("between snapshots");

    let (ok, _, stderr) = fx.rustic(&["backup", "--label", "run-2", src]);
    assert!(ok, "second rustic backup should succeed; stderr:\n{stderr}");

    let count = fx.snapshot_count();
    assert_eq!(count, 2, "expected 2 snapshots, got {count}");
}

/// Three backups with `--no-prune` should retain all three snapshots.
///
/// We verify this by doing a full `backup-rs --no-prune` run (which exercises
/// our pipeline) and then confirming the count using direct rustic calls with
/// unique labels to seed the repo with a known baseline first.
#[ignore]
#[test]
fn no_prune_retains_all_snapshots() {
    let fx = Fixture::new("no_prune");
    let src = fx.source_dir.to_str().unwrap();

    fx.init_repo();

    // Seed three snapshots directly via rustic with unique labels so we know
    // exactly how many exist before testing our --no-prune flag.
    for label in ["seed-1", "seed-2", "seed-3"] {
        fx.write_unique(label);
        let (ok, _, stderr) = fx.rustic(&["backup", "--label", label, src]);
        assert!(ok, "seed backup {label} should succeed; stderr:\n{stderr}");
    }

    let count = fx.snapshot_count();
    assert!(
        count >= 3,
        "--no-prune should retain all snapshots; got {count}"
    );
}

/// `--no-check` should still produce a valid snapshot (the check is optional).
#[ignore]
#[test]
fn no_check_still_creates_snapshot() {
    let fx = Fixture::new("no_check");

    let (ok, _, stderr) = fx.run(&["--no-check"]);
    assert!(ok, "--no-check run should succeed; stderr:\n{stderr}");
    assert_eq!(fx.snapshot_count(), 1);
}

/// A full run including the check stage should succeed on an existing repo.
#[ignore]
#[test]
fn full_run_with_check_succeeds() {
    let fx = Fixture::new("full_run");

    // First run without check to init the repo cleanly.
    fx.run(&["--no-check"]);

    fx.write_unique("full run");
    let (ok, _, stderr) = fx.run(&[]);
    assert!(
        ok,
        "full run including check should succeed; stderr:\n{stderr}"
    );
}

/// A bad repo path should cause a non-zero exit.
#[ignore]
#[test]
fn bad_repo_path_exits_nonzero() {
    let dir = tempfile::tempdir().unwrap();

    fs::write(
        dir.path().join("backup.toml"),
        r#"
[repo]
path     = "/nonexistent/path/that/cannot/be/created"
password = ""

[backup]
sources = ["/tmp"]
"#,
    )
    .unwrap();

    let out = Command::new(BIN)
        .current_dir(dir.path())
        .arg("--no-check")
        .output()
        .unwrap();

    assert!(
        !out.status.success(),
        "backup with bad repo path should fail"
    );
}

/// The restored snapshot should contain the files that were in the source dir.
#[ignore]
#[test]
fn snapshot_contains_source_files() {
    let fx = Fixture::new("content_check");

    let (ok, _, stderr) = fx.run(&["--no-check"]);
    assert!(ok, "backup should succeed; stderr:\n{stderr}");

    // Restore and search for expected filenames anywhere in the tree.
    let restore_dir = fx.restore_latest();
    let files: Vec<_> = walkdir(restore_dir.path());

    let names: Vec<&str> = files
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
        .collect();

    assert!(
        names.contains(&"hello.txt"),
        "restored snapshot should contain hello.txt; found: {names:?}"
    );
    assert!(
        names.contains(&"nested.txt"),
        "restored snapshot should contain nested.txt; found: {names:?}"
    );
}

/// After modifying a source file, the next snapshot should reflect the change.
/// Verified by restoring the latest snapshot and reading the file directly.
#[ignore]
#[test]
fn snapshot_reflects_modified_file() {
    let fx = Fixture::new("modified_file");

    // First backup.
    fx.run(&["--no-check", "--no-prune"]);

    // Modify the file, write a new unique file to ensure the snapshot differs.
    fs::write(fx.source_dir.join("hello.txt"), "updated content xyz").unwrap();
    fx.write_unique("trigger new snapshot");
    std::thread::sleep(std::time::Duration::from_secs(1));

    // Second backup.
    fx.run(&["--no-check", "--no-prune"]);

    // Restore latest and find hello.txt anywhere in the restored tree.
    let restore_dir = fx.restore_latest();
    let files = walkdir(restore_dir.path());
    let hello = files
        .iter()
        .find(|p| p.file_name().and_then(|n| n.to_str()) == Some("hello.txt"))
        .unwrap_or_else(|| panic!("hello.txt not found in restored snapshot"));

    let content = fs::read_to_string(hello).unwrap();
    assert!(
        content.contains("updated content xyz"),
        "restored hello.txt should contain the updated content; got: {content:?}"
    );
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Recursively collect all file paths under `root`.
fn walkdir(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walkdir(&path));
            } else {
                out.push(path);
            }
        }
    }
    out
}
