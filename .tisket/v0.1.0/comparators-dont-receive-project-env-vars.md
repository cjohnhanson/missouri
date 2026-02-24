---
title: "Comparators don't receive project env vars"
status: done
priority:
assignee:
labels: [bug]
depends_on: []
created: "2026-02-23T03:00:34Z"
updated: "2026-02-23T03:00:34Z"
---

Comparators run with only PATH set (built from bin_dirs + hardcoded fallback).
They don't receive the project env vars from missouri.yml, unlike assertions,
transitions, and setup commands which all get the full state_env/project_env.

This means tools like jq that are on a custom PATH configured in missouri.yml
env work in assertions but fail silently in comparators.

The fix: pass state_env (or at minimum the project env PATH) through to
`run_comparator` in `compare.rs`, matching the pattern used by
`run_single_assertion`.

Affected code: `compare.rs` `run_comparator()` and its call sites in
`executor.rs` (`execute_transition` around line 1033).
