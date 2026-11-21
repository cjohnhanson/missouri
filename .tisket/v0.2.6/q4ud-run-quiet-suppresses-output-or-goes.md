---
title: '`run --quiet` suppresses output, or goes'
status: todo
priority: null
assignee: null
due_date: null
labels:
- cli
depends_on: []
created: 2026-11-21T00:00:00Z
updated: 2026-11-21T00:00:00Z
---

## Problem

The CLI parses `RunArgs::quiet` in `src/cli.rs`, but nothing reads the value. `missouri run -q` prints exactly what `missouri run` prints. The help says the flag has no effect.

## Change

Make `--quiet` (`-q`) print only failing paths, their details, and the summary line. Omit progress lines and passing `PASS` lines. Keep the flag because removing it would break scripts that pass it.

## Acceptance

1. On a passing suite, `missouri run -q` prints only the summary line.
2. On a failing suite, it prints the failing paths with their details and the summary, without progress lines or passing `PASS` lines.
3. The exit status is the same with or without `-q`.
4. The help describes the output that `--quiet` keeps and omits.
5. Tests cover passing and failing suites.
