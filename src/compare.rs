use std::collections::{BTreeMap, BTreeSet};

use camino::{Utf8Path, Utf8PathBuf};
use ignore::gitignore::Gitignore;

use crate::graph::{EnvComparator, FileComparator};

/// Result of comparing actual state against expected state.
#[derive(Debug)]
pub struct ComparisonResult {
    pub passed: bool,
    pub file_diffs: Vec<FileDiff>,
    pub env_diffs: Vec<EnvDiff>,
}

/// A single file-level difference.
#[derive(Debug)]
pub enum FileDiff {
    /// File exists only in actual, not in expected.
    ExtraFile { path: Utf8PathBuf },
    /// File exists only in expected, not in actual.
    MissingFile { path: Utf8PathBuf },
    /// Files exist in both but differ.
    ContentMismatch { path: Utf8PathBuf, detail: String },
    /// Custom comparator returned nonzero.
    ComparatorFailed {
        path: Utf8PathBuf,
        command: String,
        stderr: String,
    },
}

/// A single env var difference.
#[derive(Debug)]
pub enum EnvDiff {
    ExtraVar {
        name: String,
    },
    MissingVar {
        name: String,
    },
    ValueMismatch {
        name: String,
        actual: String,
        expected: String,
    },
    ComparatorFailed {
        name: String,
        command: String,
        stderr: String,
    },
}

/// A difference in command output (stdout or stderr).
#[derive(Debug)]
pub enum OutputDiff {
    StdoutMismatch { expected: String, actual: String },
    StderrMismatch { expected: String, actual: String },
}

/// Compare the actual directory tree against the expected directory tree.
///
/// `actual` is the temp dir after the transition command ran.
/// `expected` is the target state directory.
/// `.missouri/` is excluded from both sides.
/// `file_comparators` are the per-transition overrides.
pub fn compare_trees(
    actual: &Utf8Path,
    expected: &Utf8Path,
    file_comparators: &[(Utf8PathBuf, FileComparator)],
    bin_dirs: &[&Utf8Path],
    state_env: &BTreeMap<String, String>,
    config_dir: &str,
    ignore: &Gitignore,
    sandbox: &dyn crate::executor::Backend,
) -> ComparisonResult {
    let actual_files = walk_tree(actual, config_dir);
    let expected_files = walk_tree(expected, config_dir);

    let all_paths: BTreeSet<&Utf8PathBuf> = actual_files
        .iter()
        .chain(expected_files.iter())
        .filter(|p| {
            let is_dir = actual.join(p).is_dir() || expected.join(p).is_dir();
            !ignore
                .matched_path_or_any_parents(p.as_str(), is_dir)
                .is_ignore()
        })
        .collect();

    let mut diffs = Vec::new();

    for path in all_paths {
        // Check if this path is covered by a comparator override
        if let Some(comparator) = find_comparator(path, file_comparators) {
            match comparator {
                FileComparator::Ignore => continue,
                FileComparator::Custom { command } => {
                    let actual_path = actual.join(path);
                    let expected_path = expected.join(path);

                    if !actual_path.exists() {
                        diffs.push(FileDiff::MissingFile { path: path.clone() });
                        continue;
                    }
                    if !expected_path.exists() {
                        diffs.push(FileDiff::ExtraFile { path: path.clone() });
                        continue;
                    }

                    match run_comparator(
                        command,
                        &actual_path,
                        &expected_path,
                        bin_dirs,
                        state_env,
                        sandbox,
                    ) {
                        Ok(()) => {}
                        Err(stderr) => {
                            diffs.push(FileDiff::ComparatorFailed {
                                path: path.clone(),
                                command: command.clone(),
                                stderr,
                            });
                        }
                    }
                    continue;
                }
            }
        }

        let in_actual = actual_files.contains(path);
        let in_expected = expected_files.contains(path);

        match (in_actual, in_expected) {
            (true, false) => {
                diffs.push(FileDiff::ExtraFile { path: path.clone() });
            }
            (false, true) => {
                diffs.push(FileDiff::MissingFile { path: path.clone() });
            }
            (true, true) => {
                let actual_path = actual.join(path);
                let expected_path = expected.join(path);

                // Only compare regular files, not directories
                if actual_path.is_file() && expected_path.is_file() {
                    match compare_files_byte_equal(&actual_path, &expected_path) {
                        Ok(true) => {}
                        Ok(false) => {
                            diffs.push(FileDiff::ContentMismatch {
                                path: path.clone(),
                                detail: format_content_diff(&actual_path, &expected_path),
                            });
                        }
                        Err(e) => {
                            diffs.push(FileDiff::ContentMismatch {
                                path: path.clone(),
                                detail: format!("failed to read: {e}"),
                            });
                        }
                    }
                }
            }
            (false, false) => unreachable!(),
        }
    }

    let passed = diffs.is_empty();
    ComparisonResult {
        passed,
        file_diffs: diffs,
        env_diffs: Vec::new(),
    }
}

