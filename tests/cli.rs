//! Integration tests for the Ralph CLI.
//!
//! These tests verify the CLI binary behavior by running the actual executable
//! and checking output, exit codes, and file system effects.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

// -----------------------------------------------------------------------------
// Test helpers
// -----------------------------------------------------------------------------

/// Creates a Command for the ralph binary.
#[allow(deprecated)]
fn ralph() -> Command {
    Command::cargo_bin("ralph").expect("failed to find ralph binary")
}

/// Creates a Command for ralph running in a specific directory.
fn ralph_in(dir: &TempDir) -> Command {
    let mut cmd = ralph();
    cmd.current_dir(dir.path());
    cmd
}

// -----------------------------------------------------------------------------
// Help and version tests
// -----------------------------------------------------------------------------

#[test]
fn test_help_shows_all_commands() {
    ralph()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("ralph"))
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("loop"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("cancel"))
        .stdout(predicate::str::contains("revert"))
        .stdout(predicate::str::contains("clean"))
        .stdout(predicate::str::contains("image"));
}

#[test]
fn test_version_shows_version() {
    ralph()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("ralph"));
}

#[test]
fn test_init_help_shows_force_flag() {
    ralph()
        .args(["init", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--force"));
}

#[test]
fn test_loop_help_shows_all_options() {
    ralph()
        .args(["loop", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--max-iterations"))
        .stdout(predicate::str::contains("--unlimited"))
        .stdout(predicate::str::contains("--no-sandbox"))
        .stdout(predicate::str::contains("--prompt"))
        .stdout(predicate::str::contains("--provider"));
}

#[test]
fn test_image_help_shows_subcommands() {
    ralph()
        .args(["image", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("build"))
        .stdout(predicate::str::contains("pull"))
        .stdout(predicate::str::contains("status"));
}

// -----------------------------------------------------------------------------
// Init command tests
// -----------------------------------------------------------------------------

#[test]
fn test_init_creates_all_files() {
    let dir = TempDir::new().unwrap();

    ralph_in(&dir)
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("initialized successfully"));

    // Verify all files were created
    assert!(dir.path().join("ralph.toml").exists());
    assert!(dir.path().join("PROMPT_plan.md").exists());
    assert!(dir.path().join("PROMPT_build.md").exists());
    assert!(dir.path().join("AGENTS.md").exists());
    assert!(dir.path().join(".cursor/rules/ralph.mdc").exists());

    // Verify ralph.toml is valid TOML
    let toml_content = fs::read_to_string(dir.path().join("ralph.toml")).unwrap();
    assert!(toml_content.contains("[agent]"));
}

#[test]
fn test_init_skips_existing_without_force() {
    let dir = TempDir::new().unwrap();

    // Create existing file
    fs::write(dir.path().join("ralph.toml"), "# existing").unwrap();

    ralph_in(&dir)
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("already exists"))
        .stdout(predicate::str::contains("--force"));

    // Verify content was not overwritten
    let content = fs::read_to_string(dir.path().join("ralph.toml")).unwrap();
    assert_eq!(content, "# existing");
}

#[test]
fn test_init_force_overwrites_existing() {
    let dir = TempDir::new().unwrap();

    // Create existing file
    fs::write(dir.path().join("ralph.toml"), "# existing").unwrap();

    ralph_in(&dir)
        .args(["init", "--force"])
        .assert()
        .success()
        .stdout(predicate::str::contains("overwritten"));

    // Verify content was overwritten
    let content = fs::read_to_string(dir.path().join("ralph.toml")).unwrap();
    assert!(content.contains("[agent]"));
}

// -----------------------------------------------------------------------------
// Status command tests
// -----------------------------------------------------------------------------

#[test]
fn test_status_no_active_loop() {
    let dir = TempDir::new().unwrap();

    // Initialize project first
    ralph_in(&dir).arg("init").assert().success();

    ralph_in(&dir)
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("No active Ralph loop"));
}

#[test]
fn test_status_with_state_file() {
    let dir = TempDir::new().unwrap();

    // Initialize project
    ralph_in(&dir).arg("init").assert().success();

    // Create state file
    fs::create_dir_all(dir.path().join(".ralph")).unwrap();
    fs::write(
        dir.path().join(".ralph/state.toml"),
        r#"
active = true
iteration = 5
mode = "build"
started_at = "2024-01-01T00:00:00Z"
last_iteration_at = "2024-01-01T00:05:00Z"
error_count = 1
consecutive_errors = 0
idle_iterations = 0
"#,
    )
    .unwrap();

    ralph_in(&dir)
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("Iteration"))
        .stdout(predicate::str::contains("5"));
}

// -----------------------------------------------------------------------------
// Cancel command tests
// -----------------------------------------------------------------------------

#[test]
fn test_cancel_no_active_loop() {
    let dir = TempDir::new().unwrap();

    // Initialize project
    ralph_in(&dir).arg("init").assert().success();

    ralph_in(&dir)
        .arg("cancel")
        .assert()
        .success()
        .stdout(predicate::str::contains("No active Ralph loop"));
}

#[test]
fn test_cancel_active_loop() {
    let dir = TempDir::new().unwrap();

    // Initialize project
    ralph_in(&dir).arg("init").assert().success();

    // Create active state file
    fs::create_dir_all(dir.path().join(".ralph")).unwrap();
    fs::write(
        dir.path().join(".ralph/state.toml"),
        r#"
active = true
iteration = 3
mode = "build"
started_at = "2024-01-01T00:00:00Z"
last_iteration_at = "2024-01-01T00:03:00Z"
error_count = 0
consecutive_errors = 0
idle_iterations = 0
"#,
    )
    .unwrap();

    ralph_in(&dir)
        .arg("cancel")
        .assert()
        .success()
        .stdout(predicate::str::contains("cancelled"));

    // Verify state was updated
    let state = fs::read_to_string(dir.path().join(".ralph/state.toml")).unwrap();
    assert!(state.contains("active = false"));
}

// -----------------------------------------------------------------------------
// Clean command tests
// -----------------------------------------------------------------------------

#[test]
fn test_clean_removes_state() {
    let dir = TempDir::new().unwrap();

    // Initialize project
    ralph_in(&dir).arg("init").assert().success();

    // Create state file
    fs::create_dir_all(dir.path().join(".ralph")).unwrap();
    fs::write(dir.path().join(".ralph/state.toml"), "active = false").unwrap();

    ralph_in(&dir).arg("clean").assert().success();

    // State file should be removed
    assert!(!dir.path().join(".ralph/state.toml").exists());
    // Config files should remain
    assert!(dir.path().join("ralph.toml").exists());
}

#[test]
fn test_clean_all_removes_config_files() {
    let dir = TempDir::new().unwrap();

    // Initialize project
    ralph_in(&dir).arg("init").assert().success();

    // Create state file
    fs::create_dir_all(dir.path().join(".ralph")).unwrap();
    fs::write(dir.path().join(".ralph/state.toml"), "active = false").unwrap();

    ralph_in(&dir).args(["clean", "--all"]).assert().success();

    // All files should be removed
    assert!(!dir.path().join(".ralph/state.toml").exists());
    assert!(!dir.path().join("PROMPT_plan.md").exists());
    assert!(!dir.path().join("PROMPT_build.md").exists());
}

#[test]
fn test_clean_no_state_file() {
    let dir = TempDir::new().unwrap();

    // Initialize project but no state file
    ralph_in(&dir).arg("init").assert().success();

    ralph_in(&dir)
        .arg("clean")
        .assert()
        .success()
        .stdout(predicate::str::contains("No Ralph files found to clean"));
}

// -----------------------------------------------------------------------------
// Image command tests
// -----------------------------------------------------------------------------

#[test]
fn test_image_status_no_docker() {
    let dir = TempDir::new().unwrap();

    // Initialize project
    ralph_in(&dir).arg("init").assert().success();

    // Image status should work even without Docker (shows config info)
    ralph_in(&dir)
        .args(["image", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Image"));
}

// -----------------------------------------------------------------------------
// Revert command tests
// -----------------------------------------------------------------------------

#[test]
fn test_revert_not_a_git_repo() {
    let dir = TempDir::new().unwrap();

    // Initialize project
    ralph_in(&dir).arg("init").assert().success();

    ralph_in(&dir)
        .arg("revert")
        .assert()
        .failure()
        .stderr(predicate::str::contains("git"));
}

// -----------------------------------------------------------------------------
// Loop command tests (without running actual loop)
// -----------------------------------------------------------------------------

#[test]
fn test_loop_without_init() {
    let dir = TempDir::new().unwrap();

    // Don't init - create minimal config but no prompt file
    fs::write(
        dir.path().join("ralph.toml"),
        "[agent]\nprovider = \"claude\"",
    )
    .unwrap();

    ralph_in(&dir)
        .args(["loop", "plan"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Prompt file not found"))
        .stderr(predicate::str::contains("ralph init"));
}

#[test]
fn test_loop_invalid_provider() {
    let dir = TempDir::new().unwrap();

    // Initialize project
    ralph_in(&dir).arg("init").assert().success();

    ralph_in(&dir)
        .args(["loop", "build", "--provider", "invalid"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("provider"));
}

// -----------------------------------------------------------------------------
// Error message tests
// -----------------------------------------------------------------------------

#[test]
fn test_unknown_command_suggests_help() {
    ralph()
        .arg("unknown")
        .assert()
        .failure()
        .stderr(predicate::str::contains("help"));
}

#[test]
fn test_invalid_mode_shows_options() {
    let dir = TempDir::new().unwrap();
    ralph_in(&dir).arg("init").assert().success();

    ralph_in(&dir)
        .args(["loop", "invalid"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("plan").or(predicate::str::contains("build")));
}

// -----------------------------------------------------------------------------
// Verbose flag tests
// -----------------------------------------------------------------------------

#[test]
fn test_verbose_flag_global() {
    let dir = TempDir::new().unwrap();

    // -v should work as a global flag
    ralph_in(&dir).args(["-v", "init"]).assert().success();

    assert!(dir.path().join("ralph.toml").exists());
}
