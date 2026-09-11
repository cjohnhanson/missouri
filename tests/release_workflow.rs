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

/// The targets the build job's matrix lists.
fn build_targets() -> Vec<String> {
    let targets: Vec<String> = job("build")
        .iter()
        .filter_map(|l| l.trim().strip_prefix("- target: ").map(str::to_string))
        .collect();
    assert!(!targets.is_empty(), "no `- target:` line in the build job");
    targets
}

#[test]
fn the_formula_names_the_archive_the_build_uploads() {
    // The build job names its archive once. The formula fetches it by
    // name from another repository, and the tap job renders the formula
    // from the template here, so the template is where the two names
    // can be read together. The first formula asked for
    // <bin>-<target>.tar.gz while the build uploaded
    // <bin>-<tag>-<target>.tar.gz, and brew would have met a 404.
    let src = workflow();
    assert!(
        src.contains(&format!("BIN_NAME: {BIN}")),
        "the workflow's BIN_NAME is not {BIN}"
    );
    let template_path = format!("homebrew/{BIN}.rb");
    assert!(
        src.contains(&format!("cp {template_path} ")),
        "the tap job does not render {template_path}"
    );
    let template = fs::read_to_string(&template_path).expect("the formula template");
    let staging = src
        .lines()
        .find_map(|l| between(l, "staging=\"", "\""))
        .expect("the build job's staging= line");
    for target in build_targets() {
        let archive = staging
            .replace("${BIN_NAME}", BIN)
            .replace("${GITHUB_REF_NAME}", "v#{version}")
            .replace("${{ matrix.target }}", &target);
        assert!(
            template.contains(&format!("/v#{{version}}/{archive}.tar.gz\"")),
            "{template_path} fetches no {archive}.tar.gz, which the build uploads"
        );
    }
    // The checksum the tap reads is the build's own, per target.
    assert!(
        src.contains(&format!("dl/{BIN}-*-\"${{target}}\".tar.gz.sha256")),
        "the tap job does not read the build's checksum files"
    );
    // The tap job replaces the version by matching one exact line of
    // the template. A reformat of that line leaves 0.0.0 in the formula
    // with correct checksums and four URLs that 404.
    assert!(
        template.lines().any(|l| l == "  version \"0.0.0\""),
        "{template_path} has no `  version \"0.0.0\"` line for the tap job to replace"
    );
    assert!(
        src.contains("s/^  version \\\".*\\\"$/  version \\\"${version}\\\"/"),
        "the tap job does not replace the formula's version line"
    );
}

#[test]
fn a_dispatch_runs_the_deb_and_tap_jobs() {
    // A dispatch rehearses the release. Gated on the event at the job,
    // these two were the ones it skipped, and both were broken while the
    // rehearsal read green. The event gate belongs on the upload step.
    for name in ["debs", "tap"] {
        let lines = job(name);
        assert!(
            !lines
                .iter()
                .any(|l| l.starts_with("    if:") && l.contains("event_name")),
            "job {name} gates on the event at the job, so a dispatch skips it"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.trim() == "if: github.event_name == 'push'"
                    || l.trim() == "github.event_name == 'push'"),
            "job {name} has no step gated on a push, so a dispatch uploads"
        );
    }
    // `!cancelled()` on the tap job drops the default success check on
    // its `needs`, so the push step must require the publishes itself.
    // Without that, a failed publish pushed a formula for a draft.
    let tap = job("tap");
    for publish in ["publish-crate", "publish-pypi", "publish-npm", "finalize"] {
        assert!(
            tap.iter()
                .any(|l| l.contains(&format!("needs.{publish}.result == 'success'"))),
            "the tap job pushes without requiring {publish}"
        );
    }
    // A draft release serves no asset to an anonymous reader, so the
    // formula must follow finalize, which clears the draft flag. With
    // the order reversed, `brew install` met a 404 on every release.
    assert!(
        !job("finalize")
            .iter()
            .any(|l| l.trim().starts_with("needs:") && l.contains("tap")),
        "finalize waits for tap, so the formula names a draft release"
    );
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
fn the_verify_job_holds_a_tag_to_main_and_to_the_manifest() {
    // Anyone who can push a tag can point one at any commit, and a tag
    // that disagrees with Cargo.toml publishes a version nobody asked
    // for. Both checks need the full history: a shallow clone answers
    // the ancestry question wrong.
    let lines = job("verify");
    assert!(
        lines.iter().any(|l| l.trim() == "fetch-depth: 0"),
        "the verify checkout is shallow, so the ancestry check cannot answer"
    );
    let step = |name: &str| -> Vec<&String> {
        let start = lines
            .iter()
            .position(|l| l.trim() == format!("- name: {name}"))
            .unwrap_or_else(|| panic!("no step named {name} in verify"));
        lines[start + 1..]
            .iter()
            .take_while(|l| !l.trim().starts_with("- "))
            .collect()
    };
    let on_main = step("Check the tag is on main");
    assert!(
        on_main
            .iter()
            .any(|l| l.contains("merge-base --is-ancestor") && l.contains("origin/main")),
        "the tag-on-main step does not ask git for ancestry against main"
    );
    let manifest = step("Check the tag matches Cargo.toml");
    assert!(
        manifest
            .iter()
            .any(|l| l.contains("Cargo.toml") && l.contains("version")),
        "the manifest step does not read the version from Cargo.toml"
    );
    assert!(
        manifest.iter().any(|l| l.contains("GITHUB_REF_NAME")),
        "the manifest step does not compare against the tag"
    );
    for (name, body) in [("on main", &on_main), ("manifest", &manifest)] {
        assert!(
            body.iter()
                .any(|l| l.trim() == "if: github.event_name == 'push'"),
            "the {name} check runs on a dispatch, where there is no tag"
        );
        assert!(
            body.iter().any(|l| l.trim() == "exit 1"),
            "the {name} check cannot refuse"
        );
    }
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
