use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;

fn missouri() -> Command {
    assert_cmd::cargo_bin_cmd!("missouri")
}

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn tmpdir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

/// Copy a fixture to a temp directory (for tests that write into the fixture).
fn copy_fixture_to_tmp(fixture_name: &str) -> tempfile::TempDir {
    let tmp = tmpdir();
    let fixture_path = fixture(fixture_name);
    let src = Path::new(&fixture_path);
    copy_dir_recursive(src, tmp.path());
    tmp
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir_recursive(&src_path, &dst_path);
        } else {
            fs::copy(&src_path, &dst_path).unwrap();
        }
    }
}

// --- -C flag (change directory) ---

#[test]
fn dash_c_changes_directory() {
    missouri()
        .arg("-C")
        .arg(fixture("01-trivial"))
        .arg("run")
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"));
}

// --- 01: Trivial (two states, one transition, no comparators) ---

#[test]
fn trivial_run_passes() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("01-trivial"))
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"));
}

#[test]
fn trivial_list_states() {
    missouri()
        .arg("list")
        .arg("-d")
        .arg(fixture("01-trivial"))
        .arg("--show")
        .arg("states")
        .assert()
        .success()
        .stdout(predicate::str::contains("state-a"))
        .stdout(predicate::str::contains("state-b"));
}

#[test]
fn trivial_list_paths() {
    missouri()
        .arg("list")
        .arg("-d")
        .arg(fixture("01-trivial"))
        .arg("--show")
        .arg("paths")
        .assert()
        .success()
        .stdout(predicate::str::contains("state-a"))
        .stdout(predicate::str::contains("state-b"));
}

#[test]
fn trivial_validate() {
    missouri()
        .arg("validate")
        .arg("-d")
        .arg(fixture("01-trivial"))
        .assert()
        .success();
}

// --- 02: Linear chain (A → B → C) ---

#[test]
fn linear_run_passes() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("02-linear"))
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"));
}

#[test]
fn linear_discovers_single_path() {
    // A → B → C is one path with two transitions
    missouri()
        .arg("list")
        .arg("-d")
        .arg(fixture("02-linear"))
        .arg("--show")
        .arg("paths")
        .assert()
        .success()
        .stdout(predicate::str::contains("state-a"))
        .stdout(predicate::str::contains("state-c"));
}

// --- 03: Branching (root → left, root → right) ---

#[test]
fn branching_run_passes() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("03-branching"))
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"));
}

#[test]
fn branching_discovers_two_paths() {
    // root → left and root → right
    missouri()
        .arg("list")
        .arg("-d")
        .arg(fixture("03-branching"))
        .arg("--show")
        .arg("paths")
        .assert()
        .success()
        .stdout(predicate::str::contains("left"))
        .stdout(predicate::str::contains("right"));
}

// --- 04: Custom comparator (JSON semantic equality) ---

#[test]
fn custom_comparator_run_passes() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("04-custom-comparator"))
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"));
}

// --- 05: Environment variables ---

#[test]
fn env_vars_run_passes() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("05-env-vars"))
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"));
}

// --- 06: Ignore pattern ---

#[test]
fn ignore_pattern_run_passes() {
    // timestamp.txt differs but is ignored, so test should pass
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("06-ignore"))
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"));
}

// --- 07: Cycle (A → B → A, no roots, should error or handle gracefully) ---

#[test]
fn cycle_no_roots_errors() {
    // Both states have inbound transitions, so there are no entry points.
    // missouri should report this as an error.
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("07-cycle"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("no entry points"));
}

#[test]
fn cycle_validate_reports_no_roots() {
    missouri()
        .arg("validate")
        .arg("-d")
        .arg(fixture("07-cycle"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("no entry points"));
}

// --- An empty directory is a different fault from a cycle ---
//
// Both end with no root. "No entry points" sends the reader after an
// inbound transition that does not exist, because there is no state to
// carry one. These tests pin each message and the absence of the other,
// so neutralizing the states check turns them red rather than leaving
// the suite green.
//
// The empty case gets one message for every project layout. An earlier
// attempt split it in two and chose by testing for <dir>/<config_dir>,
// which called an ordinary workspace member a directory with no project.

/// A directory holding a project and no state.
fn initialized_but_stateless() -> tempfile::TempDir {
    let tmp = tmpdir();
    missouri()
        .arg("init")
        .arg("-d")
        .arg(tmp.path())
        .assert()
        .success();
    tmp
}

#[test]
fn a_directory_with_no_state_is_refused_for_that() {
    let tmp = tmpdir();
    missouri()
        .arg("run")
        .arg("-d")
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("no states found"))
        .stderr(predicate::str::contains("no entry points").not());
}

