//! The npm wrapper's platform table.
//!
//! The wrapper is the entry point for `npx msri`, and no Rust test
//! reaches it. A reviewer found its Linux arm broken and every suite
//! green: the table held only a musl key, and a glibc host fell
//! through to the refusal. These tests drive the real file with node.

use std::process::Command;

fn node_available() -> bool {
    Command::new("node").arg("--version").output().is_ok()
}

/// Run the wrapper with a forced platform and architecture.
fn run_for(platform: &str, arch: &str) -> String {
    let script = format!(
        "Object.defineProperty(process, 'platform', {{ value: '{platform}' }});\n\
         Object.defineProperty(process, 'arch', {{ value: '{arch}' }});\n\
         require('./npm/msri/bin/run.js');"
    );
    let out = Command::new("node")
        .args(["-e", &script])
        .output()
        .expect("node runs");
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn every_built_platform_is_in_the_table() {
    if !node_available() {
        return;
    }
    // The release builds these four. Each must reach a lookup rather
    // than the "no prebuilt binary" refusal. The package itself is not
    // installed here, so a hit reports a missing module instead.
    for (platform, arch) in [
        ("darwin", "arm64"),
        ("darwin", "x64"),
        ("linux", "arm64"),
        ("linux", "x64"),
    ] {
        let out = run_for(platform, arch);
        assert!(
            !out.contains("ships no prebuilt binary"),
            "{platform} {arch} is built but the wrapper refuses it: {out}"
        );
    }
}

#[test]
fn an_unsupported_platform_is_refused_by_name() {
    if !node_available() {
        return;
    }
    let out = run_for("sunos", "x64");
    assert!(
        out.contains("ships no prebuilt binary") && out.contains("sunos"),
        "an unsupported platform should be named in the refusal: {out}"
    );
}

#[test]
fn the_refusal_names_the_crate_not_the_package() {
    if !node_available() {
        return;
    }
    // The npm package and the crate carry different names here, and an
    // earlier message sent the reader to a crate that does not exist.
    let out = run_for("sunos", "x64");
    assert!(
        out.contains("cargo install missouri"),
        "the refusal should name the crate: {out}"
    );
}
