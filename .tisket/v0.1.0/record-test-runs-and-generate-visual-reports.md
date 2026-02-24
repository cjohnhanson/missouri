---
title: "Record test runs and generate reports"
status: done
assignee:
labels: [feature, recording, reporting]
depends_on: []
created: "2026-02-22T21:04:02Z"
updated: "2026-02-23T00:00:00Z"
---

## Summary

`missouri run --record` captures transition stdout/stderr during test execution
and writes asciicast v2 `.cast` files. Reports render the output as static code
blocks in shelldoc format (`$ command` followed by output).

## CLI Surface

```
missouri run --record                  # run tests, produce .cast per step
missouri run --record --run-id test1   # fixed run ID (for testing)
missouri report                        # terminal summary from latest run
missouri report --format html          # self-contained HTML with code blocks
missouri report --format md            # markdown with code blocks
missouri report --run <run-id>         # report on a specific run
missouri serve                         # placeholder — verifies runs exist
```

`--record` conflicts with `--check-only` (check-only skips transitions — nothing
to record). `--record` works with `--no-check` (record transitions, skip assertions).

## Recording behavior

- **Only transitions are recorded.** Setup commands and assertions are not recorded.
- **stderr is merged into stdout** in the recording stream. Asciicast v2 uses a single
  `"o"` event type for combined output.
- **Terminal dimensions** are fixed at 80x24. Commands run in subprocesses without a
  PTY, so there's no real terminal to query.
- **Flox sandbox**: all output (including flox activate warnings) is captured as-is.
- **Failing mid-path**: steps that ran get recordings. The failed step gets a recording
  showing the failure output. Subsequent steps don't run, so no recording files exist.
- **Replay timing**: lines are spread across the recording duration at minimum 150ms
  per line, with a floor of 3 seconds total. This is for the `.cast` file only — reports
  show static output.

## Recording output structure

```
<project>/
  .missouri/
    runs/
      <run-id>/             # timestamp default, --run-id override
        results.json
        path-0/
          step-0.cast
          step-1.cast
        path-1/
          step-0.cast
```

## Report formats

- **Terminal**: pass/fail summary with path and step names
- **HTML**: self-contained HTML page with code blocks showing `$ command` + output.
  ANSI escape sequences are stripped. No JavaScript, no external resources.
- **Markdown**: fenced code blocks with `$ command` + output

## Dependencies added

| Crate | Purpose |
|-------|---------|
| `serde_json` | .cast and results.json serialization |
| `chrono` | timestamp generation for run IDs and cast headers |

## What was NOT shipped

The original tisket spec called for GIF rendering via vt100/fontdue/tiny-skia/gif
and an asciinema-player JS embed. Both approaches were prototyped and rejected.
The final implementation uses static code blocks — simpler, zero JS, no font
embedding, no rendering pipeline.
