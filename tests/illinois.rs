//! Illinois meta-tests: missouri testing itself using --config-dir .illinois.
//!
//! Each scenario is a pair of states (before → after) where the transition
//! runs `missouri run` against an embedded fixture and we verify the exit code.

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};

fn missouri_bin() -> Utf8PathBuf {
    Utf8PathBuf::try_from(assert_cmd::cargo_bin!("missouri").to_path_buf()).unwrap()
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

/// Find nix binary on PATH, returns None if not available.
fn find_nix() -> Option<Utf8PathBuf> {
    let path_var = std::env::var("PATH").ok()?;
    for dir in path_var.split(':') {
        let candidate = Utf8PathBuf::from(dir).join("nix");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Create an Illinois scenario for a nix-sandboxed fixture.
///
/// The inner missouri run needs nix on PATH to provide tools via `nix shell`.
/// PATH, HOME, and TMPDIR are inherited from the outer environment so nix
/// can resolve packages and the inner processes have writable directories.
fn setup_illinois_nix_scenario(fixture_name: &str, expected_exit_code: u8) -> tempfile::TempDir {
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

    // Script inherits PATH from env so nix tools are available
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

    // Pass through PATH (for nix), HOME and TMPDIR (for writable dirs)
    let real_path = std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into());
    let real_home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let real_tmpdir = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".into());

    let yml = format!(
        r#"env:
  MISSOURI_BIN: "{bin_path}"
  PATH: "{real_path}"
  HOME: "{real_home}"
  TMPDIR: "{real_tmpdir}"

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
fn illinois_comparator_env_passes() {
    let tmp = setup_illinois_scenario("16-comparator-env", 0);
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
fn illinois_dbt_nix_passes() {
    if find_nix().is_none() {
        eprintln!("skipping illinois_dbt_nix_passes: nix not found on PATH");
        return;
    }
    let tmp = setup_illinois_nix_scenario("08-dbt", 0);
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
fn illinois_meltano_nix_passes() {
    if find_nix().is_none() {
        eprintln!("skipping illinois_meltano_nix_passes: nix not found on PATH");
        return;
    }
    let tmp = setup_illinois_nix_scenario("09-meltano", 0);
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
fn illinois_uv_nix_passes() {
    if find_nix().is_none() {
        eprintln!("skipping illinois_uv_nix_passes: nix not found on PATH");
        return;
    }
    let tmp = setup_illinois_nix_scenario("10-uv", 0);
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
fn illinois_cargo_nix_passes() {
    if find_nix().is_none() {
        eprintln!("skipping illinois_cargo_nix_passes: nix not found on PATH");
        return;
    }
    let tmp = setup_illinois_nix_scenario("11-cargo", 0);
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
fn illinois_comparator_env_nix_passes() {
    if find_nix().is_none() {
        eprintln!("skipping illinois_comparator_env_nix_passes: nix not found on PATH");
        return;
    }
    let tmp = setup_illinois_nix_scenario("17-comparator-env-flox", 0);
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

/// Verify that the meltano fixture's pipeline-ready state uses a pinned pip_url for tap-csv.
///
/// An unpinned git URL (no `@tag` or `@commit`) installs from HEAD, which changes
/// unpredictably. When the HEAD of MeltanoLabs/tap-csv changes in a breaking way,
/// `meltano run` fails to install the plugin and exits 1, causing the illinois test
/// to fail. Pinning to a specific tag ensures reproducible behavior.
#[test]
fn illinois_meltano_fixture_uses_pinned_pip_url() {
    let meltano_yml = fs::read_to_string(format!(
        "{}/tests/fixtures/09-meltano/pipeline-ready/meltano-project/meltano.yml",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("failed to read pipeline-ready meltano.yml");

    // The tap-csv pip_url must include a version specifier (@tag or @commit).
    // Unpinned form: "git+https://github.com/MeltanoLabs/tap-csv.git"
    // Pinned form:   "git+https://github.com/MeltanoLabs/tap-csv.git@v1.3.2"
    let tap_csv_pip_url_line = meltano_yml
        .lines()
        .find(|l| l.contains("tap-csv.git"))
        .expect("no tap-csv pip_url line found in pipeline-ready meltano.yml");

    assert!(
        tap_csv_pip_url_line.contains('@'),
        "tap-csv pip_url is unpinned (no @version): {tap_csv_pip_url_line}\n\
         Unpinned git URLs install from HEAD, which changes and can break meltano run.\n\
         Pin to a specific tag, e.g. git+https://github.com/MeltanoLabs/tap-csv.git@v1.3.2"
    );
}

// --- Recording illinois tests ---

/// Comparator script that validates .cast files have valid NDJSON structure.
const COMPARE_CAST: &str = r#"#!/bin/sh
# Validates asciicast v2 structure: header line + event lines
# Args: $1 = actual, $2 = expected (placeholder)
actual="$1"
if [ ! -f "$actual" ]; then
    echo "FAIL: file not found: $actual" >&2
    exit 1
fi
# Check first line is a JSON object with version key
head -1 "$actual" | python3 -c "
import sys, json
h = json.loads(sys.stdin.readline())
assert h.get('version') == 2, f'bad version: {h.get(\"version\")}'
assert isinstance(h.get('width'), int) and h['width'] > 0, 'bad width'
assert isinstance(h.get('height'), int) and h['height'] > 0, 'bad height'
" 2>&1 || { echo "FAIL: invalid cast header in $actual" >&2; exit 1; }
# Check remaining lines are event arrays
tail -n +2 "$actual" | python3 -c "
import sys, json
lines = [l for l in sys.stdin if l.strip()]
assert len(lines) > 0, 'no events'
prev_t = 0.0
for line in lines:
    ev = json.loads(line)
    assert isinstance(ev, list) and len(ev) == 3, f'bad event: {ev}'
    assert isinstance(ev[0], (int, float)) and ev[0] >= 0, f'bad time: {ev[0]}'
    assert ev[0] >= prev_t, f'non-monotonic: {ev[0]} < {prev_t}'
    assert ev[1] == 'o', f'bad type: {ev[1]}'
    assert isinstance(ev[2], str) and len(ev[2]) > 0, f'empty data'
    prev_t = ev[0]
" 2>&1 || { echo "FAIL: invalid cast events in $actual" >&2; exit 1; }
exit 0
"#;

/// Create a comparator that validates results.json for a passing fixture.
fn compare_results_json_pass(num_paths: usize, steps_per_path: &[usize]) -> String {
    let checks = steps_per_path
        .iter()
        .enumerate()
        .map(|(i, &n)| {
            format!(
                "assert len(d['paths'][{i}]['steps']) == {n}, f'path {i}: expected {n} steps, got {{len(d[\"paths\"][{i}][\"steps\"])}}'\n\
for s in d['paths'][{i}]['steps']:\n    assert s['passed'], f'path {i} step {{s[\"index\"]}} failed'"
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"#!/bin/sh
actual="$1"
python3 - "$actual" << 'PYEOF'
import sys, json
d = json.load(open(sys.argv[1]))
assert d['run_id'] == 'test', f'bad run_id: {{d["run_id"]}}'
assert len(d['paths']) == {num_paths}, f'expected {num_paths} paths, got {{len(d["paths"])}}'
assert d['passed'] == {num_paths}, f'expected {num_paths} passed'
assert d['failed'] == 0, f'expected 0 failed'
{checks}
for p in d['paths']:
    for s in p['steps']:
        assert 'cast_file' in s, f'missing cast_file in step'
PYEOF
if [ $? -ne 0 ]; then echo "FAIL: invalid results.json" >&2; exit 1; fi
exit 0
"#
    )
}

/// Create a comparator that validates results.json for a failing fixture.
fn compare_results_json_fail(
    num_paths: usize,
    num_passed: usize,
    num_failed: usize,
    failing_path: usize,
    failing_step: usize,
) -> String {
    format!(
        r#"#!/bin/sh
actual="$1"
python3 - "$actual" << 'PYEOF'
import sys, json
d = json.load(open(sys.argv[1]))
assert d['run_id'] == 'test', f'bad run_id: {{d["run_id"]}}'
assert len(d['paths']) == {num_paths}, f'expected {num_paths} paths'
assert d['passed'] == {num_passed}, f'expected {num_passed} passed, got {{d["passed"]}}'
assert d['failed'] == {num_failed}, f'expected {num_failed} failed, got {{d["failed"]}}'
p = d['paths'][{failing_path}]
assert not p['passed'], f'path {failing_path} should have failed'
assert p['steps'][{failing_step}]['passed'] == False, f'step {failing_step} should have failed'
assert p['steps'][{failing_step}]['exit_code'] != 0, f'step {failing_step} should have nonzero exit'
for s in p['steps']:
    assert 'cast_file' in s, f'missing cast_file'
PYEOF
if [ $? -ne 0 ]; then echo "FAIL: invalid results.json" >&2; exit 1; fi
exit 0
"#
    )
}

/// Create an Illinois recording scenario.
///
/// Runs `missouri run --record --run-id test` against the fixture and validates
/// the recording output structure with custom comparators.
fn setup_illinois_record_scenario(
    fixture_name: &str,
    expected_exit_code: u8,
    path_steps: &[usize],
    results_json_comparator: &str,
) -> tempfile::TempDir {
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

    // Placeholder exit_code.txt
    fs::write(before.join("exit_code.txt"), "placeholder\n").unwrap();

    // Transition script: run with --record
    let script = format!(
        "#!/bin/sh\n\"{bin_path}\" run --record --run-id test -d fixture > output.txt 2>&1\necho $? > exit_code.txt\n"
    );
    write_executable(&before_bin.join("run-missouri-record"), &script);

    // Comparator scripts
    write_executable(&before_bin.join("compare-cast"), COMPARE_CAST);
    write_executable(
        &before_bin.join("compare-results-json"),
        results_json_comparator,
    );

    // Build comparators config for each expected recording file
    let mut file_comparators = String::new();
    file_comparators.push_str("        - path: \"output.txt\"\n          ignore: true\n");
    file_comparators.push_str("        - path: \"fixture/.missouri/runs/test/results.json\"\n          command: \"compare-results-json\"\n");

    for (path_idx, &num_steps) in path_steps.iter().enumerate() {
        for step_idx in 0..num_steps {
            file_comparators.push_str(&format!(
                "        - path: \"fixture/.missouri/runs/test/path-{path_idx}/step-{step_idx}.cast\"\n          command: \"compare-cast\"\n"
            ));
        }
    }

    let yml = format!(
        r#"env:
  MISSOURI_BIN: "{bin_path}"
  PATH: "/usr/local/bin:/usr/bin:/bin"

transitions:
  - name: "missouri run --record on {fixture_name}"
    command: "run-missouri-record"
    target: "../after"
    comparators:
      files:
{file_comparators}"#
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

    // Copy the fixture into after/ too — the state directories don't change during a run,
    // so both before and after must have them to avoid spurious "extra file" diffs.
    copy_dir_all(&fixture_src, &after.join("fixture"));

    // Create placeholder files in after state for each expected recording file
    // (these overwrite whatever was in the copied fixture's .missouri/runs/)
    fs::create_dir_all(after.join("fixture/.missouri/runs/test")).unwrap();
    fs::write(
        after.join("fixture/.missouri/runs/test/results.json"),
        "placeholder\n",
    )
    .unwrap();
    for (path_idx, &num_steps) in path_steps.iter().enumerate() {
        let path_dir = after.join(format!("fixture/.missouri/runs/test/path-{path_idx}"));
        fs::create_dir_all(&path_dir).unwrap();
        for step_idx in 0..num_steps {
            fs::write(
                path_dir.join(format!("step-{step_idx}.cast")),
                "placeholder\n",
            )
            .unwrap();
        }
    }

    fs::write(after_illinois.join("missouri.yml"), "{}").unwrap();

    tmp
}

/// Same as setup_illinois_record_scenario but inherits real PATH/HOME/TMPDIR for nix.
fn setup_illinois_record_nix_scenario(
    fixture_name: &str,
    expected_exit_code: u8,
    path_steps: &[usize],
    results_json_comparator: &str,
) -> tempfile::TempDir {
    let tmp = setup_illinois_record_scenario(
        fixture_name,
        expected_exit_code,
        path_steps,
        results_json_comparator,
    );
    let root = Utf8Path::from_path(tmp.path()).unwrap();

    // Override the illinois config to use real PATH/HOME/TMPDIR (for nix)
    let real_path = std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into());
    let real_home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let real_tmpdir = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".into());

    // Re-read the existing yml to get the comparators section
    let existing = fs::read_to_string(root.join("before/.illinois/missouri.yml")).unwrap();
    // Replace the PATH line and add HOME/TMPDIR
    let updated = existing.replace(
        "  PATH: \"/usr/local/bin:/usr/bin:/bin\"",
        &format!("  PATH: \"{real_path}\"\n  HOME: \"{real_home}\"\n  TMPDIR: \"{real_tmpdir}\""),
    );
    fs::write(root.join("before/.illinois/missouri.yml"), updated).unwrap();

    tmp
}

fn write_executable(path: &Utf8Path, content: &str) {
    fs::write(path, content).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

// --- Recording illinois tests ---

#[test]
fn illinois_record_branching_passes() {
    // 03-branching: 2 paths, each with 1 step
    let comparator = compare_results_json_pass(2, &[1, 1]);
    let tmp = setup_illinois_record_scenario("03-branching", 0, &[1, 1], &comparator);
    let output = run_illinois(&tmp);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "illinois record branching failed:\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("PASS"), "expected PASS in output: {stdout}");
}

#[test]
fn illinois_record_dbt_nix_passes() {
    if find_nix().is_none() {
        eprintln!("skipping illinois_record_dbt_nix_passes: nix not found on PATH");
        return;
    }
    // 08-dbt: 2 paths — path 0 has 1 step (dbt-seeded→dbt-ran), path 1 has 2 steps (empty→uv-initialized→uv-added)
    let comparator = compare_results_json_pass(2, &[1, 2]);
    let tmp = setup_illinois_record_nix_scenario("08-dbt", 0, &[1, 2], &comparator);
    let output = run_illinois(&tmp);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "illinois record dbt failed:\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("PASS"), "expected PASS in output: {stdout}");
}

#[test]
fn illinois_services_passes() {
    let tmp = setup_illinois_scenario("21-services", 0);
    let output = run_illinois(&tmp);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "illinois services failed:\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("PASS"), "expected PASS in output: {stdout}");
}

#[test]
fn illinois_record_fail_mid_path() {
    // 15-fail-mid-path: 1 path with 3 transitions, step 1 fails.
    // Only steps 0 and 1 should produce recordings (step 1 ran but failed).
    // Step 2 never runs.
    let comparator = compare_results_json_fail(1, 0, 1, 0, 1);
    let tmp = setup_illinois_record_scenario("15-fail-mid-path", 1, &[2], &comparator);
    let output = run_illinois(&tmp);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "illinois record fail-mid-path failed:\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("PASS"), "expected PASS in output: {stdout}");
}
