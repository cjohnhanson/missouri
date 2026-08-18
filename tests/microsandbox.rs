//! Integration tests for the Docker backend.
//!
//! These tests require Docker to be running.
//! They are skipped if Docker is not available.

use std::process::Command;

use camino::Utf8PathBuf;

fn missouri_bin() -> Utf8PathBuf {
    Utf8PathBuf::try_from(assert_cmd::cargo_bin!("missouri").to_path_buf()).unwrap()
}

/// How long the probe waits for Docker to answer.
///
/// The daemon answers a local inspect in milliseconds. A wait this
/// long is generous and still bounded.
const DOCKER_PROBE: std::time::Duration = std::time::Duration::from_secs(5);

/// Whether Docker can actually run the fixture's image.
///
/// A reachable socket is not enough. A runner may have Docker with no
/// image pulled, and the run then fails on a 404 rather than skipping.
/// CI pulls the image explicitly, so a skip there would be a real
/// problem rather than a quiet pass.
///
/// The socket file existing is not enough either. Docker Desktop
/// leaves `/var/run/docker.sock` in place when it is stopped, and
/// `docker image inspect` then blocks on a daemon that never answers.
/// This probe once hung the whole suite past ten minutes on a machine
/// with Docker installed and not running, which reads as a hang rather
/// than as the skip it was written to be. So the probe has a deadline,
/// and a probe that does not answer counts as unavailable.
fn docker_available() -> bool {
    let socket = std::path::Path::new("/var/run/docker.sock").exists()
        || std::env::var("DOCKER_HOST").is_ok();
    if !socket {
        return false;
    }
    let Ok(mut child) = Command::new("docker")
        .args(["image", "inspect", FIXTURE_IMAGE])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return false;
    };
    let deadline = std::time::Instant::now() + DOCKER_PROBE;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) => {}
            Err(_) => return false,
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            eprintln!("docker did not answer within {DOCKER_PROBE:?}; treating it as unavailable");
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// The image the fixture runs in.
const FIXTURE_IMAGE: &str = "debian:bookworm-slim";

fn fixture_dir() -> Utf8PathBuf {
    Utf8PathBuf::from(format!("{}/tests/missouri-msb", env!("CARGO_MANIFEST_DIR")))
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