#[test]
fn the_empty_refusal_names_a_command_that_works_in_every_layout() {
    // One message serves four layouts: no project, a project made by
    // `init`, a project declared at <root>/missouri.yml, and a
    // workspace member that declares no config of its own. An earlier
    // attempt split the message and chose by testing for
    // <dir>/<config_dir>, which called an ordinary member a directory
    // with no project and sent it to `init`, writing a file nothing
    // reads. The advice must hold without knowing the layout.
    let tmp = initialized_but_stateless();
    let out = missouri()
        .arg("run")
        .arg("-d")
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("no states found"));
    let raw = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    let text = raw.split_whitespace().collect::<Vec<_>>().join(" ");

    // The order carries the whole finding, so the order is what is
    // pinned. The old text named `init` first and `state add` second,
    // and it contains the `state add` substring too, so a bare
    // `contains` passes on both and tests nothing.
    let add = text
        .find("missouri state add")
        .unwrap_or_else(|| panic!("the refusal names no way forward: {raw}"));
    if let Some(init) = text.find("missouri init") {
        assert!(
            add < init,
            "the refusal names `init` before `state add`, and `init` refuses here: {raw}"
        );
    }

    // The first command the refusal names must work in this layout.
    missouri()
        .arg("state")
        .arg("add")
        .arg("first")
        .arg("-d")
        .arg(tmp.path())
        .assert()
        .success();

    // And the second must be the one that does not, which is why the
    // order matters rather than the presence of either name.
    missouri()
        .arg("init")
        .arg("-d")
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("already initialized"));
}

#[test]
fn a_root_config_project_is_not_called_a_directory_with_no_project() {
    // A project declares itself at <root>/missouri.yml as well as at
    // <root>/.missouri/missouri.yml. `graph::load_project_config` reads
    // both. A refusal that tested only the second told this layout to
    // run `init`, which writes a config that layout never reads.
    let tmp = tmpdir();
    fs::create_dir_all(tmp.path().join("cases")).unwrap();
    fs::write(tmp.path().join("missouri.yml"), "test_dir: cases\n").unwrap();
    let out = missouri()
        .arg("list")
        .arg("-d")
        .arg(tmp.path())
        .assert()
        .failure();
    let raw = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    let text = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        text.contains("missouri state add"),
        "a root-config project was not told how to add a state: {raw}"
    );
}

#[test]
fn a_cycle_is_not_reported_as_an_empty_directory() {
    // Assert the message this case wants as well as the absence of the
    // other one. Absence alone passes when the run fails for an
    // unrelated reason.
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("07-cycle"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("no entry points"))
        .stderr(predicate::str::contains("no states found").not());
}

#[test]
fn validate_refuses_a_directory_with_no_state() {
    let tmp = tmpdir();
    missouri()
        .arg("validate")
        .arg("-d")
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("no states found"))
        .stderr(predicate::str::contains("no entry points").not());
}

#[test]
fn validate_refuses_a_project_that_declares_no_state() {
    let tmp = initialized_but_stateless();
    missouri()
        .arg("validate")
        .arg("-d")
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("no states found"));
}

// --- `list` separates the three states a caller cannot otherwise tell apart ---

#[test]
fn list_refuses_a_directory_with_no_state() {
    // It printed `0 path(s)` and exited 0, which reads the same as a
    // valid suite with no path. A caller cannot act on that.
    let tmp = tmpdir();
    missouri()
        .arg("list")
        .arg("-d")
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("no states found"));
}

#[test]
fn a_directory_that_does_not_exist_is_named_in_the_refusal() {
    // The bare io error read "No such file or directory (os error 2)"
    // and named neither the path nor the flag that carried it.
    // The fixture path carries no `-d`, so the flag assertion below
    // means something. A path like `/nope-no-such-directory` satisfies
    // it by accident, because `-directory` holds the substring.
    const ABSENT: &str = "/zzz_missouri_absent_place";
    for command in ["run", "list", "validate"] {
        missouri()
            .arg(command)
            .arg("-d")
            .arg(ABSENT)
            .assert()
            .failure()
            .stderr(predicate::str::contains(ABSENT))
            .stderr(predicate::str::contains("-d"));
    }
}

