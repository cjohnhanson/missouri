//! The npm package generator, run. Every other test reads its target
//! list as text; none executed it, so its refusal of an incomplete
//! binary set and the manifest it writes were never checked. Both
//! reach a user: the refusal is what stops a wrapper naming a package
//! that was never built, and npm reads the manifest on install.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::generated_targets;

const SHORT: &str = "msri";
const CMD: &str = "missouri";

fn node_available() -> bool {
    Command::new("node").arg("--version").output().is_ok()
}

/// A copy of what the generator reads: its own file, the wrapper
/// manifest beside it, and Cargo.toml one level up.
fn scratch_repo(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("{SHORT}_generate_{name}"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("npm").join(SHORT)).expect("scratch npm dir");
    std::fs::copy("npm/generate.mjs", root.join("npm/generate.mjs")).expect("copy generator");
    std::fs::copy(
        format!("npm/{SHORT}/package.json"),
        root.join("npm").join(SHORT).join("package.json"),
    )
    .expect("copy wrapper manifest");
    std::fs::copy("Cargo.toml", root.join("Cargo.toml")).expect("copy manifest");
    root
}

fn put_binary(root: &Path, target: &str) {
    let dir = root.join("bins").join(target);
    std::fs::create_dir_all(&dir).expect("bins dir");
    std::fs::write(dir.join(CMD), "#!/bin/sh\necho stub\n").expect("stub binary");
}

fn generate(root: &Path, extra: &[&str]) -> std::process::Output {
    Command::new("node")
        .arg("npm/generate.mjs")
        .args([SHORT, CMD, "bins"])
        .args(extra)
        .current_dir(root)
        .output()
        .expect("node runs")
}

#[test]
fn an_incomplete_binary_set_is_refused() {
    if !node_available() {
        return;
    }
    let root = scratch_repo("incomplete");
    let first = &generated_targets()[0];
    put_binary(&root, &first.triple);
    let out = generate(&root, &[]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "one binary of four should refuse: {stderr}"
    );
    assert!(
        stderr.contains("build every target first"),
        "the refusal should say what is missing: {stderr}"
    );
    assert!(
        !root.join("npm").join(&first.package).exists(),
        "a refusal must write no package"
    );
}

#[test]
fn a_partial_build_writes_one_package_npm_will_install() {
    if !node_available() {
        return;
    }
    let root = scratch_repo("partial");
    let first = &generated_targets()[0];
    put_binary(&root, &first.triple);
    let out = generate(&root, &["--partial"]);
    assert!(
        out.status.success(),
        "--partial should accept one binary: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let pkg = root.join("npm").join(&first.package);
    let manifest = std::fs::read_to_string(pkg.join("package.json")).expect("manifest");
    assert!(
        manifest.contains(&format!("\"name\": \"@cjohnhanson/{}\"", first.package)),
        "manifest: {manifest}"
    );
    assert!(
        manifest.contains(&format!("\"os\": [\n    \"{}\"", first.os))
            && manifest.contains(&format!("\"cpu\": [\n    \"{}\"", first.cpu)),
        "manifest names its platform: {manifest}"
    );
    // npm enforces `libc`, and a static musl binary runs on glibc too,
    // so the field made `npm install` refuse the package on Ubuntu.
    assert!(
        !manifest.contains("libc"),
        "manifest carries libc: {manifest}"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(pkg.join(CMD))
            .expect("binary")
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o111,
            0o111,
            "npm restores no execute bit; the generator sets it"
        );
    }
    let wrapper = std::fs::read_to_string(root.join("npm").join(SHORT).join("package.json"))
        .expect("wrapper manifest");
    let deps: Vec<&str> = wrapper
        .lines()
        .filter(|l| l.contains("@cjohnhanson/"))
        .collect();
    assert_eq!(
        deps.len(),
        1,
        "a partial build must leave only the built package as a dependency: {deps:?}"
    );
    assert!(deps[0].contains(&first.package), "{deps:?}");
}
