//! Integration tests for the Docker backend.
//!
//! These tests require Docker to be running.
//! They are skipped if Docker is not available.

use std::process::Command;

use camino::Utf8PathBuf;

fn missouri_bin() -> Utf8PathBuf {
    Utf8PathBuf::try_from(assert_cmd::cargo_bin!("missouri").to_path_buf()).unwrap()
}

fn docker_available() -> bool {
    std::path::Path::new("/var/run/docker.sock").exists()
        || std::env::var("DOCKER_HOST").is_ok()
}

fn fixture_dir() -> Utf8PathBuf {
    Utf8PathBuf::from(format!(
        "{}/tests/missouri-msb",
        env!("CARGO_MANIFEST_DIR")
    ))
}

#[test]
fn docker_echo_runs_in_linux_container() {
    if !docker_available() {
        eprintln!("skipping: docker not available");
        return;
    }

    if !fixture_dir().exists() {
        eprintln!("skipping: fixture dir not found");
        return;
    }

    let output = Command::new(missouri_bin().as_str())
        .args(["run", "-d", fixture_dir().as_str(), "-v"])
        .output()
        .expect("failed to run missouri");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "missouri run failed:\nstdout: {stdout}\nstderr: {stderr}"
    );
}