#[test]
fn init_makes_the_directory_its_refusal_recommends() {
    // The refusal above tells a reader to run `missouri init -d <dir>`.
    // A directory check in the shared resolver would refuse that command
    // too, which closes the loop: the tool refuses the one command that
    // recovers, and names no way out.
    let tmp = tmpdir();
    let fresh = tmp.path().join("not-made-yet");
    missouri()
        .arg("init")
        .arg("-d")
        .arg(&fresh)
        .assert()
        .success();
    assert!(
        fresh.join(".missouri").is_dir(),
        "init did not make the project directory it was given"
    );
}

#[test]
fn the_refusal_names_a_command_that_works() {
    // Pin the two halves together. The advice must name `init`, and
    // `init` must accept the directory the advice is about.
    let tmp = tmpdir();
    let missing = tmp.path().join("absent");
    let out = missouri()
        .arg("list")
        .arg("-d")
        .arg(&missing)
        .assert()
        .failure();
    // miette wraps the help line, so a phrase can be split across two
    // lines. Collapse the whitespace before looking for it.
    let raw = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    let text = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        text.contains("missouri init"),
        "the refusal names no recovery: {raw}"
    );
    missouri()
        .arg("init")
        .arg("-d")
        .arg(&missing)
        .assert()
        .success();
}

#[test]
fn a_refused_path_carries_no_dot_segment() {
    // `-d ./nope` once printed `/cwd/./nope`. The same artifact was
    // fixed for init's success line in this branch.
    let tmp = tmpdir();
    missouri()
        .arg("list")
        .arg("-d")
        .arg("./nope-relative")
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("/./").not());
}

#[test]
fn list_refuses_path_enumeration_on_a_graph_with_no_entry_point() {
    missouri()
        .arg("list")
        .arg("-d")
        .arg(fixture("07-cycle"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("no entry points"));
}

#[test]
fn list_still_shows_the_states_of_a_graph_with_no_entry_point() {
    // The state listing is how a reader finds the cycle, so it prints.
    // Assert on the states, not only on the exit code: a `print_states`
    // that emitted nothing would pass a bare success check.
    missouri()
        .arg("list")
        .arg("--show")
        .arg("states")
        .arg("-d")
        .arg(fixture("07-cycle"))
        .assert()
        .success()
        .stdout(predicate::str::contains("state-a"))
        .stdout(predicate::str::contains("state-b"));
}

#[test]
fn list_still_shows_the_transitions_of_a_graph_with_no_entry_point() {
    missouri()
        .arg("list")
        .arg("--show")
        .arg("transitions")
        .arg("-d")
        .arg(fixture("07-cycle"))
        .assert()
        .success()
        .stdout(predicate::str::contains("state-a"))
        .stdout(predicate::str::contains("state-b"));
}

#[test]
fn list_refuses_a_workspace_member_that_holds_no_state() {
    // `list_workspace_members` returned before both guards, so the
    // fault the single-project branch refuses was still reported as
    // `0 path(s)` and exit 0 here. One fault, two answers, chosen by
    // whether the directory happened to be a workspace.
    let tmp = copy_fixture_to_tmp("19-workspace");
    let member = tmp.path().join("sub-a");
    fs::remove_dir_all(&member).unwrap();
    fs::create_dir_all(member.join(".missouri")).unwrap();
    fs::write(member.join(".missouri/missouri.yml"), "").unwrap();

    missouri()
        .arg("list")
        .arg("-d")
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("no states found"));
}

#[test]
fn list_refuses_a_workspace_member_with_no_entry_point() {
    // The roots check on the workspace path was killed by no test.
    // Weakened, `list` answers a member holding a cycle with a header,
    // `0 path(s)` and exit 0, which is the fault this branch removes.
    let tmp = copy_fixture_to_tmp("19-workspace");
    let member = tmp.path().join("sub-a");
    fs::remove_dir_all(&member).unwrap();
    copy_dir_recursive(Path::new(&fixture("07-cycle")), &member);

    missouri()
        .arg("list")
        .arg("-d")
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("no entry points"));
}

