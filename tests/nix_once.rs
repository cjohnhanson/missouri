//! A suite that lists `packages` resolves them with one `nix shell` call,
//! not one call per command. A stand-in `nix` on PATH logs every call,
//! so the tests count calls without a nix daemon.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// The stand-in answers `flake metadata` with a pinned URL, accepts
/// `build`, and runs `shell ... --command <cmd>` the way nix does: the
/// package's `bin` in front of PATH, then exec. With `bad_capture`, it
/// answers the environment capture with output that holds no PATH.
fn write_fake_nix(dir: &Path, log: &Path, pkg_bin: &Path, bad_capture: bool) {
    let mode = if bad_capture { "bad" } else { "good" };
    let script = format!(
        r#"#!/bin/sh
printf '%s\n' "$*" >> '{log}'
case "$1" in
flake) printf '{{"url":"fake:pinned"}}\n'; exit 0 ;;
build) exit 0 ;;
shell)
	while [ $# -gt 0 ] && [ "$1" != "--command" ]; do shift; done
	shift
	if [ '{mode}' = bad ] && [ "$1" = /usr/bin/env ]; then
		printf 'not an environment'
		exit 0
	fi
	PATH='{pkg_bin}':"$PATH"
	export PATH
	exec "$@" ;;
esac
exit 2
"#,
        log = log.display(),
        pkg_bin = pkg_bin.display(),
    );
    let nix = dir.join("nix");
    fs::write(&nix, script).unwrap();
    fs::set_permissions(&nix, fs::Permissions::from_mode(0o755)).unwrap();
}

/// A suite with one package, `hello`, whose `hello-pkg` prints `hi`. One
/// transition and one assertion call it, so each runs through nix.
fn write_suite(root: &Path) {
    fs::create_dir_all(root.join(".missouri")).unwrap();
    fs::write(
        root.join(".missouri/missouri.yml"),
        "packages:\n  - hello\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("a/.missouri")).unwrap();
    fs::write(
        root.join("a/.missouri/missouri.yml"),
        "transitions:\n  - name: \"greet\"\n    command: \"hello-pkg > out.txt\"\n    target: \"../b\"\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("b/.missouri")).unwrap();
    fs::write(
        root.join("b/.missouri/missouri.yml"),
        "assertions:\n  - name: \"the package is on PATH\"\n    command: \"hello-pkg\"\n    stdout: \"hi\\n\"\n",
    )
    .unwrap();
    fs::write(root.join("b/out.txt"), "hi\n").unwrap();
}

struct Run {
    passed: bool,
    stdout: String,
    stderr: String,
    nix_calls: Vec<String>,
}

fn run_suite(bad_capture: bool) -> Run {
    let tmp = tempfile::tempdir().unwrap();
    let fake = tmp.path().join("fake-bin");
    let pkg_bin = tmp.path().join("pkg/bin");
    let suite = tmp.path().join("suite");
    let log = tmp.path().join("nix-calls.log");
    fs::create_dir_all(&fake).unwrap();
    fs::create_dir_all(&pkg_bin).unwrap();
    let hello = pkg_bin.join("hello-pkg");
    fs::write(&hello, "#!/bin/sh\necho hi\n").unwrap();
    fs::set_permissions(&hello, fs::Permissions::from_mode(0o755)).unwrap();
    write_fake_nix(&fake, &log, &pkg_bin, bad_capture);
    write_suite(&suite);

    let path = format!("{}:/usr/bin:/bin", fake.display());
    let output = assert_cmd::cargo_bin_cmd!("missouri")
        .args(["run", "-d"])
        .arg(&suite)
        .env_clear()
        .env("PATH", path)
        .output()
        .unwrap();
    Run {
        passed: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        nix_calls: fs::read_to_string(&log)
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect(),
    }
}

#[test]
fn a_suite_resolves_its_packages_once_and_runs_no_command_through_nix() {
    let run = run_suite(false);
    assert!(
        run.passed,
        "the suite failed\nstdout:\n{}\nstderr:\n{}",
        run.stdout, run.stderr
    );
    let per_command: Vec<&String> = run
        .nix_calls
        .iter()
        .filter(|call| call.contains("hello-pkg"))
        .collect();
    assert!(
        per_command.is_empty(),
        "a command ran through nix: {per_command:?}"
    );
    let shells = run
        .nix_calls
        .iter()
        .filter(|call| call.starts_with("shell"))
        .count();
    assert_eq!(
        shells, 2,
        "expected the warm-up and the capture only: {:?}",
        run.nix_calls
    );
    assert!(
        !run.stderr.contains("warning"),
        "no warning on a good capture: {}",
        run.stderr
    );
}

#[test]
fn an_unreadable_capture_warns_and_wraps_each_command_with_the_same_verdict() {
    let run = run_suite(true);
    assert!(
        run.passed,
        "the suite failed\nstdout:\n{}\nstderr:\n{}",
        run.stdout, run.stderr
    );
    assert!(
        run.stderr
            .contains("warning: each command runs in its own nix shell"),
        "expected the fallback warning: {}",
        run.stderr
    );
    let per_command = run
        .nix_calls
        .iter()
        .filter(|call| call.starts_with("shell") && call.contains("hello-pkg"))
        .count();
    assert_eq!(
        per_command, 2,
        "expected the transition and the assertion through nix: {:?}",
        run.nix_calls
    );
}
