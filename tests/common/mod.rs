//! What `npm/generate.mjs` writes, read from its target list. The
//! wrapper test and the release-workflow test both check a hand-written
//! name against it.

// Cargo compiles this module into each test binary, and each one uses
// a different subset of it.
#![allow(dead_code)]

/// One target the generator builds: the node platform and architecture
/// it serves, and the package name it writes.
pub struct Target {
    /// The Rust target triple, which names the built binary's directory.
    pub triple: String,
    pub os: String,
    pub cpu: String,
    pub package: String,
}

/// The generator's target list. It composes each package name as
/// `msri-<os>-<cpu>` plus `-<libc>` where the target names one.
pub fn generated_targets() -> Vec<Target> {
    let src = std::fs::read_to_string("npm/generate.mjs").expect("generator");
    let mut targets = Vec::new();
    for line in src.lines() {
        let Some(triple) = between(line, "target: \"", "\"") else {
            continue;
        };
        let Some(os) = between(line, "os: \"", "\"") else {
            continue;
        };
        let Some(cpu) = between(line, "cpu: \"", "\"") else {
            continue;
        };
        let libc = between(line, "libc: \"", "\"");
        let package = libc.map_or_else(
            || format!("msri-{os}-{cpu}"),
            |l| format!("msri-{os}-{cpu}-{l}"),
        );
        targets.push(Target {
            triple,
            os,
            cpu,
            package,
        });
    }
    assert!(
        !targets.is_empty(),
        "no targets parsed from npm/generate.mjs"
    );
    targets
}

/// The package names the generator builds.
pub fn generated_package_names() -> Vec<String> {
    generated_targets().into_iter().map(|t| t.package).collect()
}

/// The text between `open` and the next `close` on one line.
pub fn between(line: &str, open: &str, close: &str) -> Option<String> {
    let start = line.find(open)? + open.len();
    let rest = &line[start..];
    let end = rest.find(close)?;
    Some(rest[..end].to_string())
}