#[test]
fn a_workspace_member_that_does_not_exist_is_named() {
    // The `-d` path names the directory and the flag. A member that is
    // absent gave the bare io message, naming neither the member nor
    // the key that declared it, so a workspace with twenty members and
    // one typo told nobody which one.
    let tmp = copy_fixture_to_tmp("19-workspace");
    fs::remove_dir_all(tmp.path().join("sub-a")).unwrap();

    for command in ["list", "validate", "run"] {
        let out = missouri()
            .arg(command)
            .arg("-d")
            .arg(tmp.path())
            .assert()
            .failure();
        let raw = String::from_utf8_lossy(&out.get_output().stderr).to_string();
        let text = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            text.contains("sub-a"),
            "`{command}` did not name the absent member: {raw}"
        );
        assert!(
            text.contains("members"),
            "`{command}` did not name the key that declared it: {raw}"
        );
        assert!(
            !text.contains("os error 2"),
            "`{command}` still reports the bare io message: {raw}"
        );
    }
}

#[test]
fn state_add_refuses_a_directory_that_does_not_exist() {
    // Reverting this one call site to the unchecked resolver left the
    // suite green while `state add -d <typo>` reported success and
    // built the whole missing tree somewhere the user did not mean.
    let tmp = tmpdir();
    let missing = tmp.path().join("not-a-project");
    missouri()
        .arg("state")
        .arg("add")
        .arg("first")
        .arg("-d")
        .arg(&missing)
        .assert()
        .failure()
        .stderr(predicate::str::contains("no directory at"));
    assert!(
        !missing.exists(),
        "a refused `state add` created the directory anyway"
    );
}

#[test]
fn every_listing_kind_refuses_a_directory_with_no_state() {
    // `--show paths` is the default, and it is refused by the roots
    // check rather than by the states check. So the states check is
    // only reachable through the other two kinds, and without these
    // two cases neutralizing it leaves the whole suite green.
    for kind in ["states", "transitions", "paths", "graph"] {
        let tmp = initialized_but_stateless();
        missouri()
            .arg("list")
            .arg("--show")
            .arg(kind)
            .arg("-d")
            .arg(tmp.path())
            .assert()
            .failure()
            .stderr(predicate::str::contains("no states found"));
    }
}

// --- 12: Assertions (transition output + state assertions) ---

#[test]
fn assertions_full_mode_passes() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("12-assertions"))
        .arg("-v")
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"))
        .stdout(predicate::str::contains("assert: check original content"))
        .stdout(predicate::str::contains(
            "assert: check transformed content",
        ))
        .stdout(predicate::str::contains("assert: bin script check"));
}

#[test]
fn assertions_check_only_passes() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("12-assertions"))
        .arg("--check-only")
        .arg("-v")
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"))
        .stdout(predicate::str::contains("assert: check original content"))
        .stdout(predicate::str::contains(
            "assert: check transformed content",
        ));
}

#[test]
fn assertions_no_check_passes() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("12-assertions"))
        .arg("--no-check")
        .arg("-v")
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"))
        // No assertions should appear
        .stdout(predicate::str::contains("assert").not());
}

#[test]
fn assertions_flags_conflict() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("12-assertions"))
        .arg("--check-only")
        .arg("--no-check")
        .assert()
        .failure();
}

// --- 12: Project-level shared bin/ ---
// After deduplication, check-data lives in .missouri/bin/ at the root
// and both states find it via project bin on PATH.
#[test]
fn assertions_project_bin_passes() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("12-assertions"))
        .arg("-v")
        .assert()
        .success()
        .stdout(predicate::str::contains("assert: bin script check"));
}

// --- 13: Setup commands ---

#[test]
fn setup_runs_before_paths() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("13-setup"))
        .arg("-v")
        .assert()
        .success()
        .stdout(predicate::str::contains("setup"))
        .stdout(predicate::str::contains("PASS"));
}

#[test]
fn setup_failure_stops_execution() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("14-setup-fail"))
        .assert()
        .failure()
        .stdout(predicate::str::contains("setup"));
}

// --- init command ---

#[test]
fn init_creates_project_structure() {
    let tmp = tmpdir();
    let dir = tmp.path().to_str().unwrap();
    missouri().arg("init").arg("-d").arg(dir).assert().success();

    assert!(tmp.path().join(".missouri").is_dir());
    assert!(tmp.path().join(".missouri/missouri.yml").is_file());
    assert!(tmp.path().join(".missouri/bin").is_dir());
    assert!(tmp.path().join(".missouri/ignore").is_file());
}