/// Compare environment variables between actual and expected states.
pub fn compare_env(
    actual_env: &BTreeMap<String, String>,
    expected_env: &BTreeMap<String, String>,
    env_comparators: &[(String, EnvComparator)],
    bin_dirs: &[&Utf8Path],
    state_env: &BTreeMap<String, String>,
    sandbox: &dyn crate::executor::Backend,
) -> Vec<EnvDiff> {
    let mut diffs = Vec::new();
    let all_keys: BTreeSet<&String> = actual_env.keys().chain(expected_env.keys()).collect();

    for key in all_keys {
        if let Some(comparator) = find_env_comparator(key, env_comparators) {
            match comparator {
                EnvComparator::Ignore => continue,
                EnvComparator::Custom { command } => {
                    let actual_val = actual_env.get(key).map(|s| s.as_str()).unwrap_or("");
                    let expected_val = expected_env.get(key).map(|s| s.as_str()).unwrap_or("");
                    // For env comparators, pass values as args
                    match run_comparator(
                        command,
                        Utf8Path::new(actual_val),
                        Utf8Path::new(expected_val),
                        bin_dirs,
                        state_env,
                        sandbox,
                    ) {
                        Ok(()) => {}
                        Err(stderr) => {
                            diffs.push(EnvDiff::ComparatorFailed {
                                name: key.clone(),
                                command: command.clone(),
                                stderr,
                            });
                        }
                    }
                    continue;
                }
            }
        }

        let in_actual = actual_env.contains_key(key);
        let in_expected = expected_env.contains_key(key);

        match (in_actual, in_expected) {
            (true, false) => diffs.push(EnvDiff::ExtraVar { name: key.clone() }),
            (false, true) => diffs.push(EnvDiff::MissingVar { name: key.clone() }),
            (true, true) => {
                let a = &actual_env[key];
                let e = &expected_env[key];
                if a != e {
                    diffs.push(EnvDiff::ValueMismatch {
                        name: key.clone(),
                        actual: a.clone(),
                        expected: e.clone(),
                    });
                }
            }
            (false, false) => unreachable!(),
        }
    }

    diffs
}

/// Walk a directory tree and collect every relative path. Skips the config
/// directory, for example `.missouri/`.
fn walk_tree(root: &Utf8Path, config_dir: &str) -> BTreeSet<Utf8PathBuf> {
    let mut paths = BTreeSet::new();
    walk_recursive(root, root, config_dir, &mut paths);
    paths
}

fn walk_recursive(
    root: &Utf8Path,
    dir: &Utf8Path,
    config_dir: &str,
    out: &mut BTreeSet<Utf8PathBuf>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = match Utf8PathBuf::try_from(entry.path()) {
            Ok(p) => p,
            Err(_) => continue,
        };

        let name = path.file_name().unwrap_or("");

        // Skip config metadata directory
        if name == config_dir {
            continue;
        }

        let relative = path.strip_prefix(root).unwrap_or(&path).to_owned();

        if path.is_dir() {
            // Include the directory itself in the set
            out.insert(relative.clone());
            walk_recursive(root, &path, config_dir, out);
        } else {
            out.insert(relative);
        }
    }
}

