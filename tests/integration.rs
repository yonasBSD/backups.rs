//! Integration tests for the `backup-rs` binary.
//!
//! These tests exercise the CLI layer end-to-end: they spawn the actual compiled
//! binary and assert on exit codes, stdout, and stderr.  `rustic` is **not**
//! required — these tests cover argument parsing, config loading, `backup init`,
//! `--print-config`, and error paths that never reach the rustic invocation.
//!
//! # Running
//!
//! ```sh
//! cargo test --test integration
//! ```

use std::{fs, process::Command};

/// Absolute path to the compiled `backup-rs` binary, resolved at compile time
/// by Cargo.  This works correctly for both `cargo test` and `cargo test
/// --release` without any hardcoding.
const BIN: &str = env!("CARGO_BIN_EXE_backup-rs");

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Run `backup-rs` with `args` in a fresh temporary directory.
///
/// Returns `(exit_success, stdout, stderr)`.
fn run(args: &[&str]) -> (bool, String, String) {
    run_in(args, &std::env::temp_dir())
}

/// Run `backup-rs` with `args` in the given working directory.
fn run_in(args: &[&str], dir: &std::path::Path) -> (bool, String, String) {
    let out = Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn {BIN}: {e}"));

    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// ─── --help / --version ───────────────────────────────────────────────────────

#[test]
fn help_exits_zero() {
    let (ok, stdout, _) = run(&["--help"]);
    assert!(ok, "backup-rs --help should exit 0");
    assert!(
        stdout.contains("backup"),
        "help text should mention the binary name"
    );
}

#[test]
fn version_exits_zero() {
    let (ok, stdout, _) = run(&["--version"]);
    assert!(ok, "--version should exit 0");
    assert!(
        stdout.contains("0.1.0"),
        "--version should print the version"
    );
}

#[test]
fn init_help_exits_zero() {
    let (ok, stdout, _) = run(&["init", "--help"]);
    assert!(ok);
    assert!(stdout.to_lowercase().contains("init") || stdout.to_lowercase().contains("scaffold"));
}

// ─── backup init ─────────────────────────────────────────────────────────────

#[test]
fn init_creates_backup_toml() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, _, _) = run_in(&["init"], dir.path());
    assert!(ok, "backup-rs init should exit 0");

    let toml_path = dir.path().join("backup.toml");
    assert!(toml_path.exists(), "backup.toml should be created");

    let content = fs::read_to_string(&toml_path).unwrap();
    assert!(content.contains("[repo]"));
    assert!(content.contains("[backup]"));
    assert!(content.contains("[retention]"));
}

#[test]
fn init_with_custom_config_path() {
    let dir = tempfile::tempdir().unwrap();
    let custom = dir.path().join("custom.toml");
    let (ok, _, _) = run_in(&["--config", custom.to_str().unwrap(), "init"], dir.path());
    assert!(ok);
    assert!(custom.exists(), "custom.toml should be created");
}

#[test]
fn init_refuses_to_overwrite_existing_config() {
    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("backup.toml");
    fs::write(&toml_path, "# existing").unwrap();

    let (ok, stdout, stderr) = run_in(&["init"], dir.path());
    assert!(!ok, "init should fail when backup.toml already exists");

    // The original content must be untouched.
    assert_eq!(fs::read_to_string(&toml_path).unwrap(), "# existing");

    // The conflict message goes to stdout via StageOutcome::print().
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("already exists") || combined.contains("refusing"),
        "error message should explain why init failed; got: {combined}"
    );
}

#[test]
fn init_populates_sources_with_cwd() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, _, _) = run_in(&["init"], dir.path());
    assert!(ok);

    let content = fs::read_to_string(dir.path().join("backup.toml")).unwrap();
    let cwd = dir.path().to_string_lossy();
    assert!(
        content.contains(cwd.as_ref()),
        "generated config should contain the cwd '{cwd}' in sources"
    );
}

#[test]
fn init_generated_config_is_valid_toml() {
    let dir = tempfile::tempdir().unwrap();
    run_in(&["init"], dir.path());

    let content = fs::read_to_string(dir.path().join("backup.toml")).unwrap();

    // Strip trailing inline comments that some toml parsers choke on, then
    // verify the file parses without error.
    let stripped: String = content
        .lines()
        .map(|l| {
            if let Some(i) = l.find("   #") {
                &l[..i]
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    toml::from_str::<toml::Value>(&stripped).expect("generated backup.toml must be valid TOML");
}

// ─── --print-config ───────────────────────────────────────────────────────────

#[test]
fn print_config_exits_zero_with_valid_config() {
    let dir = tempfile::tempdir().unwrap();
    // Generate a config first.
    run_in(&["init"], dir.path());

    let (ok, stdout, _) = run_in(&["--print-config"], dir.path());
    assert!(ok, "--print-config should exit 0 when config is valid");
    // The debug output should contain the struct field names.
    assert!(stdout.contains("repo") || stdout.contains("RepoConfig"));
}

#[test]
fn print_config_exits_zero_with_missing_config() {
    // No backup.toml — falls back to defaults, should still print and exit 0.
    let dir = tempfile::tempdir().unwrap();
    let (ok, _, _) = run_in(&["--print-config"], dir.path());
    assert!(
        ok,
        "--print-config should exit 0 even without a config file"
    );
}

#[test]
fn print_config_errors_on_invalid_toml() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("backup.toml"), "not valid toml ][[[").unwrap();

    let (ok, _, _) = run_in(&["--print-config"], dir.path());
    assert!(!ok, "invalid TOML should cause a non-zero exit");
}

// ─── --config flag ────────────────────────────────────────────────────────────

#[test]
fn config_flag_reads_specified_file() {
    let dir = tempfile::tempdir().unwrap();

    // Write a minimal valid config at a non-default path.
    let cfg_path = dir.path().join("myconfig.toml");
    fs::write(
        &cfg_path,
        r#"
[repo]
path     = "/tmp/test-repo-xyz"
password = ""
"#,
    )
    .unwrap();

    let (ok, stdout, _) = run_in(
        &["--config", cfg_path.to_str().unwrap(), "--print-config"],
        dir.path(),
    );
    assert!(ok);
    assert!(
        stdout.contains("test-repo-xyz"),
        "should have loaded the specified config file"
    );
}

// ─── unknown flags ────────────────────────────────────────────────────────────

#[test]
fn unknown_flag_exits_nonzero() {
    let (ok, _, _) = run(&["--this-flag-does-not-exist"]);
    assert!(!ok, "unknown flag should exit non-zero");
}
