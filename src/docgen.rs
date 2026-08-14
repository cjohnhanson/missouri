//! Documentation generation from missouri test suites.
//!
//! Renders a test path as a markdown tutorial or as a JSON document. The
//! output mixes the state prose, the file trees, the transition prose, the
//! commands, and the expected output.

use camino::Utf8Path;
use ignore::gitignore::Gitignore;
use serde_json::{json, Value};

use crate::graph::StateGraph;
use crate::paths::TestPath;

/// Render a test path as a markdown document.
///
/// The output interleaves:
/// 1. State prose (from `doc:` field)
/// 2. File tree of the state directory (excluding `.missouri/` and ignored files)
/// 3. Transition prose
/// 4. Command in a console code block
/// 5. Expected stdout (if present)
///
/// Returns an empty string for paths with no steps.
pub fn render_markdown(graph: &StateGraph, path: &TestPath) -> String {
    if path.steps.is_empty() {
        return String::new();
    }

    let mut out = String::new();

    // Walk through each step: state + transition
    for &step_idx in &path.steps {
        let t = &graph.transitions[step_idx];
        let state = &graph.states[t.source.0];

        // State section
        out.push_str(&format!("## {}\n\n", state.name));

        if let Some(doc) = &state.doc {
            out.push_str(doc.trim_end());
            out.push_str("\n\n");
        }

        let files = walk_state_files(&state.path, &graph.config_dir, &graph.ignore);
        if !files.is_empty() {
            out.push_str("```\n");
            for f in &files {
                out.push_str(f);
                out.push('\n');
            }
            out.push_str("```\n\n");
        }

        // Transition section
        if let Some(doc) = &t.doc {
            out.push_str(doc.trim_end());
            out.push_str("\n\n");
        }

        out.push_str("```console\n");
        out.push_str(&format!("$ {}\n", t.command));
        out.push_str("```\n");

        if let Some(stdout) = &t.expected_stdout {
            if !stdout.is_empty() {
                out.push('\n');
                out.push_str("```\n");
                out.push_str(stdout);
                out.push_str("```\n");
            }
        }

        out.push('\n');
    }

    // Final state
    let last_t = &graph.transitions[*path.steps.last().unwrap()];
    let final_state = &graph.states[last_t.target.0];

    out.push_str(&format!("## {}\n\n", final_state.name));

    if let Some(doc) = &final_state.doc {
        out.push_str(doc.trim_end());
        out.push_str("\n\n");
    }

    let files = walk_state_files(&final_state.path, &graph.config_dir, &graph.ignore);
    if !files.is_empty() {
        out.push_str("```\n");
        for f in &files {
            out.push_str(f);
            out.push('\n');
        }
        out.push_str("```\n\n");
    }

    out
}

/// Render a test path as a JSON document.
///
/// Returns a JSON object with a `steps` array. Each step has:
/// - `state`: `{ name, doc, files }`
/// - `transition`: `{ name, command, doc, stdout }`
pub fn render_json(graph: &StateGraph, path: &TestPath) -> Value {
    if path.steps.is_empty() {
        return json!({ "steps": [] });
    }

    let mut steps = Vec::new();

    for &step_idx in &path.steps {
        let t = &graph.transitions[step_idx];
        let state = &graph.states[t.source.0];

        let files = walk_state_files_with_content(&state.path, &graph.config_dir, &graph.ignore);

        let state_obj = json!({
            "name": state.name,
            "doc": state.doc,
            "files": files,
        });

        let transition_obj = json!({
            "name": t.name,
            "command": t.command,
            "doc": t.doc,
            "stdout": t.expected_stdout,
        });

        steps.push(json!({
            "state": state_obj,
            "transition": transition_obj,
        }));
    }

    // Include the final state (no outgoing transition)
    let last_t = &graph.transitions[*path.steps.last().unwrap()];
    let final_state = &graph.states[last_t.target.0];
    let final_files =
        walk_state_files_with_content(&final_state.path, &graph.config_dir, &graph.ignore);

    json!({
        "steps": steps,
        "final_state": {
            "name": final_state.name,
            "doc": final_state.doc,
            "files": final_files,
        }
    })
}

