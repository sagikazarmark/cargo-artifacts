use std::fs;

use assert_cmd::Command;
use serde_json::json;
use tempfile::TempDir;

fn compiler_artifact(source_path: &str) -> String {
    json!({
        "reason": "compiler-artifact",
        "package_id": "path+file:///workspace/app#0.1.0",
        "target": {
            "kind": ["bin"],
            "crate_types": ["bin"],
            "name": "app",
            "src_path": "/workspace/app/src/main.rs",
            "edition": "2024",
            "doc": true,
            "doctest": false,
            "test": true
        },
        "profile": {
            "opt_level": "0",
            "debuginfo": 2,
            "debug_assertions": true,
            "overflow_checks": true,
            "test": false
        },
        "features": [],
        "filenames": [source_path],
        "executable": null,
        "fresh": false
    })
    .to_string()
}

fn build_finished() -> String {
    json!({
        "reason": "build-finished",
        "success": true
    })
    .to_string()
}

fn write_fixture(temp: &TempDir, source_path: &str) -> std::path::PathBuf {
    let input = temp.path().join("build.log");
    fs::write(
        &input,
        format!("{}\n{}\n", compiler_artifact(source_path), build_finished()),
    )
    .unwrap();
    input
}

#[test]
fn direct_invocation_lists_final_artifacts() {
    let temp = TempDir::new().unwrap();
    let source_path = temp.path().join("target/debug/app");
    let source_path = source_path.to_string_lossy().into_owned();
    let input = write_fixture(&temp, &source_path);

    let mut command = Command::cargo_bin("cargo-artifacts").unwrap();
    command.args(["list", "--input"]).arg(input);

    command
        .assert()
        .success()
        .stdout(format!("{source_path}\n"))
        .stderr("");
}

#[test]
fn cargo_subcommand_shim_invocation_lists_final_artifacts() {
    let temp = TempDir::new().unwrap();
    let source_path = temp.path().join("target/debug/app");
    let source_path = source_path.to_string_lossy().into_owned();
    let input = write_fixture(&temp, &source_path);

    let mut command = Command::cargo_bin("cargo-artifacts").unwrap();
    command.args(["artifacts", "list", "--input"]).arg(input);

    command
        .assert()
        .success()
        .stdout(format!("{source_path}\n"))
        .stderr("");
}

#[test]
fn short_input_alias_lists_final_artifacts() {
    let temp = TempDir::new().unwrap();
    let source_path = temp.path().join("target/debug/app");
    let source_path = source_path.to_string_lossy().into_owned();
    let input = write_fixture(&temp, &source_path);

    let mut command = Command::cargo_bin("cargo-artifacts").unwrap();
    command.args(["list", "-i"]).arg(input);

    command
        .assert()
        .success()
        .stdout(format!("{source_path}\n"))
        .stderr("");
}
