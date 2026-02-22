//! Illinois meta-tests: missouri testing itself using --config-dir .illinois.
//!
//! Each scenario is a pair of states (before → after) where the transition
//! runs `missouri run` against an embedded fixture and we verify the exit code.

use std::fs;

use assert_cmd::cargo::cargo_bin;
use camino::{Utf8Path, Utf8PathBuf};

fn missouri_bin() -> Utf8PathBuf {
    Utf8PathBuf::try_from(cargo_bin("missouri")).unwrap()
}

/// Create an Illinois scenario in a temp dir.
///
/// `fixture_name` - name of the fixture in tests/fixtures/ to embed
/// `expected_exit_code` - what missouri run should return (0 = pass, 1 = fail, 2 = error)
fn setup_illinois_scenario(fixture_name: &str, expected_exit_code: u8) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(tmp.path()).unwrap();
    let bin_path = missouri_bin();

    // Create "before" state
    let before = root.join("before");
    let before_illinois = before.join(".illinois");
    let before_bin = before_illinois.join("bin");
    fs::create_dir_all(&before_bin).unwrap();

    // Copy the fixture into before/fixture/
    let fixture_src = Utf8PathBuf::from(format!(
        "{}/tests/fixtures/{fixture_name}",
        env!("CARGO_MANIFEST_DIR")
    ));
    copy_dir_all(&fixture_src, &before.join("fixture"));

    // Create a placeholder exit_code.txt in before
    fs::write(before.join("exit_code.txt"), "placeholder\n").unwrap();

    // Create the transition script
    let script = format!(
        "#!/bin/sh\n\"{bin_path}\" run -d fixture > output.txt 2>&1\necho $? > exit_code.txt\n"
    );
    let script_path = before_bin.join("run-missouri");
    fs::write(&script_path, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    // Create illinois.yml for "before" state
    let yml = format!(
        r#"env:
  MISSOURI_BIN: "{bin_path}"
  PATH: "/usr/local/bin:/usr/bin:/bin"

transitions:
  - name: "run missouri on {fixture_name}"
    command: "run-missouri"
    target: "../after"
    comparators:
      files:
        - path: "output.txt"
          ignore: true
        - path: "fixture/"
          ignore: true
"#
    );
    fs::write(before_illinois.join("missouri.yml"), yml).unwrap();

    // Create "after" state
    let after = root.join("after");
    let after_illinois = after.join(".illinois");
    fs::create_dir_all(&after_illinois).unwrap();

    // after has the expected exit_code.txt
    fs::write(
        after.join("exit_code.txt"),
        format!("{expected_exit_code}\n"),
    )
    .unwrap();

    // Terminal state - no transitions
    fs::write(after_illinois.join("missouri.yml"), "{}").unwrap();

    tmp
}

fn copy_dir_all(src: &Utf8Path, dst: &Utf8Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let src_path = Utf8PathBuf::try_from(entry.path()).unwrap();
        let dst_path = dst.join(src_path.file_name().unwrap());
        let file_type = entry.file_type().unwrap();
        if file_type.is_dir() {
            copy_dir_all(&src_path, &dst_path);
        } else if file_type.is_symlink() {
            // Recreate symlinks as-is
            #[cfg(unix)]
            {
                let target = fs::read_link(&src_path).unwrap();
                std::os::unix::fs::symlink(&target, &dst_path).unwrap();
            }
        } else if file_type.is_file() {
            fs::copy(&src_path, &dst_path).unwrap();
            // Preserve executable permission
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let src_perms = fs::metadata(&src_path).unwrap().permissions();
                if src_perms.mode() & 0o111 != 0 {
                    fs::set_permissions(&dst_path, src_perms).unwrap();
                }
            }
        }
        // Skip sockets, pipes, and other special files
    }
}

fn run_illinois(tmp: &tempfile::TempDir) -> std::process::Output {
    let root = Utf8Path::from_path(tmp.path()).unwrap();
    std::process::Command::new(missouri_bin().as_str())
        .args(["--config-dir", ".illinois", "run", "-d", root.as_str()])
        .output()
        .unwrap()
}

