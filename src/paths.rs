use std::collections::HashSet;

use crate::graph::{StateGraph, StateId};

/// An ordered sequence of transition indices forming a simple path through the graph.
#[derive(Debug, Clone)]
pub struct TestPath {
    /// The starting state.
    pub start: StateId,
    /// Indices into `StateGraph::transitions`, in traversal order.
    pub steps: Vec<usize>,
}

impl TestPath {
    /// Human-readable representation: state names joined by arrows.
    pub fn display(&self, graph: &StateGraph) -> String {
        if self.steps.is_empty() {
            return graph.states[self.start.0].name.clone();
        }
        let mut parts = vec![graph.states[self.start.0].name.clone()];
        for &step in &self.steps {
            let t = &graph.transitions[step];
            parts.push(graph.states[t.target.0].name.clone());
        }
        parts.join(" → ")
    }
}

/// Enumerate all simple paths (no repeated states) starting from each root.
///
/// A "simple path" visits each state at most once. This naturally handles
/// cycles by refusing to revisit a state already on the current path.
pub fn enumerate_paths(graph: &StateGraph) -> Vec<TestPath> {
    let roots = graph.roots();
    let mut all_paths = Vec::new();

    for root in roots {
        let mut visited = HashSet::new();
        visited.insert(root);
        let mut current_path = Vec::new();
        dfs(graph, root, &mut visited, &mut current_path, &mut all_paths);
    }

    all_paths
}

fn dfs(
    graph: &StateGraph,
    current: StateId,
    visited: &mut HashSet<StateId>,
    current_path: &mut Vec<usize>,
    results: &mut Vec<TestPath>,
) {
    let outgoing = graph.outgoing(current);

    if outgoing.is_empty() {
        // Terminal state — emit the path if it has any transitions.
        if !current_path.is_empty() {
            results.push(TestPath {
                start: find_start(graph, current_path),
                steps: current_path.clone(),
            });
        }
        return;
    }

    // Track whether we found any unvisited successor.
    let mut any_extended = false;

    for &t_idx in outgoing {
        let target = graph.transitions[t_idx].target;
        if visited.contains(&target) {
            continue;
        }

        any_extended = true;
        visited.insert(target);
        current_path.push(t_idx);

        dfs(graph, target, visited, current_path, results);

        current_path.pop();
        visited.remove(&target);
    }

    // If all successors were already visited (cycle), emit the path ending here.
    if !any_extended && !current_path.is_empty() {
        results.push(TestPath {
            start: find_start(graph, current_path),
            steps: current_path.clone(),
        });
    }
}

fn find_start(graph: &StateGraph, steps: &[usize]) -> StateId {
    graph.transitions[steps[0]].source
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::StateGraph;
    use camino::Utf8Path;
    use std::fs;

    fn make_state(tmp: &Utf8Path, name: &str, yaml: &str) {
        let state_dir = tmp.join(name);
        let missouri_dir = state_dir.join(".missouri");
        fs::create_dir_all(&missouri_dir).unwrap();
        fs::write(missouri_dir.join("missouri.yml"), yaml).unwrap();
    }

    #[test]
    fn linear_single_path() {
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
        make_state(
            root,
            "b",
            r#"
transitions:
  - command: "echo"
    target: "../c"
"#,
        );
        make_state(root, "c", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        let paths = enumerate_paths(&graph);

        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].steps.len(), 2);
    }

    #[test]
    fn branching_two_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        make_state(
            root,
            "start",
            r#"
transitions:
  - command: "echo left"
    target: "../left"
  - command: "echo right"
    target: "../right"
"#,
        );
        make_state(root, "left", "{}");
        make_state(root, "right", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        let paths = enumerate_paths(&graph);

        assert_eq!(paths.len(), 2);
        // Each path has one step
        assert!(paths.iter().all(|p| p.steps.len() == 1));
    }

    #[test]
    fn diamond_two_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        // A → B, A → C, B → D, C → D
        make_state(
            root,
            "a",
            r#"
transitions:
  - command: "echo"
    target: "../b"
  - command: "echo"
    target: "../c"
"#,
        );
        make_state(
            root,
            "b",
            r#"
transitions:
  - command: "echo"
    target: "../d"
"#,
        );
        make_state(
            root,
            "c",
            r#"
transitions:
  - command: "echo"
    target: "../d"
"#,
        );
        make_state(root, "d", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        let paths = enumerate_paths(&graph);

        // A→B→D and A→C→D
        assert_eq!(paths.len(), 2);
        assert!(paths.iter().all(|p| p.steps.len() == 2));
    }

    #[test]
    fn cycle_no_roots_no_paths() {
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
        make_state(
            root,
            "b",
            r#"
transitions:
  - command: "echo"
    target: "../a"
"#,
        );

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        let paths = enumerate_paths(&graph);

        // No roots, so no paths enumerated
        assert!(paths.is_empty());
    }

    #[test]
    fn path_display() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        make_state(
            root,
            "start",
            r#"
transitions:
  - command: "echo"
    target: "../end"
"#,
        );
        make_state(root, "end", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        let paths = enumerate_paths(&graph);
        let display = paths[0].display(&graph);

        assert!(display.contains("start"));
        assert!(display.contains("end"));
        assert!(display.contains("→"));
    }
}
