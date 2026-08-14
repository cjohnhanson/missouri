//! Integration tests for the Docker backend.
//!
//! These tests require Docker to be running.
//! They are skipped if Docker is not available.

use std::process::Command;

use camino::Utf8PathBuf;

fn missouri_bin() -> Utf8PathBuf {
    Utf8PathBuf::try_from(assert_cmd::cargo_bin!("missouri").to_path_buf()).unwrap()
}

/// Whether Docker can actually run the fixture's image.
///
/// A reachable socket is not enough. A runner may have Docker with no
/// image pulled, and the run then fails on a 404 rather than skipping.
/// CI pulls the image explicitly, so a skip there would be a real
/// problem rather than a quiet pass.
fn docker_available() -> bool {
    let socket = std::path::Path::new("/var/run/docker.sock").exists()
        || std::env::var("DOCKER_HOST").is_ok();
    if !socket {
        return false;
    }
    Command::new("docker")
        .args(["image", "inspect", FIXTURE_IMAGE])
        .output()
        .is_ok_and(|o| o.status.success())
}

/// The image the fixture runs in.
const FIXTURE_IMAGE: &str = "debian:bookworm-slim";

fn fixture_dir() -> Utf8PathBuf {
    Utf8PathBuf::from(format!(
        "{}/tests/missouri-msb",
        env!("CARGO_MANIFEST_DIR")
    ))
}

#[test]
fn docker_echo_runs_in_linux_container() {
    if !docker_available() {
        eprintln!("skipping: docker cannot run {FIXTURE_IMAGE}");
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
