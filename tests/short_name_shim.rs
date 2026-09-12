//! The short-name binary is what `uvx msri` and the .deb install, and
//! it execs `missouri` beside it. No test ran it, so a wrong sibling
//! name or a dropped argument left every suite green.

use std::process::Command;

fn run(exe: &str, args: &[&str]) -> (String, i32) {
    let out = Command::new(exe).args(args).output().expect("binary runs");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn the_shim_answers_as_the_long_name_does() {
    let (short, short_code) = run(env!("CARGO_BIN_EXE_msri"), &["--version"]);
    let (long, long_code) = run(env!("CARGO_BIN_EXE_missouri"), &["--version"]);
    assert_eq!(short_code, long_code);
    assert_eq!(short, long, "the shim should print what missouri prints");
    assert!(!short.trim().is_empty(), "--version printed nothing");
}

#[test]
fn the_shim_passes_every_argument_through() {
    // `docs` with a topic reads two arguments; a shim that dropped one
    // would list the topics instead.
    let (short, _) = run(env!("CARGO_BIN_EXE_msri"), &["docs", "cli-reference"]);
    let (long, _) = run(env!("CARGO_BIN_EXE_missouri"), &["docs", "cli-reference"]);
    assert_eq!(short, long);
    assert!(short.len() > 200, "the page should print in full: {short}");
}
