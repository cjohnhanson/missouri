//! `scripts/prepublish.sh`, run against stubs. The script is the last
//! check before a registry that allows a yank and never a reuse, and
//! nothing ran it: a version whose `run` helper exited on the first
//! failing check, before it printed why, passed one commit's review
//! with a message that said the helper was fixed.
//!
//! Every program the script calls resolves through PATH, so a directory
//! of stubs ahead of PATH is the whole seam. `python3`, `sed`, `tail`
//! and `mktemp` stay real.

// The stub bodies are shell text. `{"packages":...}` is JSON and
// `${STUB_LOG:?}` is a shell expansion, not a format argument.
#![allow(clippy::literal_string_with_formatting_args)]

use std::path::{Path, PathBuf};
use std::process::Command;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// A fixture with the script, the toolchain pin, and one stub per
/// program the script calls. Returns the fixture and its stub dir.
fn fixture(name: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("missouri_prepublish_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scripts")).expect("fixture dir");
    std::fs::copy(
        format!("{ROOT}/scripts/prepublish.sh"),
        dir.join("scripts/prepublish.sh"),
    )
    .expect("copy the script");
    std::fs::copy(
        format!("{ROOT}/rust-toolchain.toml"),
        dir.join("rust-toolchain.toml"),
    )
    .expect("copy the toolchain pin");
    let bin = dir.join("stubs");
    std::fs::create_dir_all(&bin).expect("stub dir");
    stub(
        &bin,
        "cargo",
        r#"case "$*" in
  "hack --version") [ -z "${STUB_NO_HACK:-}" ] || exit 1; echo cargo-hack; exit 0 ;;
  "metadata --no-deps --format-version 1")
    printf '{"packages":[{"name":"stubcrate","version":"9.9.9"}]}'; exit 0 ;;
esac
case " $* " in
  *" ${STUB_FAIL:-__none__} "*) echo "stub: $1 exploded"; exit 1 ;;
esac
exit 0
"#,
    );
    // `rustup run <toolchain> <cmd...>`: record the toolchain, run the
    // command through PATH, which is the cargo stub.
    stub(
        &bin,
        "rustup",
        r#"[ "$1" = run ] || exit 0
printf '%s\n' "$2" >> "${STUB_LOG:?}"
shift 2
exec "$@"
"#,
    );
    stub(
        &bin,
        "curl",
        r#"[ -z "${STUB_CURL_DIES:-}" ] || exit 7
printf '%s' "${STUB_HTTP_CODE:-404}"
"#,
    );
    stub(&bin, "git", "exit 0\n");
    stub(&bin, "tar", "exit 0\n");
    (dir, bin)
}

fn stub(bin: &Path, name: &str, body: &str) {
    let path = bin.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}")).expect("write stub");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
}

/// Run the script in the fixture with the given stub settings. Returns
/// (exit code, stdout and stderr together, the toolchains rustup saw).
fn prepublish(name: &str, env: &[(&str, &str)]) -> (i32, String, String) {
    let (dir, bin) = fixture(name);
    let log = dir.join("rustup.log");
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new("sh")
        .arg("scripts/prepublish.sh")
        .current_dir(&dir)
        .env("PATH", path)
        .env("STUB_LOG", &log)
        .env_remove("TOOLCHAIN")
        .envs(env.iter().copied())
        .output()
        .expect("sh runs");
    let text =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    let seen = std::fs::read_to_string(&log).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    (out.status.code().unwrap_or(-1), text, seen)
}

/// The channel `rust-toolchain.toml` pins.
fn pinned_channel() -> String {
    let toml = std::fs::read_to_string(format!("{ROOT}/rust-toolchain.toml")).expect("pin");
    toml.lines()
        .find_map(|l| l.strip_prefix("channel = \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .expect("a channel line")
        .to_string()
}

#[test]
fn a_free_version_passes() {
    let (code, out, seen) = prepublish("free", &[]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("PRE-PUBLISH: ok"), "{out}");
    assert!(out.contains("version 9.9.9 is unpublished"), "{out}");
    // One home for the toolchain version: every check ran under the
    // channel the pin names, and under nothing else.
    let channel = pinned_channel();
    assert!(!seen.is_empty(), "rustup saw no toolchain");
    assert!(
        seen.lines().all(|l| l == channel),
        "a check ran under a toolchain other than {channel}: {seen}"
    );
}

#[test]
fn a_published_version_is_refused() {
    let (code, out, _) = prepublish("published", &[("STUB_HTTP_CODE", "200")]);
    assert_ne!(code, 0, "a published version passed: {out}");
    assert!(out.contains("already published"), "{out}");
    assert!(out.contains("PRE-PUBLISH: REFUSED"), "{out}");
}

#[test]
fn an_unanswered_check_is_refused() {
    // `curl -sf` exits nonzero for a 404, a 5xx, and a dead network
    // alike, so a version once read as free whenever crates.io did not
    // answer. Only a 404 is an answer that the version is free.
    let (code, out, _) = prepublish("five_xx", &[("STUB_HTTP_CODE", "503")]);
    assert_ne!(code, 0, "a 503 passed as unpublished: {out}");
    assert!(out.contains("crates.io answered 503"), "{out}");

    let (code, out, _) = prepublish("dead_network", &[("STUB_CURL_DIES", "1")]);
    assert_ne!(code, 0, "a dead network passed as unpublished: {out}");
    assert!(out.contains("crates.io answered 000"), "{out}");
}

#[test]
fn a_failing_check_prints_its_reason_and_the_rest_still_run() {
    // `set -e` once ended the script at the first failing check, before
    // the reason printed and before the later checks ran.
    let (code, out, _) = prepublish("failing", &[("STUB_FAIL", "clippy")]);
    assert_ne!(code, 0, "a failing check passed: {out}");
    assert!(out.contains("clippy") && out.contains("FAILS"), "{out}");
    assert!(
        out.contains("stub: clippy exploded"),
        "the refusal does not carry the check's output: {out}"
    );
    assert!(
        out.contains("version 9.9.9 is unpublished"),
        "the checks after the failing one did not run: {out}"
    );
    assert!(out.contains("PRE-PUBLISH: REFUSED"), "{out}");
}

#[test]
fn a_missing_cargo_hack_is_refused_with_the_install_command() {
    let (code, out, _) = prepublish("no_hack", &[("STUB_NO_HACK", "1")]);
    assert_ne!(code, 0, "{out}");
    assert!(out.contains("cargo install cargo-hack"), "{out}");
}