fn find_comparator<'a>(
    path: &Utf8Path,
    comparators: &'a [(Utf8PathBuf, FileComparator)],
) -> Option<&'a FileComparator> {
    for (pattern, comparator) in comparators {
        let pattern_str = pattern.as_str();
        // Directory comparator (trailing /)
        if pattern_str.ends_with('/') {
            let prefix = pattern_str.trim_end_matches('/');
            if path.as_str().starts_with(prefix) {
                return Some(comparator);
            }
        } else if path == pattern {
            return Some(comparator);
        }
    }
    None
}

fn find_env_comparator<'a>(
    name: &str,
    comparators: &'a [(String, EnvComparator)],
) -> Option<&'a EnvComparator> {
    comparators.iter().find(|(n, _)| n == name).map(|(_, c)| c)
}

fn compare_files_byte_equal(a: &Utf8Path, b: &Utf8Path) -> std::io::Result<bool> {
    let a_bytes = std::fs::read(a)?;
    let b_bytes = std::fs::read(b)?;
    Ok(a_bytes == b_bytes)
}

fn format_content_diff(actual: &Utf8Path, expected: &Utf8Path) -> String {
    let a = std::fs::read_to_string(actual).unwrap_or_else(|_| "<binary>".into());
    let e = std::fs::read_to_string(expected).unwrap_or_else(|_| "<binary>".into());

    if a.len() > 1000 || e.len() > 1000 {
        return "files differ (content too large to display)".into();
    }

    format!("expected:\n{e}\nactual:\n{a}")
}