#[test]
fn init_with_custom_config_dir() {
    let tmp = tmpdir();
    let dir = tmp.path().to_str().unwrap();
    missouri()
        .arg("--config-dir")
        .arg(".test-config")
        .arg("init")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    assert!(tmp.path().join(".test-config").is_dir());
    assert!(tmp.path().join(".test-config/missouri.yml").is_file());
    assert!(tmp.path().join(".test-config/bin").is_dir());
}

#[test]
fn init_fails_if_already_initialized() {
    let tmp = tmpdir();
    let dir = tmp.path().to_str().unwrap();

    missouri().arg("init").arg("-d").arg(dir).assert().success();

    missouri()
        .arg("init")
        .arg("-d")
        .arg(dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("already initialized"));
}

// --- state add command ---

#[test]
fn state_add_creates_empty_state() {
    let tmp = tmpdir();
    let dir = tmp.path().to_str().unwrap();
    missouri().arg("init").arg("-d").arg(dir).assert().success();

    missouri()
        .arg("state")
        .arg("add")
        .arg("my-state")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    assert!(tmp.path().join("my-state").is_dir());
    assert!(tmp.path().join("my-state/.missouri/missouri.yml").is_file());
}

#[test]
fn state_add_fails_if_state_exists() {
    let tmp = tmpdir();
    let dir = tmp.path().to_str().unwrap();
    missouri().arg("init").arg("-d").arg(dir).assert().success();

    missouri()
        .arg("state")
        .arg("add")
        .arg("my-state")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    missouri()
        .arg("state")
        .arg("add")
        .arg("my-state")
        .arg("-d")
        .arg(dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn state_add_from_copies_state_and_creates_transition() {
    let tmp = tmpdir();
    let dir = tmp.path().to_str().unwrap();
    missouri().arg("init").arg("-d").arg(dir).assert().success();

    missouri()
        .arg("state")
        .arg("add")
        .arg("before")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    // Add a data file to the source state
    fs::write(tmp.path().join("before/data.txt"), "hello\n").unwrap();

    missouri()
        .arg("state")
        .arg("add")
        .arg("after")
        .arg("--from")
        .arg("before")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    // Data file was copied
    assert_eq!(
        fs::read_to_string(tmp.path().join("after/data.txt")).unwrap(),
        "hello\n"
    );
    // Config was copied
    assert!(tmp.path().join("after/.missouri/missouri.yml").is_file());
    // Transition was appended to source
    let yml = fs::read_to_string(tmp.path().join("before/.missouri/missouri.yml")).unwrap();
    assert!(yml.contains("../after"));
    assert!(yml.contains("TODO"));
}

#[test]
fn state_add_from_nonexistent_fails() {
    let tmp = tmpdir();
    let dir = tmp.path().to_str().unwrap();
    missouri().arg("init").arg("-d").arg(dir).assert().success();

    missouri()
        .arg("state")
        .arg("add")
        .arg("after")
        .arg("--from")
        .arg("nonexistent")
        .arg("-d")
        .arg(dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

// --- 15: Fail mid-path ---

#[test]
fn fail_mid_path_run_fails() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("15-fail-mid-path"))
        .assert()
        .failure()
        .stdout(predicate::str::contains("FAIL"))
        .stdout(predicate::str::contains("step one"))
        .stdout(predicate::str::contains("step two fails"));
}

// --- Recording ---

#[test]
fn record_produces_output_directory() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("run")
        .arg("--record")
        .arg("--run-id")
        .arg("clitest")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    assert!(tmp.path().join(".missouri/runs/clitest").is_dir());
    assert!(
        tmp.path()
            .join(".missouri/runs/clitest/results.json")
            .is_file()
    );
    assert!(
        tmp.path()
            .join(".missouri/runs/clitest/path-0/step-0.cast")
            .is_file()
    );
}

#[test]
fn record_cast_files_per_step() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("run")
        .arg("--record")
        .arg("--run-id")
        .arg("clitest")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    let runs_dir = tmp.path().join(".missouri/runs/clitest");
    // 03-branching has 2 paths, each with 1 step = 2 .cast files total
    assert!(runs_dir.join("path-0/step-0.cast").is_file());
    assert!(runs_dir.join("path-1/step-0.cast").is_file());
    // No step-1 in either path
    assert!(!runs_dir.join("path-0/step-1.cast").exists());
    assert!(!runs_dir.join("path-1/step-1.cast").exists());
}

