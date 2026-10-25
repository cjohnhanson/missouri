//! A suite that lists `packages` resolves them with one `nix shell` call,
//! not one call per command. A stand-in `nix` on PATH logs every call,
//! so the tests count calls without a nix daemon.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// How the stand-in `nix` answers warm-up.
#[derive(Clone, Copy, PartialEq)]
enum Nix {
    /// Like nix: the capture prints the environment, the build succeeds.
    Good,
    /// The capture prints output that holds no PATH.
    UnreadableCapture,
    /// The capture prints a good environment, then exits 1.
    CaptureExits1,
    /// The build that roots the store paths exits 1.
    BuildFails,
}

/// The stand-in answers `flake metadata` with a pinned URL, answers
/// `build` by its mode, and runs `shell ... --command <cmd>` the way nix
/// does: the package's `bin` in front of PATH, then exec.
fn write_fake_nix(dir: &Path, log: &Path, pkg_bin: &Path, mode: Nix) {
    let mode = match mode {
        Nix::Good => "good",
        Nix::UnreadableCapture => "unreadable",
        Nix::CaptureExits1 => "capture-exits-1",
        Nix::BuildFails => "build-fails",
    };
    let script = format!(
        r#"#!/bin/sh
printf '%s\n' "$*" >> '{log}'
case "$1" in
flake) printf '{{"url":"fake:pinned"}}\n'; exit 0 ;;
build) [ '{mode}' = build-fails ] && exit 1; exit 0 ;;
shell)
	while [ $# -gt 0 ] && [ "$1" != "--command" ]; do shift; done
	shift
	PATH='{pkg_bin}':"$PATH"
	export PATH
	if [ "$1" = /usr/bin/env ]; then
		[ '{mode}' = unreadable ] && {{ printf 'not an environment'; exit 0; }}
		[ '{mode}' = capture-exits-1 ] && {{ "$@"; exit 1; }}
	fi
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

fn executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// A suite with one package, `hello`. Its `hello-pkg` prints `hi`, and
/// its `cmp-pkg` compares two files and logs that it ran. A shell
/// transition, a shell assertion, a `shell: false` assertion, and a file
/// comparator each call a program of the package.
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
        r#"transitions:
  - name: "greet"
    command: "hello-pkg > out.txt"
    target: "../b"
    comparators:
      files:
        - path: "out.txt"
          command: "cmp-pkg"
"#,
    )
    .unwrap();
    fs::create_dir_all(root.join("b/.missouri")).unwrap();
    fs::write(
        root.join("b/.missouri/missouri.yml"),
        r#"assertions:
  - name: "the package is on PATH"
    command: "hello-pkg"
    stdout: "hi\n"
  - name: "the package is on PATH without a shell"
    command: "hello-pkg"
    shell: false
    stdout: "hi\n"
"#,
    )
    .unwrap();
    fs::write(root.join("b/out.txt"), "hi\n").unwrap();
}

struct Run {
    passed: bool,
    stdout: String,
    stderr: String,
    nix_calls: Vec<String>,
    comparator_runs: usize,
    pkg_dir: PathBuf,
}

fn run_suite(mode: Nix) -> Run {
    let tmp = tempfile::tempdir().unwrap();
    let fake = tmp.path().join("fake-bin");
    let pkg_dir = tmp.path().join("pkg");
    let pkg_bin = pkg_dir.join("bin");
    let suite = tmp.path().join("suite");
    let log = tmp.path().join("nix-calls.log");
    let comparator_log = tmp.path().join("comparator.log");
    fs::create_dir_all(&fake).unwrap();
    fs::create_dir_all(&pkg_bin).unwrap();
    executable(&pkg_bin.join("hello-pkg"), "#!/bin/sh\necho hi\n");
    executable(
        &pkg_bin.join("cmp-pkg"),
        &format!(
            "#!/bin/sh\necho ran >> '{}'\nexec cmp -s \"$1\" \"$2\"\n",
            comparator_log.display()
        ),
    );
    write_fake_nix(&fake, &log, &pkg_bin, mode);
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
        comparator_runs: fs::read_to_string(&comparator_log)
            .unwrap_or_default()
            .lines()
            .count(),
        // The directory outlives the run only as a name to compare with.
        pkg_dir,
    }
}