/// Find flox binary on PATH, returns None if not available.
fn find_flox() -> Option<Utf8PathBuf> {
    let path_var = std::env::var("PATH").ok()?;
    for dir in path_var.split(':') {
        let candidate = Utf8PathBuf::from(dir).join("flox");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Create an Illinois scenario for a flox-based fixture.
///
/// The inner missouri run needs flox on PATH, and the fixture's .flox/ dir
/// must be present. The PATH is inherited from the outer environment so flox
/// can activate and provide tools to transitions.
fn setup_illinois_flox_scenario(fixture_name: &str, expected_exit_code: u8) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(tmp.path()).unwrap();
    let bin_path = missouri_bin();

    // Create "before" state
    let before = root.join("before");
    let before_illinois = before.join(".illinois");
    let before_bin = before_illinois.join("bin");
    fs::create_dir_all(&before_bin).unwrap();

    // Copy the fixture into before/fixture/
    let fixture_src = Utf8PathBuf::from(format!(
        "{}/tests/fixtures/{fixture_name}",
        env!("CARGO_MANIFEST_DIR")
    ));
    copy_dir_all(&fixture_src, &before.join("fixture"));

    // Create a placeholder exit_code.txt in before
    fs::write(before.join("exit_code.txt"), "placeholder\n").unwrap();

    // Script inherits PATH from env so flox and nix tools are available
    let script = format!(
        "#!/bin/sh\n\"{bin_path}\" run -d fixture > output.txt 2>&1\necho $? > exit_code.txt\n"
    );
    let script_path = before_bin.join("run-missouri");
    fs::write(&script_path, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    // Use the real PATH so flox, nix, and other tools are available
    let real_path = std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into());

    let yml = format!(
        r#"env:
  MISSOURI_BIN: "{bin_path}"
  PATH: "{real_path}"

transitions:
  - name: "run missouri on {fixture_name}"
    command: "run-missouri"
    target: "../after"
    comparators:
      files:
        - path: "output.txt"
          ignore: true
        - path: "fixture/"
          ignore: true
"#
    );
    fs::write(before_illinois.join("missouri.yml"), yml).unwrap();

    // Create "after" state
    let after = root.join("after");
    let after_illinois = after.join(".illinois");
    fs::create_dir_all(&after_illinois).unwrap();

    fs::write(
        after.join("exit_code.txt"),
        format!("{expected_exit_code}\n"),
    )
    .unwrap();

    fs::write(after_illinois.join("missouri.yml"), "{}").unwrap();

    tmp
}

// --- Illinois meta-tests ---

#[test]
fn illinois_trivial_passes() {
    let tmp = setup_illinois_scenario("01-trivial", 0);
    let output = run_illinois(&tmp);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "illinois failed:\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("PASS"), "expected PASS in output: {stdout}");
}

#[test]
fn illinois_linear_passes() {
    let tmp = setup_illinois_scenario("02-linear", 0);
    let output = run_illinois(&tmp);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "illinois failed:\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("PASS"), "expected PASS in output: {stdout}");
}

#[test]
fn illinois_branching_passes() {
    let tmp = setup_illinois_scenario("03-branching", 0);
    let output = run_illinois(&tmp);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "illinois failed:\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("PASS"), "expected PASS in output: {stdout}");
}

#[test]
fn illinois_custom_comparator_passes() {
    let tmp = setup_illinois_scenario("04-custom-comparator", 0);
    let output = run_illinois(&tmp);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "illinois failed:\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("PASS"), "expected PASS in output: {stdout}");
}

#[test]
fn illinois_env_vars_passes() {
    let tmp = setup_illinois_scenario("05-env-vars", 0);
    let output = run_illinois(&tmp);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "illinois failed:\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("PASS"), "expected PASS in output: {stdout}");
}

#[test]
fn illinois_ignore_passes() {
    let tmp = setup_illinois_scenario("06-ignore", 0);
    let output = run_illinois(&tmp);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "illinois failed:\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("PASS"), "expected PASS in output: {stdout}");
}

#[test]
fn illinois_cycle_fails() {
    let tmp = setup_illinois_scenario("07-cycle", 2);
    let output = run_illinois(&tmp);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "illinois failed:\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("PASS"), "expected PASS in output: {stdout}");
}

#[test]
fn illinois_dbt_flox_passes() {
    if find_flox().is_none() {
        eprintln!("skipping illinois_dbt_flox_passes: flox not found on PATH");
        return;
    }
    let tmp = setup_illinois_flox_scenario("08-dbt", 0);
    let output = run_illinois(&tmp);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "illinois failed:\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("PASS"), "expected PASS in output: {stdout}");
}

#[test]
fn illinois_meltano_flox_passes() {
    if find_flox().is_none() {
        eprintln!("skipping illinois_meltano_flox_passes: flox not found on PATH");
        return;
    }
    let tmp = setup_illinois_flox_scenario("09-meltano", 0);
    let output = run_illinois(&tmp);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "illinois failed:\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("PASS"), "expected PASS in output: {stdout}");
}

#[test]
fn illinois_uv_flox_passes() {
    if find_flox().is_none() {
        eprintln!("skipping illinois_uv_flox_passes: flox not found on PATH");
        return;
    }
    let tmp = setup_illinois_flox_scenario("10-uv", 0);
    let output = run_illinois(&tmp);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "illinois failed:\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("PASS"), "expected PASS in output: {stdout}");
}

#[test]
fn illinois_cargo_flox_passes() {
    if find_flox().is_none() {
        eprintln!("skipping illinois_cargo_flox_passes: flox not found on PATH");
        return;
    }
    let tmp = setup_illinois_flox_scenario("11-cargo", 0);
    let output = run_illinois(&tmp);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "illinois failed:\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("PASS"), "expected PASS in output: {stdout}");
}

#[test]
fn illinois_assertions_passes() {
    let tmp = setup_illinois_scenario("12-assertions", 0);
    let output = run_illinois(&tmp);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "illinois failed:\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("PASS"), "expected PASS in output: {stdout}");
}
