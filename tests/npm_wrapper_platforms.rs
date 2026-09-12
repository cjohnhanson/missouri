//! The npm wrapper's platform table.
//!
//! The wrapper is the entry point for `npx @cjohnhanson/msri`, and no Rust test
//! reaches it. A reviewer found its Linux arm broken and every suite
//! green: the table held only a musl key, and a glibc host fell
//! through to the refusal. These tests drive the real file with node.

mod common;

use common::{generated_package_names, generated_targets};
use std::process::Command;

fn node_available() -> bool {
    Command::new("node").arg("--version").output().is_ok()
}

/// Run the wrapper with a forced platform and architecture.
fn run_for(platform: &str, arch: &str) -> String {
    run_for_with(platform, arch, &[])
}

/// Run the wrapper with a forced platform and architecture, plus
/// environment for node.
fn run_for_with(platform: &str, arch: &str, envs: &[(&str, &str)]) -> String {
    let script = format!(
        "Object.defineProperty(process, 'platform', {{ value: '{platform}' }});\n\
         Object.defineProperty(process, 'arch', {{ value: '{arch}' }});\n\
         require('./npm/msri/bin/run.js');"
    );
    let out = Command::new("node")
        .args(["-e", &script])
        .envs(envs.iter().copied())
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
    // Assert what the wrapper says, not what it fails to say. An
    // absent-string assertion also passes when the file throws at load,
    // which is how a broken wrapper stayed green.
    // The table's package names are checked against the generator in
    // the_table_names_only_packages_the_generator_builds. This checks
    // what the wrapper says for each platform the release builds.
    for (platform, arch) in [
        ("darwin", "arm64"),
        ("darwin", "x64"),
        ("linux", "arm64"),
        ("linux", "x64"),
    ] {
        let out = run_for(platform, arch);
        // The package is not installed here, so a correct table reaches
        // the "supported, not installed" branch and names the platform.
        assert!(
            out.contains("supports") && out.contains(platform) && out.contains(arch),
            "{platform} {arch} should reach the not-installed branch: {out}"
        );
    }
}

#[test]
fn each_platform_execs_the_package_built_for_it() {
    if !node_available() {
        return;
    }
    // A name-set comparison passes with the darwin arm64 and x64 entries
    // swapped: both sets still match, and the wrapper execs a binary the
    // host cannot run. This installs a stub for every package the
    // generator builds, each printing its own package name, and reads
    // which one the wrapper execs for each platform.
    let root = std::env::temp_dir().join("msri_platform_probe");
    for t in generated_targets() {
        let dir = root.join("@cjohnhanson").join(&t.package);
        std::fs::create_dir_all(&dir).expect("stub dir");
        let stub = dir.join("missouri");
        std::fs::write(&stub, format!("#!/bin/sh\necho {}\n", t.package)).expect("write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }
    }
    // NODE_PATH is node's own extra lookup root for a bare specifier, so
    // the wrapper's require.resolve finds the stubs without a copy of
    // the wrapper or a node_modules under the repository.
    let node_path = root.to_str().expect("utf8");
    for t in generated_targets() {
        let out = run_for_with(&t.os, &t.cpu, &[("NODE_PATH", node_path)]);
        assert_eq!(
            out.trim(),
            t.package,
            "{} {} should exec {}, the package the generator builds for it",
            t.os,
            t.cpu,
            t.package
        );
    }
}

#[test]
fn the_wrapper_loads_without_throwing() {
    if !node_available() {
        return;
    }
    // Every other test here reads stdout and stderr, and a file that
    // throws at load produces output too. This one reads the exit code.
    let out = std::process::Command::new("node")
        .args(["-e", "require('./npm/msri/bin/run.js')"])
        .output()
        .expect("node runs");
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(
        !text.contains("SyntaxError") && !text.contains("ReferenceError"),
        "the wrapper throws at load: {text}"
    );
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

/// Run the wrapper with a binary override, and return (stdout+stderr, code).
fn run_with_binary(path: &str) -> (String, i32) {
    let out = Command::new("node")
        .args(["npm/msri/bin/run.js"])
        .env("MSRI_BINARY", path)
        .output()
        .expect("node runs");
    (
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn a_binary_killed_by_a_signal_does_not_exit_zero() {
    if !node_available() {
        return;
    }
    // A null status assigned straight to process.exitCode exits 0, so a
    // crashed tool read as a pass to every hook and CI job that calls
    // this wrapper. A shell reports 128 plus the signal number.
    let dir = std::env::temp_dir().join("msri_signal_probe");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let script = dir.join("missouri");
    std::fs::write(&script, "#!/bin/sh\nkill -SEGV $$\n").expect("write");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let (out, code) = run_with_binary(script.to_str().expect("utf8"));
    assert_ne!(code, 0, "a crash must not exit 0: {out}");
    assert!(out.contains("SIGSEGV"), "the signal should be named: {out}");
}

#[test]
fn an_override_that_cannot_run_is_refused_readably() {
    if !node_available() {
        return;
    }
    // A directory, or a file with no execute bit, threw a raw node
    // stack trace naming the wrapper's own line number.
    let dir = std::env::temp_dir();
    let (out, code) = run_with_binary(dir.to_str().expect("utf8"));
    assert_ne!(code, 0, "an unrunnable override must fail: {out}");
    assert!(
        out.contains("cannot run") && !out.contains("run.js:"),
        "the refusal should name the path, not a stack frame: {out}"
    );
}

#[test]
fn the_table_names_only_packages_the_generator_builds() {
    // The wrapper points at a package by name, and the generator writes
    // the package. Neither reads the other, so a rename on one side is
    // silent until a user meets it. This compares the two lists.
    let built = generated_package_names();
    let referenced = referenced_package_names();
    assert!(
        !referenced.is_empty(),
        "no package names parsed from the wrapper"
    );
    for name in &referenced {
        assert!(
            built.contains(name),
            "the wrapper points at {name}, which npm/generate.mjs never \
             builds; it builds {built:?}"
        );
    }
    for name in &built {
        assert!(
            referenced.contains(name),
            "npm/generate.mjs builds {name}, which the wrapper never \
             points at; it points at {referenced:?}"
        );
    }
}

/// The package names the wrapper's platform table points at.
fn referenced_package_names() -> Vec<String> {
    let src = std::fs::read_to_string("npm/msri/bin/run.js").expect("wrapper");
    let mut names = Vec::new();
    for line in src.lines() {
        let Some(rest) = line.split("@cjohnhanson/").nth(1) else {
            continue;
        };
        let Some(name) = rest.split('/').next() else {
            continue;
        };
        if name.starts_with("msri-") {
            names.push(name.to_string());
        }
    }
    names.sort();
    names.dedup();
    names
}