/// Walk a state directory and return the relative file paths, sorted. Skips
/// the config directory and every path that an ignore pattern matches.
pub fn walk_state_files(
    state_path: &Utf8Path,
    config_dir: &str,
    ignore: &Gitignore,
) -> Vec<String> {
    let mut files = Vec::new();
    collect_files(state_path, state_path, config_dir, ignore, &mut files);
    files.sort();
    files
}

/// Walk a state directory and return file entries with path and content.
pub fn walk_state_files_with_content(
    state_path: &Utf8Path,
    config_dir: &str,
    ignore: &Gitignore,
) -> Vec<Value> {
    let paths = walk_state_files(state_path, config_dir, ignore);
    paths
        .into_iter()
        .map(|rel| {
            let full = state_path.join(&rel);
            let content = std::fs::read_to_string(&full).ok();
            json!({
                "path": rel,
                "content": content,
            })
        })
        .collect()
}

fn collect_files(
    base: &Utf8Path,
    dir: &Utf8Path,
    config_dir: &str,
    ignore: &Gitignore,
    out: &mut Vec<String>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = match camino::Utf8PathBuf::try_from(entry.path()) {
            Ok(p) => p,
            Err(_) => continue,
        };

        let rel = match path.strip_prefix(base) {
            Ok(r) => r.to_owned(),
            Err(_) => continue,
        };

        // Skip the config directory (e.g., .missouri/)
        if rel.as_str() == config_dir || rel.starts_with(config_dir) {
            continue;
        }

        // Check ignore patterns against the full path
        let matched = ignore.matched_path_or_any_parents(path.as_std_path(), path.is_dir());
        if matched.is_ignore() {
            continue;
        }

        if path.is_dir() {
            collect_files(base, &path, config_dir, ignore, out);
        } else {
            out.push(rel.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use camino::Utf8Path;
    use serde_json::Value;

    use super::*;
    use crate::graph::StateGraph;
    use crate::paths::enumerate_paths;

    fn make_state(tmp: &Utf8Path, name: &str, yaml: &str) {
        let state_dir = tmp.join(name);
        let missouri_dir = state_dir.join(".missouri");
        fs::create_dir_all(&missouri_dir).unwrap();
        fs::write(missouri_dir.join("missouri.yml"), yaml).unwrap();
    }

    fn make_state_with_files(tmp: &Utf8Path, name: &str, yaml: &str, files: &[(&str, &str)]) {
        make_state(tmp, name, yaml);
        let state_dir = tmp.join(name);
        for (rel_path, content) in files {
            let full_path = state_dir.join(rel_path);
            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&full_path, content).unwrap();
        }
    }

    fn single_path_graph(tmp: &Utf8Path) -> (StateGraph, crate::paths::TestPath) {
        make_state_with_files(
            tmp,
            "start",
            r#"
doc: |
  The repository starts empty.
transitions:
  - name: "init project"
    command: "init --name myproject"
    target: "../initialized"
    doc: |
      Running init creates the project structure.
    stdout: "initialized myproject\n"
"#,
            &[("README.md", "# placeholder")],
        );
        make_state_with_files(
            tmp,
            "initialized",
            r#"
doc: |
  After init the project has a config file.
"#,
            &[("project.toml", "[project]\nname = \"myproject\"")],
        );

        let graph = StateGraph::discover(tmp, ".missouri").unwrap();
        let mut paths = enumerate_paths(&graph);
        assert_eq!(paths.len(), 1, "expected exactly one path");
        let path = paths.remove(0);
        (graph, path)
    }

    // --- render_markdown tests ---

    #[test]
    fn render_markdown_includes_state_prose() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        let (graph, path) = single_path_graph(root);

        let md = render_markdown(&graph, &path);

        assert!(
            md.contains("The repository starts empty."),
            "start state prose missing from:\n{md}"
        );
        assert!(
            md.contains("After init the project has a config file."),
            "initialized state prose missing from:\n{md}"
        );
    }

    #[test]
    fn render_markdown_includes_transition_prose() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        let (graph, path) = single_path_graph(root);

        let md = render_markdown(&graph, &path);

        assert!(
            md.contains("Running init creates the project structure."),
            "transition prose missing from:\n{md}"
        );
    }

    #[test]
    fn render_markdown_includes_command_in_code_block() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        let (graph, path) = single_path_graph(root);

        let md = render_markdown(&graph, &path);

        // Command should appear in a console/shell code block
        assert!(
            md.contains("init --name myproject"),
            "command missing from:\n{md}"
        );
        // Should be inside a fenced code block
        assert!(md.contains("```"), "no fenced code block in:\n{md}");
    }

    #[test]
    fn render_markdown_includes_expected_stdout() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        let (graph, path) = single_path_graph(root);

        let md = render_markdown(&graph, &path);

        assert!(
            md.contains("initialized myproject"),
            "expected stdout missing from:\n{md}"
        );
    }

    #[test]
    fn render_markdown_includes_file_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        let (graph, path) = single_path_graph(root);

        let md = render_markdown(&graph, &path);

        // The start state has README.md
        assert!(
            md.contains("README.md"),
            "start state file tree missing README.md from:\n{md}"
        );
        // The initialized state has project.toml
        assert!(
            md.contains("project.toml"),
            "initialized state file tree missing project.toml from:\n{md}"
        );
    }

    #[test]
    fn render_markdown_excludes_config_dir_from_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        let (graph, path) = single_path_graph(root);

        let md = render_markdown(&graph, &path);

        // .missouri/ internals should not appear in the file tree
        assert!(
            !md.contains("missouri.yml"),
            ".missouri/missouri.yml should not appear in file tree:\n{md}"
        );
    }

    #[test]
    fn render_markdown_no_doc_fields_still_works() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        make_state(
            root,
            "a",
            r#"
transitions:
  - command: "echo hello"
    target: "../b"
    stdout: "hello\n"
"#,
        );
        make_state(root, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        let paths = enumerate_paths(&graph);

        // Should not panic — just produces output without prose
        let md = render_markdown(&graph, &paths[0]);
        assert!(
            md.contains("echo hello"),
            "command should still appear:\n{md}"
        );
    }

    #[test]
    fn render_markdown_prose_before_command() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        let (graph, path) = single_path_graph(root);

        let md = render_markdown(&graph, &path);

        // State prose should appear before the command
        let prose_pos = md
            .find("The repository starts empty.")
            .expect("start prose missing");
        let cmd_pos = md.find("init --name myproject").expect("command missing");
        assert!(
            prose_pos < cmd_pos,
            "state prose should appear before command (prose at {prose_pos}, cmd at {cmd_pos})"
        );
    }

    #[test]
    fn render_markdown_ignored_files_excluded_from_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        // Set up an ignore pattern
        let missouri_dir = root.join(".missouri");
        fs::create_dir_all(&missouri_dir).unwrap();
        fs::write(missouri_dir.join("ignore"), "*.log\n").unwrap();

        make_state_with_files(
            root,
            "a",
            r#"
transitions:
  - command: "echo"
    target: "../b"
"#,
            &[("output.txt", "data"), ("debug.log", "ignored")],
        );
        make_state(root, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        let paths = enumerate_paths(&graph);

        let md = render_markdown(&graph, &paths[0]);

        assert!(
            md.contains("output.txt"),
            "output.txt should appear in tree:\n{md}"
        );
        assert!(
            !md.contains("debug.log"),
            "debug.log should be ignored:\n{md}"
        );
    }

    // --- walk_state_files tests ---

    #[test]
    fn walk_state_files_returns_relative_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let state_dir = root.join("mystate");
        let missouri_dir = state_dir.join(".missouri");
        fs::create_dir_all(&missouri_dir).unwrap();
        fs::write(missouri_dir.join("missouri.yml"), "{}").unwrap();
        fs::write(state_dir.join("foo.txt"), "content").unwrap();
        fs::write(state_dir.join("bar.md"), "# bar").unwrap();

        let (builder, _) = ignore::gitignore::GitignoreBuilder::new(root).build_global();
        let files = walk_state_files(&state_dir, ".missouri", &builder);

        assert!(
            files.contains(&"foo.txt".to_string()),
            "foo.txt missing: {files:?}"
        );
        assert!(
            files.contains(&"bar.md".to_string()),
            "bar.md missing: {files:?}"
        );
        // .missouri/ itself should not appear
        assert!(
            !files.iter().any(|f| f.starts_with(".missouri")),
            ".missouri should be excluded: {files:?}"
        );
    }

    #[test]
    fn walk_state_files_respects_ignore_patterns() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let state_dir = root.join("mystate");
        fs::create_dir_all(state_dir.join(".missouri")).unwrap();
        fs::write(state_dir.join(".missouri/missouri.yml"), "{}").unwrap();
        fs::write(state_dir.join("app.rs"), "fn main() {}").unwrap();
        fs::write(state_dir.join("app.log"), "log data").unwrap();

        let mut builder = ignore::gitignore::GitignoreBuilder::new(root);
        // Write a temp ignore file
        let ignore_file = root.join("ignore");
        fs::write(&ignore_file, "*.log\n").unwrap();
        builder.add(&ignore_file);
        let gitignore = builder.build().unwrap();

        let files = walk_state_files(&state_dir, ".missouri", &gitignore);

        assert!(
            files.contains(&"app.rs".to_string()),
            "app.rs should be present: {files:?}"
        );
        assert!(
            !files.contains(&"app.log".to_string()),
            "app.log should be ignored: {files:?}"
        );
    }

    // --- render_json tests ---

    #[test]
    fn render_json_returns_array_of_steps() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        let (graph, path) = single_path_graph(root);

        let json = render_json(&graph, &path);

        assert!(json.is_object(), "expected JSON object, got: {json}");
        let steps = json.get("steps").expect("missing 'steps' key");
        assert!(steps.is_array(), "steps should be an array");
        let steps = steps.as_array().unwrap();
        // One transition means two states: start and end
        assert!(!steps.is_empty(), "steps should not be empty");
    }

    #[test]
    fn render_json_step_has_state_and_transition() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        let (graph, path) = single_path_graph(root);

        let json = render_json(&graph, &path);
        let steps = json["steps"].as_array().unwrap();
        let first_step = &steps[0];

        assert!(
            first_step.get("state").is_some(),
            "step missing 'state': {first_step}"
        );
        assert!(
            first_step.get("transition").is_some(),
            "step missing 'transition': {first_step}"
        );
    }

    #[test]
    fn render_json_state_has_doc_and_files() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        let (graph, path) = single_path_graph(root);

        let json = render_json(&graph, &path);
        let steps = json["steps"].as_array().unwrap();
        let state = &steps[0]["state"];

        assert_eq!(
            state["name"].as_str(),
            Some("start"),
            "state name wrong: {state}"
        );
        assert!(
            state["doc"]
                .as_str()
                .unwrap_or("")
                .contains("repository starts empty"),
            "state doc missing: {state}"
        );
        assert!(
            state["files"].is_array(),
            "state files should be array: {state}"
        );
    }

    #[test]
    fn render_json_transition_has_command_and_doc() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        let (graph, path) = single_path_graph(root);

        let json = render_json(&graph, &path);
        let steps = json["steps"].as_array().unwrap();
        let transition = &steps[0]["transition"];

        assert!(
            transition["command"]
                .as_str()
                .unwrap_or("")
                .contains("init --name myproject"),
            "transition command wrong: {transition}"
        );
        assert!(
            transition["doc"]
                .as_str()
                .unwrap_or("")
                .contains("Running init creates"),
            "transition doc wrong: {transition}"
        );
    }

    #[test]
    fn render_json_null_doc_when_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        make_state(
            root,
            "a",
            r#"
transitions:
  - command: "echo"
    target: "../b"
"#,
        );
        make_state(root, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        let paths = enumerate_paths(&graph);

        let json = render_json(&graph, &paths[0]);
        let steps = json["steps"].as_array().unwrap();

        assert_eq!(
            steps[0]["state"]["doc"],
            Value::Null,
            "doc should be null when absent"
        );
        assert_eq!(
            steps[0]["transition"]["doc"],
            Value::Null,
            "transition doc should be null when absent"
        );
    }
}