#[test]
fn record_does_not_break_pass_fail() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("run")
        .arg("--record")
        .arg("-d")
        .arg(dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"))
        .stdout(predicate::str::contains("2 passed"));
}

#[test]
fn record_with_failing_fixture() {
    let tmp = copy_fixture_to_tmp("15-fail-mid-path");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("run")
        .arg("--record")
        .arg("--run-id")
        .arg("clitest")
        .arg("-d")
        .arg(dir)
        .assert()
        .failure();

    let runs_dir = tmp.path().join(".missouri/runs/clitest");
    assert!(runs_dir.join("results.json").is_file());
    // Step 0 ran and succeeded
    assert!(runs_dir.join("path-0/step-0.cast").is_file());
    // Step 1 ran and failed (but output was still captured)
    assert!(runs_dir.join("path-0/step-1.cast").is_file());
    // Step 2 never ran
    assert!(!runs_dir.join("path-0/step-2.cast").exists());
}

#[test]
fn record_run_id_flag() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("run")
        .arg("--record")
        .arg("--run-id")
        .arg("my-custom-id")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    assert!(tmp.path().join(".missouri/runs/my-custom-id").is_dir());
}

#[test]
fn record_default_run_id_is_timestamp() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("run")
        .arg("--record")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    let runs_dir = tmp.path().join(".missouri/runs");
    assert!(runs_dir.is_dir());
    let entries: Vec<_> = fs::read_dir(&runs_dir).unwrap().collect();
    assert_eq!(entries.len(), 1);
    let dirname = entries[0].as_ref().unwrap().file_name();
    let dirname = dirname.to_str().unwrap();
    // Should look like 2026-02-22T17-30-00
    assert!(
        dirname.len() >= 19,
        "expected timestamp-like dirname, got: {dirname}"
    );
    assert!(
        dirname.contains('T'),
        "expected timestamp-like dirname, got: {dirname}"
    );
}

#[test]
fn record_conflicts_with_check_only() {
    missouri()
        .arg("run")
        .arg("--record")
        .arg("--check-only")
        .arg("-d")
        .arg(fixture("03-branching"))
        .assert()
        .failure();
}

#[test]
fn record_works_with_no_check() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("run")
        .arg("--record")
        .arg("--no-check")
        .arg("--run-id")
        .arg("clitest")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    assert!(
        tmp.path()
            .join(".missouri/runs/clitest/path-0/step-0.cast")
            .is_file()
    );
}

// --- Reporting ---

#[test]
fn report_terminal_default() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    // Setup: record a run
    missouri()
        .arg("run")
        .arg("--record")
        .arg("--run-id")
        .arg("r1")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    // Report
    missouri()
        .arg("report")
        .arg("-d")
        .arg(dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("passed"))
        .stdout(predicate::str::contains("root"));
}

#[test]
fn report_html_generates_file() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("run")
        .arg("--record")
        .arg("--run-id")
        .arg("r1")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    missouri()
        .arg("report")
        .arg("--format")
        .arg("html")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    // Find the generated HTML file
    let report_path = tmp.path().join(".missouri/runs/r1/report.html");
    assert!(report_path.is_file(), "HTML report should be generated");
    let html = fs::read_to_string(&report_path).unwrap();
    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("<pre><code>"));
    assert!(html.contains("PASS") || html.contains("FAIL"));
}

#[test]
fn report_html_is_self_contained() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("run")
        .arg("--record")
        .arg("--run-id")
        .arg("r1")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    missouri()
        .arg("report")
        .arg("--format")
        .arg("html")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    let html = fs::read_to_string(tmp.path().join(".missouri/runs/r1/report.html")).unwrap();
    assert!(html.contains("<pre><code>"));
    // No external resources
    assert!(!html.contains("<script src="));
    assert!(!html.contains("<link href="));
}

