//! The docs are tests.
//!
//! `console_examples` runs every `console` block in README.md and
//! docs/*.md against the built binary with trycmd, and compares the
//! printed output with the block. Each page runs top to bottom in its own
//! scratch directory, with `HOME`, the XDG directories, and `TMPDIR` in
//! that directory, so no page reads or writes anything outside it.
//!
//! Every command a page runs starts through a small `/bin/sh` wrapper that
//! clears the environment (`env -i`) and sets only the variables below.
//! missouri clears the environment of the commands it runs, but not its
//! own, and an inherited `MISSOURI_SANDBOX` or `TMPDIR` changes what it
//! does.
//!
//! missouri prints ANSI colour codes whether or not `NO_COLOR` is set
//! (.tisket/v0.2.6/nc7r-missouri-honours-no-color.md). The
//! `missouri` wrapper removes them, so a page shows the text a reader
//! sees. It passes the exit status through.
//!
//! trycmd writes the working directory as `[CWD]` in output, and the
//! directory that holds it as `[PARENT]`. A duration varies from run to
//! run, so a page writes it as `[..]`, which matches any text within a
//! line.
//!
//! A fence with `{filename="<path>"}` in its info string is a file the
//! reader writes. Its body is written to that path in the page's working
//! directory when the page reaches it, so the commands after it see the
//! file.
//!
//! A page can have fixtures in tests/docs/<page>/: `setup.md` holds
//! `console` blocks (and file fences) that run before the page, for
//! examples that assume earlier state; `work/` is copied into the working
//! directory and `home/` into `HOME`.
//!
//! `help_matches_snapshots` compares the `--help` output of every visible
//! command with tests/snapshots/help/. docs/cli-reference.md is generated
//! from the same definitions by tests/cli_reference.rs.
//!
//! Write the real output into the pages and snapshots, then read the
//! diff: `TRYCMD=overwrite cargo test --test docs` for the examples,
//! `SNAPSHOTS=overwrite cargo test --test docs` for the help snapshots.

use std::path::{Path, PathBuf};
use std::process::Command;

use clap::CommandFactory as _;

