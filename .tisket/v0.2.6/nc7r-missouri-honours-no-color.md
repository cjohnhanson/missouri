---
title: missouri honours `NO_COLOR`
status: todo
priority: null
assignee: null
due_date: null
labels:
- output
depends_on: []
created: 2026-11-21T00:00:00Z
updated: 2026-11-21T00:00:00Z
---

## Problem

`src/report.rs` unconditionally prints ANSI colour codes around `✓`, `✗`, `PASS` and `FAIL`. It uses `ESC[32m` for green and `ESC[31m` for red, even with `NO_COLOR=1` or when stdout or stderr is a pipe or file. The [NO_COLOR convention](https://no-color.org) asks programs to omit colour when `NO_COLOR` is set and non-empty. Error reports from miette use a separate output path.

## Change

Omit colour codes when `NO_COLOR` is set and non-empty, or when the output stream is not a terminal. Keep colour on a terminal otherwise. This changes output only for readers who request no colour or redirect a stream; terminal colour remains available. Ship the change with a version bump. Remove the colour stripping from `tests/docs.rs` once the output needs none.

## Acceptance

1. With `NO_COLOR=1`, `missouri run` prints no ESC byte on stdout or stderr.
2. Output sent to a pipe or file contains no ESC byte, whether it comes from stdout or stderr.
3. Terminal output retains colour when `NO_COLOR` is unset or empty.
4. Tests cover each case, including passing and failing output.
5. `tests/docs.rs` passes without stripping colour codes.
6. The release includes a version bump.