#[test]
fn report_md_generates_file() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("run")
        .arg("--record")
        .arg("--run-id")
        .arg("r1")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    missouri()
        .arg("report")
        .arg("--format")
        .arg("md")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    let report_path = tmp.path().join(".missouri/runs/r1/report.md");
    assert!(report_path.is_file(), "Markdown report should be generated");
    let md = fs::read_to_string(&report_path).unwrap();
    assert!(md.contains("```"));
    assert!(md.contains("root"));
}

#[test]
fn report_specific_run() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("run")
        .arg("--record")
        .arg("--run-id")
        .arg("run-a")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    missouri()
        .arg("run")
        .arg("--record")
        .arg("--run-id")
        .arg("run-b")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    missouri()
        .arg("report")
        .arg("--run")
        .arg("run-a")
        .arg("-d")
        .arg(dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("run-a"));
}

#[test]
fn report_no_runs_errors() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("report")
        .arg("-d")
        .arg(dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("no recorded runs"));
}

// --- Serving ---

#[test]
fn serve_no_runs_errors() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("serve")
        .arg("-d")
        .arg(dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("no recorded runs"));
}

#[test]
fn serve_with_a_recording_refuses_and_exits_nonzero() {
    // serve is a placeholder, and the exit code is its whole contract:
    // a caller that reads 0 takes the placeholder for a server that
    // started. Returning Ok(true) here once left the suite green, so
    // this test exists to fail when it does.
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();
    std::fs::create_dir_all(tmp.path().join(".missouri/runs/20260101-000000"))
        .expect("a recorded run directory");

    missouri()
        .arg("serve")
        .arg("-d")
        .arg(dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("not implemented yet"));
}

// --- 18: Root-level missouri.yml with test_dir ---

#[test]
fn test_dir_run_passes() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("18-test-dir"))
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"));
}

#[test]
fn test_dir_list_states() {
    missouri()
        .arg("list")
        .arg("-d")
        .arg(fixture("18-test-dir"))
        .arg("--show")
        .arg("states")
        .assert()
        .success()
        .stdout(predicate::str::contains("state-a"))
        .stdout(predicate::str::contains("state-b"));
}

#[test]
fn test_dir_list_paths() {
    missouri()
        .arg("list")
        .arg("-d")
        .arg(fixture("18-test-dir"))
        .arg("--show")
        .arg("paths")
        .assert()
        .success()
        .stdout(predicate::str::contains("state-a"))
        .stdout(predicate::str::contains("state-b"));
}

#[test]
fn test_dir_dash_c_works() {
    // -C should work with root-level missouri.yml
    missouri()
        .arg("-C")
        .arg(fixture("18-test-dir"))
        .arg("run")
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"));
}

// --- workspace (members) ---

#[test]
fn workspace_run_passes_all_members() {
    // Workspace mode should print member headers and per-member summaries
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("19-workspace"))
        .assert()
        .success()
        // Member section headers
        .stdout(predicate::str::contains("sub-a"))
        .stdout(predicate::str::contains("sub-b"))
        // Per-member pass lines
        .stdout(predicate::str::contains("PASS"));
}

#[test]
fn workspace_run_has_per_member_summaries() {
    // Each member should produce its own summary line with pass/fail counts
    let output = missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("19-workspace"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Two separate summary lines (one per member)
    let summary_count = stdout.matches("passed").count();
    assert!(
        summary_count >= 2,
        "expected at least 2 summary lines, got {summary_count}: {stdout}"
    );
}

#[test]
fn workspace_run_fails_if_any_member_fails() {
    // When one member has failing tests, the overall result should be failure
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("20-workspace-fail"))
        .assert()
        .failure()
        // The failing member should appear in output
        .stdout(predicate::str::contains("bad-sub"))
        .stdout(predicate::str::contains("FAIL"));
}

#[test]
fn workspace_dash_c_works() {
    // -C should work with workspace missouri.yml
    missouri()
        .arg("-C")
        .arg(fixture("19-workspace"))
        .arg("run")
        .assert()
        .success()
        .stdout(predicate::str::contains("sub-a"))
        .stdout(predicate::str::contains("sub-b"));
}

#[test]
fn workspace_validate_checks_all_members() {
    missouri()
        .arg("validate")
        .arg("-d")
        .arg(fixture("19-workspace"))
        .assert()
        .success()
        // Should report valid state for both members
        .stdout(predicate::str::contains("sub-a"))
        .stdout(predicate::str::contains("sub-b"));
}