const BIN: &str = env!("CARGO_BIN_EXE_missouri");
/// clap wraps help to `COLUMNS` when no terminal is attached, so the
/// width is fixed for every run.
const COLUMNS: &str = "100";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// README.md and every page in docs/, in name order.
fn pages() -> Vec<PathBuf> {
    let mut pages: Vec<PathBuf> = std::fs::read_dir(root().join("docs"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .collect();
    pages.sort();
    pages.insert(0, root().join("README.md"));
    pages
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn scratch() -> (tempfile::TempDir, PathBuf) {
    let guard = tempfile::tempdir().unwrap();
    // Canonical, so a path missouri prints resolved matches the path
    // trycmd writes as `[CWD]`.
    let path = guard.path().canonicalize().unwrap();
    (guard, path)
}

#[test]
fn console_examples() {
    let (_guard, scratch) = scratch();
    run_pages(&scratch, &pages());
}

/// Run every page, then fail naming each page whose examples differ.
fn run_pages(scratch: &Path, pages: &[PathBuf]) {
    let mut failed = Vec::new();
    for page in pages {
        if std::panic::catch_unwind(|| run_page(scratch, page)).is_err() {
            let name = page.strip_prefix(root()).unwrap_or(page);
            failed.push(name.display().to_string());
        }
    }
    assert!(
        failed.is_empty(),
        "examples differ from the binary in: {failed:?}; \
         `TRYCMD=overwrite cargo test --test docs` writes the real output, then read the diff"
    );
}

/// Run one page's `console` blocks in `scratch/<stem>/`.
fn run_page(scratch: &Path, page: &Path) {
    let stem = page.file_stem().unwrap().to_string_lossy().into_owned();
    let fixtures = root().join("tests/docs").join(&stem);
    let dir = scratch.join(&stem);
    // trycmd runs `page.md` in `page.in/`.
    let work = dir.join("page.in");
    let home = dir.join("home");
    let tmp = dir.join("tmp");
    let bin = dir.join("bin");
    for (from, to) in [
        (fixtures.join("work"), &work),
        (fixtures.join("home"), &home),
    ] {
        if from.is_dir() {
            copy_dir(&from, to);
        } else {
            std::fs::create_dir_all(to).unwrap();
        }
    }
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::create_dir_all(&bin).unwrap();
    let setup = std::fs::read_to_string(fixtures.join("setup.md")).unwrap_or_default();
    let body = std::fs::read_to_string(page).unwrap();
    let text = format!("{setup}{body}");

    // trycmd skips, and does not fail, a command it cannot resolve, and it
    // resolves an unregistered crate binary to the binary itself. So every
    // command a page runs is registered here, through a wrapper, from the
    // blocks trycmd runs, or the page fails.
    let mut registered = Vec::new();
    for name in commands(page, &text) {
        let wrapper = if name == "missouri" {
            write_missouri_wrapper(&bin)
        } else {
            let found = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                .map(|dir| dir.join(&name))
                .find(|p| p.is_file());
            let path = found.unwrap_or_else(|| {
                panic!("{} runs `{name}`, which is not on PATH", page.display())
            });
            write_wrapper(&bin, &name, &path)
        };
        registered.push((name, wrapper));
    }

    let case = dir.join("page.md");
    let overwrite = std::env::var_os("TRYCMD").is_some_and(|m| m == "overwrite");
    let mut written = String::new();
    for (index, segment) in segments(page, &text).into_iter().enumerate() {
        if segment.text.lines().any(|l| l.starts_with("$ ")) {
            std::fs::write(&case, &segment.text).unwrap();
            let cases = trycmd::TestCases::new();
            for (name, wrapper) in &registered {
                cases.register_bin(name.clone(), wrapper);
            }
            let home_var = |sub: &str| home.join(sub).display().to_string();
            // A path beside the working directory, such as a kept temp
            // directory under `TMPDIR`, prints as an absolute path.
            // `[PARENT]` stands for the directory that holds the working
            // directory, as `[CWD]` stands for the working directory; the
            // longer value is replaced first.
            cases
                .insert_var("[PARENT]", dir.display().to_string())
                .unwrap();
            cases
                .env("HOME", home.display().to_string())
                .env("XDG_CONFIG_HOME", home_var(".config"))
                .env("XDG_STATE_HOME", home_var(".local/state"))
                .env("XDG_DATA_HOME", home_var(".local/share"))
                .env("XDG_CACHE_HOME", home_var(".cache"))
                .env("TMPDIR", tmp.display().to_string())
                .case(&case)
                .run();
            if overwrite {
                written.push_str(&std::fs::read_to_string(&case).unwrap());
            } else {
                written.push_str(&segment.text);
            }
        } else {
            written.push_str(&segment.text);
        }
        if let Some((name, content)) = segment.file {
            assert!(
                !name.starts_with('/') && !name.split('/').any(|p| p == ".."),
                "{}: segment {index} writes {name}, outside the working directory",
                page.display()
            );
            let target = work.join(&name);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(target, content).unwrap();
        }
    }

    // TRYCMD=overwrite rewrote the scratch copies. Carry the page's part
    // back to the page; the setup part stays in tests/docs.
    if overwrite {
        let body = written.strip_prefix(&setup).unwrap_or_else(|| {
            panic!(
                "the setup for {} changed; run it again after updating setup.md",
                page.display()
            )
        });
        std::fs::write(page, body).unwrap();
    }
}

/// A run of a page that trycmd runs as one case, and the file the reader
/// writes at its end, if any.
struct Segment {
    text: String,
    file: Option<(String, String)>,
}

/// The value of `filename="..."` in a fence's info string.
fn filename(info: &str) -> Option<String> {
    let start = info.find("filename=\"")? + "filename=\"".len();
    let end = info[start..].find('"')?;
    Some(info[start..start + end].to_string())
}

/// Split a page after each fence that names a file, so the file can be
/// written before the commands that follow it run. Fences are read by the
/// same rules as `run_lines`.
fn segments(page: &Path, text: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut current = String::new();
    // The open fence: its backticks, the file it names, and its body.
    let mut fence: Option<(String, Option<String>, String)> = None;
    for line in text.split_inclusive('\n') {
        current.push_str(line);
        let bare = line.trim_end_matches(['\n', '\r']);
        if let Some((marker, file, body)) = fence.as_mut() {
            if bare.starts_with(marker.as_str()) {
                let file = file.take();
                let content = std::mem::take(body);
                fence = None;
                if let Some(name) = file {
                    segments.push(Segment {
                        text: std::mem::take(&mut current),
                        file: Some((name, content)),
                    });
                }
            } else {
                body.push_str(line);
            }
            continue;
        }
        let ticks = bare.len() - bare.trim_start_matches('`').len();
        if ticks < 3 {
            continue;
        }
        let info = &bare[ticks..];
        let file = filename(info);
        assert!(
            file.is_none() || !info.trim_start().starts_with('{'),
            "{}: a file fence needs a language before {{filename=...}}",
            page.display()
        );
        fence = Some((bare[..ticks].to_string(), file, String::new()));
    }
    segments.push(Segment {
        text: current,
        file: None,
    });
    segments
}

/// The lines of every block trycmd runs, read by trycmd 1.2's own rules
/// (`parse_trycmd` in its schema.rs): a fence with no info string, or
/// one whose language is `console` or `trycmd`, runs; the block ends at
/// the first line that starts with the opening backticks.
///
/// Panics on a `console,ignore` or `trycmd,ignore` block: trycmd skips
/// it, so its commands would never be checked.
fn run_lines<'a>(page: &Path, text: &'a str) -> Vec<&'a str> {
    let mut lines = Vec::new();
    // The open fence: its backticks, and whether trycmd runs its body.
    let mut fence: Option<(&str, bool)> = None;
    for line in text.lines() {
        if let Some((marker, runs)) = fence {
            if line.starts_with(marker) {
                fence = None;
            } else if runs {
                lines.push(line);
            }
            continue;
        }
        let ticks = line.len() - line.trim_start_matches('`').len();
        if ticks < 3 {
            continue;
        }
        let info = line[ticks..].trim();
        let mut parts = info.split(',');
        let lang = parts.next().unwrap_or_default();
        let runs = info.is_empty() || lang == "console" || lang == "trycmd";
        assert!(
            !(runs && parts.any(|p| p == "ignore")),
            "{}: `{info}` hides its commands from the docs test",
            page.display()
        );
        fence = Some((&line[..ticks], runs));
    }
    lines
}

/// The command word of every `$ ` line in the blocks trycmd runs.
///
/// Panics on an output line holding `\`: trycmd reads `\` in real output
/// as `/`, so such a line can never match the binary.
fn commands(page: &Path, text: &str) -> std::collections::BTreeSet<String> {
    let mut names = std::collections::BTreeSet::new();
    for line in run_lines(page, text) {
        if let Some(cmd) = line.strip_prefix("$ ") {
            if let Some(word) = cmd.split_whitespace().find(|w| !w.contains('=')) {
                names.insert(word.to_string());
            }
        } else if !line.starts_with("> ") {
            // `> ` continues a command; every other line is output.
            assert!(
                !line.contains('\\'),
                "{}: trycmd cannot match output holding a backslash; choose an example without one: {line}",
                page.display()
            );
        }
    }
    names
}

/// The variables every page command sees, and nothing else. `$HOME`, the
/// XDG directories, and `$TMPDIR` are the page's, set by the test on the
/// wrapper.
fn clean_env() -> String {
    format!(
        "env -i HOME=\"$HOME\" XDG_CONFIG_HOME=\"$XDG_CONFIG_HOME\" \
         XDG_STATE_HOME=\"$XDG_STATE_HOME\" XDG_DATA_HOME=\"$XDG_DATA_HOME\" \
         XDG_CACHE_HOME=\"$XDG_CACHE_HOME\" TMPDIR=\"$TMPDIR\" PATH=\"$PATH\" \
         NO_COLOR=1 TERM=dumb COLUMNS={COLUMNS}"
    )
}

fn quoted(path: &Path) -> String {
    // The wrappers quote paths in single quotes.
    assert!(
        !path.to_string_lossy().contains('\''),
        "cannot wrap a path holding a single quote: {}",
        path.display()
    );
    format!("'{}'", path.display())
}

fn write_script(path: &Path, script: &str) -> PathBuf {
    std::fs::write(path, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    path.to_path_buf()
}

/// `name` in the examples: `target` with the clean environment.
fn write_wrapper(bin: &Path, name: &str, target: &Path) -> PathBuf {
    write_script(
        &bin.join(name),
        &format!(
            "#!/bin/sh\nexec {} {} \"$@\"\n",
            clean_env(),
            quoted(target)
        ),
    )
}

/// `missouri` in the examples: the built binary with the clean
/// environment, its colour codes removed, and its exit status passed
/// through. trycmd joins stderr to stdout for a Markdown case, so the
/// wrapper joining them first changes nothing a page shows.
fn write_missouri_wrapper(bin: &Path) -> PathBuf {
    write_script(
        &bin.join("missouri"),
        &format!(
            "#!/bin/sh\n\
             status=$(mktemp) || exit 125\n\
             {{ {} {} \"$@\" 2>&1; echo $? >\"$status\"; }} | \
             LC_ALL=C sed \"s/$(printf '\\033')\\[[0-9;]*m//g\"\n\
             code=$(cat \"$status\")\n\
             rm -f \"$status\"\n\
             exit \"$code\"\n",
            clean_env(),
            quoted(Path::new(BIN))
        ),
    )
}

/// Every visible command path, depth first: `[]`, `["run"]`, ...
fn command_paths(cmd: &clap::Command, prefix: &[String], out: &mut Vec<Vec<String>>) {
    out.push(prefix.to_vec());
    for sub in cmd.get_subcommands() {
        if sub.is_hide_set() || sub.get_name() == "help" {
            continue;
        }
        let mut path = prefix.to_vec();
        path.push(sub.get_name().to_string());
        command_paths(sub, &path, out);
    }
}

fn visible_command_paths() -> Vec<Vec<String>> {
    let mut paths = Vec::new();
    command_paths(&missouri::cli::Args::command(), &[], &mut paths);
    paths
}

fn help(path: &[String]) -> String {
    let out = Command::new(BIN)
        .args(path)
        .arg("--help")
        .env_clear()
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .env("COLUMNS", COLUMNS)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{path:?} --help failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn snapshot_dir() -> PathBuf {
    root().join("tests/snapshots/help")
}

/// The snapshot file of one command path: `missouri-state-add.txt`.
fn snapshot_name(path: &[String]) -> String {
    let mut name = String::from("missouri");
    for part in path {
        name.push('-');
        name.push_str(part);
    }
    name + ".txt"
}

/// Every visible command's `--help` output matches its snapshot.
#[test]
fn help_matches_snapshots() {
    let dir = snapshot_dir();
    std::fs::create_dir_all(&dir).unwrap();
    for path in visible_command_paths() {
        let file = dir.join(snapshot_name(&path));
        snapbox::Assert::new()
            .action_env("SNAPSHOTS")
            .eq(help(&path), snapbox::Data::read_from(&file, None).raw());
    }
}

/// Every snapshot names a visible command, so a removed or renamed
/// command leaves no stale snapshot behind.
#[test]
fn every_help_snapshot_names_a_visible_command() {
    let wanted: std::collections::BTreeSet<String> = visible_command_paths()
        .iter()
        .map(|p| snapshot_name(p))
        .collect();
    let mut have = std::collections::BTreeSet::new();
    for entry in std::fs::read_dir(snapshot_dir()).unwrap() {
        have.insert(entry.unwrap().file_name().to_string_lossy().into_owned());
    }
    assert_eq!(have, wanted);
}

/// One wrong page among right ones fails the run, and the failure names
/// that page alone.
#[test]
fn a_wrong_page_fails_the_run_and_is_named() {
    if std::env::var_os("TRYCMD").is_some() {
        return;
    }
    let (_guard, scratch) = scratch();
    let right = scratch.join("right.md");
    std::fs::write(
        &right,
        "```console\n$ missouri init\ninitialized missouri project at [CWD]/.missouri\n\n```\n",
    )
    .unwrap();
    let wrong = scratch.join("wrong.md");
    std::fs::write(&wrong, "```console\n$ missouri init\nInitialized\n\n```\n").unwrap();
    let pages = [right, wrong.clone()];
    let err = std::panic::catch_unwind(|| run_pages(&scratch, &pages)).unwrap_err();
    let message = err.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(
        message.contains(&format!("[{:?}]", wrong.display().to_string())),
        "{message}"
    );
}

/// The harness itself: a page whose output differs from the binary fails,
/// and a page that matches passes.
#[test]
fn a_wrong_example_fails_and_a_right_one_passes() {
    if std::env::var_os("TRYCMD").is_some() {
        return; // overwrite mode would rewrite the wrong page and pass
    }
    let (_guard, scratch) = scratch();
    let right = scratch.join("right.md");
    std::fs::write(
        &right,
        "```console\n\
         $ missouri init\n\
         initialized missouri project at [CWD]/.missouri\n\
         \n\
         $ missouri state add clean\n\
         created state \"clean\"\n\
         \n\
         $ missouri validate\n\
         valid: 1 state(s), 0 transition(s), 1 root(s)\n\
         \n\
         ```\n",
    )
    .unwrap();
    run_page(&scratch, &right);
    let wrong = scratch.join("wrong.md");
    std::fs::write(&wrong, "```console\n$ missouri init\nInitialized\n\n```\n").unwrap();
    assert!(std::panic::catch_unwind(|| run_page(&scratch, &wrong)).is_err());
}

/// A page sees missouri's output without colour codes, as a reader with
/// colour off does, and a failing run keeps its exit status: the page
/// passes with `? 1` and fails without it.
#[test]
fn a_run_shows_plain_text_and_keeps_its_exit_status() {
    if std::env::var_os("TRYCMD").is_some() {
        return;
    }
    let (_guard, scratch) = scratch();
    let setup = "```console\n\
         $ missouri init\n\
         initialized missouri project at [CWD]/.missouri\n\
         \n\
         $ missouri state add clean\n\
         created state \"clean\"\n\
         \n\
         $ missouri state add built\n\
         created state \"built\"\n\
         \n\
         ```\n\
         \n\
         ```yaml {filename=\"clean/.missouri/missouri.yml\"}\n\
         transitions:\n\
         \x20 - name: make\n\
         \x20   command: \"echo hello > out.txt\"\n\
         \x20   target: ../built\n\
         ```\n\
         \n\
         ```text {filename=\"built/out.txt\"}\n\
         goodbye\n\
         ```\n\
         \n";
    let run = "```console\n\
         $ missouri run\n\
         ? 1\n\
         ✗ [1/1]\n\
         FAIL clean → built [..]\n\
         \x20 ✗ clean → built (make) [..]\n\
         \x20   differs: out.txt\n\
         \x20     expected:\n\
         \x20     goodbye\n\
         \x20     \n\
         \x20     actual:\n\
         \x20     hello\n\
         \n\
         0 passed, 1 failed, 1 step in [..]\n\
         \n\
         ```\n";
    let marked = scratch.join("marked.md");
    std::fs::write(&marked, format!("{setup}{run}")).unwrap();
    run_page(&scratch, &marked);
    let unmarked = scratch.join("unmarked.md");
    std::fs::write(&unmarked, format!("{setup}{}", run.replace("? 1\n", ""))).unwrap();
    assert!(std::panic::catch_unwind(|| run_page(&scratch, &unmarked)).is_err());
}

/// The wrapper removes colour codes itself: run directly, its output
/// holds no escape byte, though missouri prints one.
#[test]
fn the_missouri_wrapper_removes_colour_codes() {
    let (_guard, scratch) = scratch();
    let wrapper = write_missouri_wrapper(&scratch);
    let project = scratch.join("project");
    std::fs::create_dir_all(&project).unwrap();
    let run = |cmd: &Path, args: &[&str]| {
        Command::new(cmd)
            .args(args)
            .current_dir(&project)
            .env("TMPDIR", &scratch)
            .output()
            .unwrap()
    };
    for args in [
        &["init"][..],
        &["state", "add", "start"][..],
        &["state", "add", "end"][..],
    ] {
        assert!(run(Path::new(BIN), args).status.success());
    }
    std::fs::write(
        project.join("start/.missouri/missouri.yml"),
        "transitions:\n  - command: \"true\"\n    target: ../end\n",
    )
    .unwrap();
    let raw = run(Path::new(BIN), &["run"]);
    let raw =
        String::from_utf8_lossy(&raw.stdout).into_owned() + &String::from_utf8_lossy(&raw.stderr);
    assert!(raw.contains('\x1b'), "missouri printed no colour: {raw}");
    let wrapped = run(&wrapper, &["run"]);
    assert!(wrapped.status.success());
    let wrapped = String::from_utf8(wrapped.stdout).unwrap();
    assert!(!wrapped.contains('\x1b'), "{wrapped}");
    assert!(wrapped.contains("PASS start → end"), "{wrapped}");
    // The status is the binary's, not the filter's: a failing run fails.
    std::fs::write(project.join("start/stray.txt"), "x\n").unwrap();
    assert_eq!(run(&wrapper, &["run"]).status.code(), Some(1));
}

/// A file fence is on disk for the commands after it, and a wrong output
/// after it fails the page.
#[test]
fn a_file_fence_lands_before_the_commands_after_it() {
    if std::env::var_os("TRYCMD").is_some() {
        return;
    }
    let (_guard, scratch) = scratch();
    let right = scratch.join("after.md");
    std::fs::write(
        &right,
        "```text {filename=\"a/b.txt\"}\nhello\n```\n\n```console\n$ cat a/b.txt\nhello\n\n```\n",
    )
    .unwrap();
    run_page(&scratch, &right);
    let wrong = scratch.join("after-wrong.md");
    std::fs::write(
        &wrong,
        "```text {filename=\"a.txt\"}\nhello\n```\n\n```console\n$ cat a.txt\ngoodbye\n\n```\n",
    )
    .unwrap();
    assert!(std::panic::catch_unwind(|| run_page(&scratch, &wrong)).is_err());
}

/// A path beside the working directory matches `[PARENT]`, and the
/// working directory still matches `[CWD]`.
#[test]
fn a_path_beside_the_working_directory_is_parent() {
    if std::env::var_os("TRYCMD").is_some() {
        return;
    }
    let (_guard, scratch) = scratch();
    let page = scratch.join("beside.md");
    std::fs::write(
        &page,
        "```console\n\
         $ printenv TMPDIR\n\
         [PARENT]/tmp\n\
         \n\
         $ pwd\n\
         [CWD]\n\
         \n\
         ```\n",
    )
    .unwrap();
    run_page(&scratch, &page);
}

/// A page's commands see the page's `HOME`, XDG directories, and
/// `TMPDIR`, never the test process's, and the fixtures in `home/` and
/// `work/` are copied.
#[test]
fn a_page_sees_its_own_home_and_fixtures() {
    if std::env::var_os("TRYCMD").is_some() {
        return;
    }
    let (_guard, scratch) = scratch();
    let page = scratch.join("page-env-fixture.md");
    std::fs::write(
        &page,
        "```console\n\
         $ printenv HOME\n\
         [PARENT]/home\n\
         \n\
         $ printenv XDG_CONFIG_HOME\n\
         [PARENT]/home/.config\n\
         \n\
         $ printenv XDG_STATE_HOME\n\
         [PARENT]/home/.local/state\n\
         \n\
         $ printenv XDG_DATA_HOME\n\
         [PARENT]/home/.local/share\n\
         \n\
         $ printenv XDG_CACHE_HOME\n\
         [PARENT]/home/.cache\n\
         \n\
         $ cat note.txt\n\
         from the work fixture\n\
         \n\
         $ cat ../home/note.txt\n\
         from the home fixture\n\
         \n\
         ```\n",
    )
    .unwrap();
    run_page(&scratch, &page);
}

/// A page's commands see none of the test process's environment.
#[test]
fn page_commands_run_in_a_clean_environment() {
    let (_guard, scratch) = scratch();
    let printenv = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|dir| dir.join("printenv"))
        .find(|p| p.is_file())
        .expect("printenv on PATH");
    let wrapper = write_wrapper(&scratch, "printenv", &printenv);
    let out = Command::new(&wrapper)
        .env("MISSOURI_SANDBOX", "preinstalled")
        .env("GIT_DIR", "/elsewhere")
        .env("HOME", "/page-home")
        .output()
        .unwrap();
    let env = String::from_utf8(out.stdout).unwrap();
    assert!(env.contains("HOME=/page-home\n"), "{env}");
    assert!(env.contains("NO_COLOR=1\n"), "{env}");
    assert!(!env.contains("MISSOURI_SANDBOX"), "{env}");
    assert!(!env.contains("GIT_DIR"), "{env}");
}

/// The README's tree names the transition that tests/docs/README/setup.md
/// writes for the README's `missouri run`, so the run the page shows is
/// the run of the suite the page draws.
#[test]
fn the_readme_tree_shows_the_suite_its_run_uses() {
    let setup = std::fs::read_to_string(root().join("tests/docs/README/setup.md")).unwrap();
    let open = "```yaml {filename=\"clean/.missouri/missouri.yml\"}\n";
    let start = setup.find(open).expect("setup writes clean's config") + open.len();
    let config = setup[start..].lines().next().unwrap();
    let readme = std::fs::read_to_string(root().join("README.md")).unwrap();
    assert!(
        readme.contains(&format!("missouri.yml        # {config}\n")),
        "the README tree does not show `{config}`"
    );
    assert!(setup.contains("```text {filename=\"built/out.txt\"}\nhello\n```"));
    assert!(readme.contains("out.txt               # file that should exist: contains \"hello\""));
}

/// Every page the docs test runs: README.md, and each Markdown file in
/// docs/, counted here by name rather than by the filter `pages` uses.
#[test]
fn pages_cover_readme_and_every_docs_page() {
    let mut expected: Vec<PathBuf> = std::fs::read_dir(root().join("docs"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".md"))
        .collect();
    expected.sort();
    assert!(expected.len() >= 5, "{expected:?}");
    expected.insert(0, root().join("README.md"));
    assert_eq!(pages(), expected);
}

/// A `> ` line continues a command, so it may hold a backslash; an
/// output line may not.
#[test]
fn a_continuation_line_may_hold_a_backslash() {
    let names: Vec<String> = commands(
        Path::new("p.md"),
        "```console\n$ missouri list\n> --sh\\ow paths\nok\n```\n",
    )
    .into_iter()
    .collect();
    assert_eq!(names, ["missouri"]);
}

#[test]
fn commands_lists_what_trycmd_runs() {
    let page = "```sh\nls\n```\n\n````markdown\n```console\n$ inner\n```\n````\n\n```console\n$ A=1 missouri run\n\n$ cat out.txt\nok\n```\n\n```\n$ bare\n```\n\n```trycmd\n$ tagged\n```\n";
    let names: Vec<String> = commands(Path::new("p.md"), page).into_iter().collect();
    assert_eq!(names, ["bare", "cat", "missouri", "tagged"]);
}

/// A fence closes only at its own backticks, so a shorter fence inside a
/// longer console block does not end it.
#[test]
fn a_console_block_ends_at_its_own_backticks() {
    let page = "````console\n$ first\n```\n$ second\n````\n";
    let names: Vec<String> = commands(Path::new("p.md"), page).into_iter().collect();
    assert_eq!(names, ["first", "second"]);
}

#[test]
fn a_page_running_an_unknown_command_fails() {
    let (_guard, scratch) = scratch();
    let page = scratch.join("unknown.md");
    std::fs::write(&page, "```console\n$ no-such-command-in-path\n```\n").unwrap();
    let err = std::panic::catch_unwind(|| run_page(&scratch, &page)).unwrap_err();
    let message = err.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(message.contains("not on PATH"), "{message}");
}

#[test]
#[should_panic(expected = "hides its commands")]
fn an_ignored_console_block_is_refused() {
    commands(
        Path::new("p.md"),
        "```console,ignore\n$ missouri run\n```\n",
    );
}

#[test]
#[should_panic(expected = "backslash")]
fn output_with_a_backslash_is_refused() {
    commands(
        Path::new("p.md"),
        "```console\n$ missouri run\n      stdout expected: \"goodbye\\n\"\n```\n",
    );
}

#[test]
#[should_panic(expected = "outside the working directory")]
fn a_file_fence_outside_the_working_directory_is_refused() {
    let (_guard, scratch) = scratch();
    let page = scratch.join("escape.md");
    std::fs::write(&page, "```text {filename=\"../x\"}\nx\n```\n").unwrap();
    run_page(&scratch, &page);
}

#[test]
#[should_panic(expected = "outside the working directory")]
fn a_file_fence_with_an_absolute_path_is_refused() {
    let (_guard, scratch) = scratch();
    let page = scratch.join("absolute.md");
    let target = scratch.join("escaped.txt");
    std::fs::write(
        &page,
        format!("```text {{filename=\"{}\"}}\nx\n```\n", target.display()),
    )
    .unwrap();
    run_page(&scratch, &page);
}

#[test]
#[should_panic(expected = "needs a language")]
fn a_file_fence_without_a_language_is_refused() {
    segments(Path::new("p.md"), "```{filename=\"a.txt\"}\nx\n```\n");
}
