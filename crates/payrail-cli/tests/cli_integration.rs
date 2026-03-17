use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

fn payrail_cmd() -> assert_cmd::Command {
    cargo_bin_cmd!("payrail")
}

#[test]
fn help_exits_zero() {
    payrail_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("PayRail CLI"));
}

#[test]
fn version_exits_zero() {
    payrail_cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("payrail"));
}

#[test]
fn json_flag_accepted() {
    // --json alone without subcommand should show help/error but accept the flag
    payrail_cmd()
        .args(["--json", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("PayRail CLI"));
}

#[test]
fn no_color_flag_accepted() {
    payrail_cmd()
        .args(["--no-color", "--help"])
        .assert()
        .success();
}

#[test]
fn verbose_flag_accepted() {
    payrail_cmd()
        .args(["--verbose", "--help"])
        .assert()
        .success();
}

#[test]
fn quiet_flag_accepted() {
    payrail_cmd().args(["--quiet", "--help"]).assert().success();
}

#[test]
fn help_shows_subcommands() {
    payrail_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("knowledge"))
        .stdout(predicate::str::contains("generate"))
        .stdout(predicate::str::contains("conformance"))
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("doctor"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("reconciliation"))
        .stdout(predicate::str::contains("completions"));
}

#[test]
fn help_shows_global_flags() {
    payrail_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--json"))
        .stdout(predicate::str::contains("--verbose"))
        .stdout(predicate::str::contains("--quiet"))
        .stdout(predicate::str::contains("--no-color"));
}

#[test]
fn generate_missing_pack_exits_with_error() {
    payrail_cmd()
        .args(["generate", "nonexistent-provider"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("knowledge pack"));
}

#[test]
fn conformance_run_help_exits_zero() {
    payrail_cmd()
        .args(["conformance", "run", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("provider"));
}

#[test]
fn conformance_run_missing_adapter_exits_error() {
    payrail_cmd()
        .args(["conformance", "run", "test-provider"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("adapter"));
}

#[test]
fn doctor_runs_and_shows_checks() {
    // Doctor exits 1 when checks fail (env vars not set in test environment)
    payrail_cmd()
        .arg("doctor")
        .assert()
        .failure()
        .stdout(predicate::str::contains("PayRail Doctor"));
}

#[test]
fn init_help_exits_zero() {
    payrail_cmd()
        .args(["init", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--provider"));
}

#[test]
fn status_shows_output() {
    payrail_cmd()
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("PayRail Status"));
}

#[test]
fn completions_bash_exits_zero() {
    payrail_cmd()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("payrail"));
}

#[test]
fn completions_zsh_exits_zero() {
    payrail_cmd()
        .args(["completions", "zsh"])
        .assert()
        .success();
}

#[test]
fn completions_fish_exits_zero() {
    payrail_cmd()
        .args(["completions", "fish"])
        .assert()
        .success();
}

#[test]
fn knowledge_publish_help_exits_zero() {
    payrail_cmd()
        .args(["knowledge", "publish", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("PROVIDER"));
}

#[test]
fn knowledge_init_with_valid_provider() {
    let tmp = tempfile::tempdir().unwrap();
    payrail_cmd()
        .args(["knowledge", "init", "test-provider"])
        .current_dir(tmp.path())
        .assert()
        .success();
}

#[test]
fn knowledge_init_with_invalid_provider() {
    let tmp = tempfile::tempdir().unwrap();
    payrail_cmd()
        .args(["knowledge", "init", "Bad-Name"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid provider name"));
}

#[test]
fn help_shows_reconciliation_subcommand() {
    payrail_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("reconciliation"));
}

#[test]
fn reconciliation_shows_report() {
    payrail_cmd()
        .arg("reconciliation")
        .assert()
        .success()
        .stdout(predicate::str::contains("Reconciliation Report"));
}

#[test]
fn reconciliation_with_provider_filter() {
    payrail_cmd()
        .args(["reconciliation", "--provider", "peach_payments"])
        .assert()
        .success()
        .stdout(predicate::str::contains("peach_payments"));
}

#[test]
fn reconciliation_with_period_flag() {
    payrail_cmd()
        .args(["reconciliation", "--period", "7d"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Last 7d"));
}

#[test]
fn reconciliation_json_output_is_valid() {
    let output = payrail_cmd()
        .args(["--json", "reconciliation", "--provider", "test_provider"])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(parsed["provider"], "test_provider");
    assert_eq!(parsed["total_payments"], 0);
    assert!(parsed["period"]["start"].is_string());
    assert!(parsed["period"]["end"].is_string());
    assert_eq!(parsed["discrepancies"]["timing_delay"], 0);
    assert_eq!(parsed["resolutions"]["auto_resolved"], 0);
}

#[test]
fn reconciliation_json_with_period() {
    let output = payrail_cmd()
        .args([
            "--json",
            "reconciliation",
            "--provider",
            "peach",
            "--period",
            "1h",
        ])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(parsed["provider"], "peach");
    assert!((parsed["match_rate"].as_f64().unwrap() - 100.0).abs() < 0.01);
}

#[test]
fn reconciliation_invalid_period_rejected() {
    payrail_cmd()
        .args(["reconciliation", "--period", "99d"])
        .assert()
        .failure();
}

#[test]
fn knowledge_init_json_output() {
    let tmp = tempfile::tempdir().unwrap();
    let output = payrail_cmd()
        .args(["--json", "knowledge", "init", "test-provider"])
        .current_dir(tmp.path())
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(parsed["provider"], "test-provider");
    assert_eq!(parsed["action"], "initialized");
}