fn run_comparator(
    command: &str,
    arg1: &Utf8Path,
    arg2: &Utf8Path,
    bin_dirs: &[&Utf8Path],
    state_env: &BTreeMap<String, String>,
    sandbox: &dyn crate::executor::Backend,
) -> Result<(), String> {
    // Build PATH: bin dirs → state_env PATH → system PATH → fallback
    let system_path =
        std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into());
    let base_path = state_env
        .get("PATH")
        .map(|s| s.as_str())
        .unwrap_or(&system_path);
    let mut path_parts: Vec<&str> = bin_dirs.iter().map(|b| b.as_str()).collect();
    path_parts.push(base_path);
    let path_env = path_parts.join(":");

    let inner_cmd = format!(
        "{command} {} {}",
        shell_quote(arg1.as_str()),
        shell_quote(arg2.as_str())
    );

    // A comparator always runs as a shell command, because inner_cmd is a
    // shell expression. Pass a placeholder work_dir. A comparator needs no
    // working directory.
    let work_dir = camino::Utf8Path::new("/");
    let output = crate::signal::run_tracked(
        &mut sandbox.build_shell_command(&inner_cmd, work_dir, state_env, &path_env),
    )
    .map_err(|e| format!("failed to run comparator: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ignore::gitignore::GitignoreBuilder;
    use std::fs;

    fn empty_ignore() -> Gitignore {
        GitignoreBuilder::new("").build().unwrap()
    }

    #[test]
    fn identical_trees_pass() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let a = root.join("a");
        let b = root.join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();

        fs::write(a.join("file.txt"), "hello").unwrap();
        fs::write(b.join("file.txt"), "hello").unwrap();

        let result = compare_trees(
            &a,
            &b,
            &[],
            &[],
            &BTreeMap::new(),
            ".missouri",
            &empty_ignore(),
            &crate::executor::BareBackend,
        );
        assert!(result.passed);
        assert!(result.file_diffs.is_empty());
    }

    #[test]
    fn content_mismatch_detected() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let a = root.join("a");
        let b = root.join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();

        fs::write(a.join("file.txt"), "hello").unwrap();
        fs::write(b.join("file.txt"), "world").unwrap();

        let result = compare_trees(
            &a,
            &b,
            &[],
            &[],
            &BTreeMap::new(),
            ".missouri",
            &empty_ignore(),
            &crate::executor::BareBackend,
        );
        assert!(!result.passed);
        assert_eq!(result.file_diffs.len(), 1);
        assert!(matches!(
            &result.file_diffs[0],
            FileDiff::ContentMismatch { .. }
        ));
    }

    #[test]
    fn extra_file_detected() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let a = root.join("a");
        let b = root.join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();

        fs::write(a.join("file.txt"), "hello").unwrap();
        fs::write(a.join("extra.txt"), "surprise").unwrap();
        fs::write(b.join("file.txt"), "hello").unwrap();

        let result = compare_trees(
            &a,
            &b,
            &[],
            &[],
            &BTreeMap::new(),
            ".missouri",
            &empty_ignore(),
            &crate::executor::BareBackend,
        );
        assert!(!result.passed);
        assert!(result
            .file_diffs
            .iter()
            .any(|d| matches!(d, FileDiff::ExtraFile { path } if path.as_str() == "extra.txt")));
    }

    #[test]
    fn missing_file_detected() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let a = root.join("a");
        let b = root.join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();

        fs::write(a.join("file.txt"), "hello").unwrap();
        fs::write(b.join("file.txt"), "hello").unwrap();
        fs::write(b.join("needed.txt"), "important").unwrap();

        let result = compare_trees(
            &a,
            &b,
            &[],
            &[],
            &BTreeMap::new(),
            ".missouri",
            &empty_ignore(),
            &crate::executor::BareBackend,
        );
        assert!(!result.passed);
        assert!(result
            .file_diffs
            .iter()
            .any(|d| matches!(d, FileDiff::MissingFile { path } if path.as_str() == "needed.txt")));
    }

    #[test]
    fn ignore_comparator_skips_file() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let a = root.join("a");
        let b = root.join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();

        fs::write(a.join("data.txt"), "same").unwrap();
        fs::write(b.join("data.txt"), "same").unwrap();
        fs::write(a.join("log.txt"), "different1").unwrap();
        fs::write(b.join("log.txt"), "different2").unwrap();

        let comparators = vec![(Utf8PathBuf::from("log.txt"), FileComparator::Ignore)];

        let result = compare_trees(
            &a,
            &b,
            &comparators,
            &[],
            &BTreeMap::new(),
            ".missouri",
            &empty_ignore(),
            &crate::executor::BareBackend,
        );
        assert!(result.passed);
    }

    #[test]
    fn missouri_dir_excluded() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let a = root.join("a");
        let b = root.join("b");
        fs::create_dir_all(a.join(".missouri")).unwrap();
        fs::create_dir_all(&b).unwrap();

        fs::write(a.join("file.txt"), "hello").unwrap();
        fs::write(b.join("file.txt"), "hello").unwrap();
        fs::write(a.join(".missouri").join("missouri.yml"), "{}").unwrap();
        // .missouri/ only exists in a, but should be excluded from comparison

        let result = compare_trees(
            &a,
            &b,
            &[],
            &[],
            &BTreeMap::new(),
            ".missouri",
            &empty_ignore(),
            &crate::executor::BareBackend,
        );
        assert!(result.passed);
    }

    #[test]
    fn env_comparison_identical() {
        let mut a = BTreeMap::new();
        a.insert("KEY".into(), "value".into());
        let mut b = BTreeMap::new();
        b.insert("KEY".into(), "value".into());

        let diffs = compare_env(
            &a,
            &b,
            &[],
            &[],
            &BTreeMap::new(),
            &crate::executor::BareBackend,
        );
        assert!(diffs.is_empty());
    }

    #[test]
    fn env_comparison_mismatch() {
        let mut a = BTreeMap::new();
        a.insert("KEY".into(), "val1".into());
        let mut b = BTreeMap::new();
        b.insert("KEY".into(), "val2".into());

        let diffs = compare_env(
            &a,
            &b,
            &[],
            &[],
            &BTreeMap::new(),
            &crate::executor::BareBackend,
        );
        assert_eq!(diffs.len(), 1);
        assert!(matches!(&diffs[0], EnvDiff::ValueMismatch { name, .. } if name == "KEY"));
    }

    #[test]
    fn env_ignore_comparator() {
        let mut a = BTreeMap::new();
        a.insert("TIMESTAMP".into(), "123".into());
        let mut b = BTreeMap::new();
        b.insert("TIMESTAMP".into(), "456".into());

        let comparators = vec![("TIMESTAMP".into(), EnvComparator::Ignore)];
        let diffs = compare_env(
            &a,
            &b,
            &comparators,
            &[],
            &BTreeMap::new(),
            &crate::executor::BareBackend,
        );
        assert!(diffs.is_empty());
    }

    fn build_ignore(patterns: &[&str]) -> Gitignore {
        let mut builder = GitignoreBuilder::new("");
        for p in patterns {
            builder.add_line(None, p).unwrap();
        }
        builder.build().unwrap()
    }

    #[test]
    fn ignore_file_filters_extra_files() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let a = root.join("a");
        let b = root.join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();

        fs::write(a.join("file.txt"), "hello").unwrap();
        fs::write(b.join("file.txt"), "hello").unwrap();
        // Extra file in actual that would normally fail
        fs::write(a.join("debug.log"), "some logs").unwrap();

        let ignore = build_ignore(&["*.log"]);
        let result = compare_trees(
            &a,
            &b,
            &[],
            &[],
            &BTreeMap::new(),
            ".missouri",
            &ignore,
            &crate::executor::BareBackend,
        );
        assert!(result.passed);
    }

    #[test]
    fn ignore_file_filters_differing_files() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let a = root.join("a");
        let b = root.join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();

        fs::write(a.join("file.txt"), "hello").unwrap();
        fs::write(b.join("file.txt"), "hello").unwrap();
        // Same name, different content — would normally be ContentMismatch
        fs::write(a.join("cache.bin"), "version1").unwrap();
        fs::write(b.join("cache.bin"), "version2").unwrap();

        let ignore = build_ignore(&["*.bin"]);
        let result = compare_trees(
            &a,
            &b,
            &[],
            &[],
            &BTreeMap::new(),
            ".missouri",
            &ignore,
            &crate::executor::BareBackend,
        );
        assert!(result.passed);
    }

    #[test]
    fn ignore_file_filters_directory_contents() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let a = root.join("a");
        let b = root.join("b");
        fs::create_dir_all(a.join("__pycache__")).unwrap();
        fs::create_dir_all(&b).unwrap();

        fs::write(a.join("main.py"), "print('hi')").unwrap();
        fs::write(b.join("main.py"), "print('hi')").unwrap();
        fs::write(
            a.join("__pycache__").join("main.cpython-312.pyc"),
            "bytecode",
        )
        .unwrap();

        // Gitignore semantics: trailing / matches directory and all contents
        let ignore = build_ignore(&["__pycache__/"]);
        let result = compare_trees(
            &a,
            &b,
            &[],
            &[],
            &BTreeMap::new(),
            ".missouri",
            &ignore,
            &crate::executor::BareBackend,
        );
        assert!(result.passed);
    }

    #[test]
    fn ignore_file_does_not_affect_unmatched() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let a = root.join("a");
        let b = root.join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();

        fs::write(a.join("file.txt"), "hello").unwrap();
        fs::write(b.join("file.txt"), "world").unwrap();

        // Ignore pattern doesn't match file.txt
        let ignore = build_ignore(&["*.log"]);
        let result = compare_trees(
            &a,
            &b,
            &[],
            &[],
            &BTreeMap::new(),
            ".missouri",
            &ignore,
            &crate::executor::BareBackend,
        );
        assert!(!result.passed);
    }
}