/// The verdict lines of a run, with each duration removed: the PASS or
/// FAIL line of each path, and the summary of counts. Both runs write the
/// same colour codes, so the lines keep them.
fn verdict(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter(|line| line.contains("PASS") || line.contains("FAIL") || line.contains(" passed, "))
        .map(|line| match line.rfind(" in ") {
            Some(at) if line.contains(" passed, ") => line[..at].to_string(),
            _ => line
                .rsplit_once(' ')
                .map_or(line, |(head, _)| head)
                .to_string(),
        })
        .collect()
}

fn calls_naming<'a>(run: &'a Run, program: &str) -> Vec<&'a String> {
    run.nix_calls
        .iter()
        .filter(|call| call.starts_with("shell") && call.contains(program))
        .collect()
}

fn assert_fallback(mode: Nix) {
    let good = run_suite(Nix::Good);
    let run = run_suite(mode);
    assert!(
        run.passed,
        "the suite failed\nstdout:\n{}\nstderr:\n{}",
        run.stdout, run.stderr
    );
    let warnings: Vec<&str> = run
        .stderr
        .lines()
        .filter(|line| line.contains("warning"))
        .collect();
    assert_eq!(
        warnings.len(),
        1,
        "expected one warning line: {}",
        run.stderr
    );
    assert!(
        warnings[0].starts_with("warning: warm-up could not read the nix environment")
            && warnings[0].contains("MISSOURI_SANDBOX=preinstalled"),
        "unexpected warning: {}",
        warnings[0]
    );
    assert_eq!(
        calls_naming(&run, "hello-pkg").len(),
        3,
        "expected the transition and both assertions through nix: {:?}",
        run.nix_calls
    );
    assert_eq!(
        calls_naming(&run, "cmp-pkg").len(),
        1,
        "expected the comparator through nix: {:?}",
        run.nix_calls
    );
    assert_eq!(verdict(&run.stdout), verdict(&good.stdout));
}

#[test]
fn a_suite_resolves_its_packages_once_and_runs_no_command_through_nix() {
    let run = run_suite(Nix::Good);
    assert!(
        run.passed,
        "the suite failed\nstdout:\n{}\nstderr:\n{}",
        run.stdout, run.stderr
    );
    for program in ["hello-pkg", "cmp-pkg"] {
        let per_command = calls_naming(&run, program);
        assert!(
            per_command.is_empty(),
            "{program} ran through nix: {per_command:?}"
        );
    }
    assert_eq!(run.comparator_runs, 1, "the comparator did not run");
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
fn warm_up_roots_the_store_path_of_each_package_directory() {
    let run = run_suite(Nix::Good);
    let builds: Vec<Vec<&str>> = run
        .nix_calls
        .iter()
        .filter(|call| call.starts_with("build"))
        .map(|call| call.split_whitespace().collect())
        .collect();
    assert_eq!(builds.len(), 1, "expected one build: {:?}", run.nix_calls);
    let words = &builds[0];
    let store_dir = run.pkg_dir.to_str().unwrap();
    let bin_dir = run.pkg_dir.join("bin");
    assert!(words.contains(&"--out-link"), "no --out-link: {words:?}");
    assert!(
        words.contains(&store_dir),
        "the store path {store_dir} is not rooted: {words:?}"
    );
    assert!(
        !words.contains(&bin_dir.to_str().unwrap()),
        "a bin directory is rooted instead of its store path: {words:?}"
    );
}

#[test]
fn an_unreadable_capture_warns_and_wraps_each_command_with_the_same_verdict() {
    assert_fallback(Nix::UnreadableCapture);
}

#[test]
fn a_capture_that_exits_1_warns_and_wraps_each_command_with_the_same_verdict() {
    assert_fallback(Nix::CaptureExits1);
}

#[test]
fn a_failed_root_build_warns_and_wraps_each_command_with_the_same_verdict() {
    assert_fallback(Nix::BuildFails);
}
