//! The release workflow, read as text.
//!
//! No job in the workflow runs before a tag, and a tag publishes to
//! registries that never take a version back. A reviewer found the
//! workflow packing a package path the generator never writes,
//! publishing from a glob that matched nothing, and downloading an
//! archive under a name the build never uploads. Every suite was green.
//! These tests read the workflow against the files it names, so the
//! next rename fails here and not on a tag.

mod common;

use common::{between, generated_package_names};
use std::fs;

/// The npm wrapper's name, and the prefix of every platform package.
const SHORT: &str = "msri";
/// The binary the build job archives.
const BIN: &str = "missouri";
const WORKFLOW: &str = ".github/workflows/release.yml";

fn workflow() -> String {
    fs::read_to_string(WORKFLOW).expect("release workflow")
}

/// The lines of one job, from its header to the next job's.
fn job(name: &str) -> Vec<String> {
    let src = workflow();
    let header = format!("  {name}:");
    let mut lines = src.lines().skip_while(|l| *l != header);
    let first = lines
        .next()
        .unwrap_or_else(|| panic!("no job {name} in {WORKFLOW}"));
    std::iter::once(first)
        .chain(lines.take_while(|l| !is_job_header(l)))
        .map(str::to_string)
        .collect()
}

fn is_job_header(line: &str) -> bool {
    line.starts_with("  ") && !line.starts_with("   ") && line.ends_with(':')
}

#[test]
fn npm_pack_names_a_package_the_generator_builds() {
    let built = generated_package_names();
    let mut packed = 0;
    for line in workflow().lines() {
        let Some(rest) = line.trim().strip_prefix("npm pack ./npm/") else {
            continue;
        };
        let name = rest
            .split_whitespace()
            .next()
            .expect("a path after npm pack");
        packed += 1;
        assert!(
            name == SHORT || built.iter().any(|b| b == name),
            "{WORKFLOW} packs npm/{name}, which npm/generate.mjs never \
             writes; it writes {built:?}"
        );
    }
    assert!(packed > 0, "no `npm pack ./npm/` line in {WORKFLOW}");
}

#[test]
fn npm_publish_globs_the_prefix_the_generator_writes() {
    // A glob that matches nothing runs the loop zero times, and the job
    // reports success with no platform package published.
    let line = workflow()
        .lines()
        .find(|l| l.contains("for d in npm/"))
        .map(str::to_string)
        .expect("a publish loop over npm/ in the workflow");
    let glob = between(&line, "for d in npm/", ";").expect("a glob after `for d in npm/`");
    assert_eq!(
        glob,
        format!("{SHORT}-*"),
        "the publish loop globs npm/{glob}; the generator writes npm/{SHORT}-<os>-<cpu>"
    );
}

#[test]
fn the_tap_downloads_the_archive_the_build_uploads() {
    // The build job names its archive once, and the tap job spells the
    // same name a second time by hand. The two drifted: the tap read
    // the name without the tag and exited 1 on every release.
    let src = workflow();
    assert!(
        src.contains(&format!("BIN_NAME: {BIN}")),
        "the workflow's BIN_NAME is not {BIN}"
    );
    let staging = src
        .lines()
        .find_map(|l| between(l, "staging=\"", "\""))
        .expect("the build job's staging= line");
    let uploaded = staging
        .replace("${BIN_NAME}", BIN)
        .replace("${{ matrix.target }}", "${target}");
    let read = src
        .lines()
        .find_map(|l| between(l, "file=\"dl/", "\""))
        .expect("the tap job's file=\"dl/ line");
    assert_eq!(
        read,
        format!("{uploaded}.tar.gz"),
        "the tap job reads an archive name the build job never uploads"
    );
}

#[test]
fn a_dispatch_runs_the_deb_and_tap_jobs() {
    // A dispatch rehearses the release. Gated at the job, these two were
    // the ones it skipped, and both were broken while the rehearsal read
    // green. The gate belongs on the upload step.
    for name in ["debs", "tap"] {
        let lines = job(name);
        assert!(
            !lines.iter().any(|l| l.starts_with("    if:")),
            "job {name} carries a job-level if, so a dispatch skips it"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.trim() == "if: github.event_name == 'push'"),
            "job {name} has no step gated on a push, so a dispatch uploads"
        );
    }
}

#[test]
fn the_deb_job_fetches_the_binary_it_packages() {
    let lines = job("debs");
    let fetch = lines
        .iter()
        .position(|l| l.contains("actions/download-artifact"))
        .expect("the debs job downloads the build artifact");
    let pack = lines
        .iter()
        .position(|l| l.contains("cargo deb "))
        .expect("the debs job runs cargo deb");
    assert!(
        fetch < pack,
        "cargo deb --no-build runs before the binary is on disk"
    );
}

#[test]
fn git_ignores_the_generator_output() {
    // A generated package holds a binary, and one staged by accident
    // ships inside the crate.
    let ignore = fs::read_to_string(".gitignore").expect(".gitignore");
    let pattern = format!("npm/{SHORT}-*/");
    assert!(
        ignore.lines().any(|l| l == pattern),
        ".gitignore does not carry {pattern}"
    );
}
