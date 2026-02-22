---
title: "Record test runs and generate visual reports"
status: todo
priority: 2
assignee:
labels: [feature, recording, reporting]
depends_on: []
created: "2026-02-22T21:04:02Z"
updated: "2026-02-22T21:04:02Z"
---

## Summary

`missouri run --record` captures terminal output with timing during test execution,
renders it to GIF, and supports generating visual HTML reports that can be served
locally. Single binary — no external tools, no CDN, embedded font.

## CLI Surface

```
missouri run --record                  # run tests, produce .cast + .gif per step
missouri run --record --run-id test1   # fixed run ID (for testing)
missouri report                        # terminal output from latest run (default)
missouri report --format html          # self-contained HTML with inlined gifs
missouri report --format md            # markdown with pass/fail tables + gif paths
missouri report --run <run-id>         # report on a specific run
missouri serve                         # serve latest HTML report on localhost
```

`--record` conflicts with `--check-only` (check-only skips transitions — nothing
to record). `--record` works with `--no-check` (record transitions, skip assertions).

## Recording behavior

- **Only transitions are recorded.** Setup commands and assertions are not recorded —
  they're short validation commands, not interesting to watch.
- **stderr is merged into stdout** in the recording stream. Asciicast v2 uses a single
  `"o"` event type for combined output.
- **Terminal dimensions** are fixed at 80x24. Commands run in subprocesses without a
  PTY, so there's no real terminal to query.
- **Flox sandbox**: the inner command output is recorded, not the `flox activate`
  wrapper output.
- **Failing mid-path**: steps that ran get recordings. The failed step gets a recording
  showing the failure output. Subsequent steps don't run, so no recording files exist
  for them.

## Recording output structure

Paths and steps are indexed numerically. `results.json` maps indices to
human-readable names. No unicode or encoding issues in directory/file names.

```
<project>/
  .missouri/
    runs/
      <run-id>/             # timestamp default, --run-id override
        results.json
        path-0/
          step-0.cast
          step-0.gif
          step-1.cast
          step-1.gif
        path-1/
          step-0.cast
          step-0.gif
```

**results.json schema:**

```json
{
  "run_id": "2026-02-22T17-30-00",
  "timestamp": "2026-02-22T17:30:00Z",
  "project": "/path/to/project",
  "total": 2,
  "passed": 2,
  "failed": 0,
  "paths": [
    {
      "index": 0,
      "name": "root → left",
      "passed": true,
      "steps": [
        {
          "index": 0,
          "source": "root",
          "target": "left",
          "transition": "go left",
          "passed": true,
          "exit_code": 0,
          "cast_file": "path-0/step-0.cast",
          "gif_file": "path-0/step-0.gif"
        }
      ]
    }
  ]
}
```

## Tests (the spec)

### Illinois test: 08-dbt with --record (flox sandbox)

08-dbt has two paths:
- Path 0: `empty → uv-initialized → uv-added` (2 transitions: `uv init`, `uv add`)
- Path 1: `dbt-seeded → dbt-ran` (1 transition: `dbt run`)

The illinois scenario runs `missouri run --record --run-id test -d fixture` as
the transition command. The "after" state contains the expected output structure.

Expected recording output:

```
fixture/.missouri/runs/test/
  results.json
  path-0/
    step-0.cast
    step-0.gif
    step-1.cast
    step-1.gif
  path-1/
    step-0.cast
    step-0.gif
```

**Comparators** (in `.illinois/bin/`):

`compare-cast` — receives expected path and actual path as arguments:
- Parse actual file as NDJSON (newline-delimited JSON)
- First line: object with `version` (must be 2), `width` (must be 80),
  `height` (must be 24), `timestamp` (number)
- Remaining lines: arrays of `[float, string, string]` where:
  - Element 0: elapsed seconds (non-negative float, monotonically non-decreasing)
  - Element 1: event type (must be `"o"`)
  - Element 2: output data (non-empty string)
- Must have at least one event line
- Ignore actual timestamp values and output content (non-deterministic)
- Exit 0 if valid structure, exit 1 with diagnostic if not

`compare-gif` — receives expected path and actual path as arguments:
- First 6 bytes of actual file must be `GIF89a` or `GIF87a`
- File size must be > 100 bytes (a real frame, not just headers)
- Exit 0 if valid, exit 1 with diagnostic if not

`compare-results-json` — receives expected path and actual path as arguments:
- Parse actual file as JSON
- Must have `run_id` (string, equals "test"), `paths` (array)
- For 08-dbt: `paths` has 2 entries
  - `paths[0]`: `index` 0, `steps` array length 2, all `passed` true
  - `paths[1]`: `index` 1, `steps` array length 1, all `passed` true
- Each step must have `cast_file` and `gif_file` strings matching `path-N/step-N.*`
- `total` equals 2, `passed` equals 2, `failed` equals 0
- Exit 0 if valid, exit 1 with diagnostic if not

**Illinois config for "before" state:**

```yaml
transitions:
  - name: "missouri run --record on 08-dbt"
    command: "run-missouri-record"
    target: "../after"
    comparators:
      files:
        - path: "output.txt"
          ignore: true
        - path: "fixture/.missouri/.flox/"
          ignore: true
        - path: "fixture/.missouri/runs/test/results.json"
          command: "compare-results-json"
        - path: "fixture/.missouri/runs/test/path-0/step-0.cast"
          command: "compare-cast"
        - path: "fixture/.missouri/runs/test/path-0/step-0.gif"
          command: "compare-gif"
        - path: "fixture/.missouri/runs/test/path-0/step-1.cast"
          command: "compare-cast"
        - path: "fixture/.missouri/runs/test/path-0/step-1.gif"
          command: "compare-gif"
        - path: "fixture/.missouri/runs/test/path-1/step-0.cast"
          command: "compare-cast"
        - path: "fixture/.missouri/runs/test/path-1/step-0.gif"
          command: "compare-gif"
```

The "after" state contains placeholder files at each of these paths (empty files
or minimal stubs). The comparators validate the actual content against structural
rules, not byte-for-byte equality.

### Illinois test: 03-branching with --record (no sandbox)

03-branching has two paths:
- Path 0: `root → left` (1 transition: `go left`)
- Path 1: `root → right` (1 transition: `go right`)

Expected recording output:

```
fixture/.missouri/runs/test/
  results.json
  path-0/
    step-0.cast
    step-0.gif
  path-1/
    step-0.cast
    step-0.gif
```

**Illinois config for "before" state:**

```yaml
transitions:
  - name: "missouri run --record on 03-branching"
    command: "run-missouri-record"
    target: "../after"
    comparators:
      files:
        - path: "output.txt"
          ignore: true
        - path: "fixture/.missouri/runs/test/results.json"
          command: "compare-results-json"
        - path: "fixture/.missouri/runs/test/path-0/step-0.cast"
          command: "compare-cast"
        - path: "fixture/.missouri/runs/test/path-0/step-0.gif"
          command: "compare-gif"
        - path: "fixture/.missouri/runs/test/path-1/step-0.cast"
          command: "compare-cast"
        - path: "fixture/.missouri/runs/test/path-1/step-0.gif"
          command: "compare-gif"
```

`compare-results-json` for this fixture validates:
- `paths` has 2 entries, each with 1 step
- All `passed` true, `total` 2, `passed` 2, `failed` 0
- `run_id` equals "test"

### Illinois test: 15-fail-mid-path with --record (partial recording on failure)

New fixture `tests/fixtures/15-fail-mid-path` — a 4-state linear path where the
second of three transitions fails:

```
state-a → state-b → state-c → state-d
          (ok)       (fails)   (never runs)
```

- `state-a → state-b`: `echo "step one succeeds"` (exit 0)
- `state-b → state-c`: `sh -c "echo 'step two failing' && exit 1"` (exit 1)
- `state-c → state-d`: `echo "step three never runs"` (would exit 0, but never executes)

Each state has a `data.txt` so filesystem comparison has something to look at.
`state-c/data.txt` differs from `state-b/data.txt` so the transition would fail
on comparison too, but the command failure happens first.

The illinois scenario runs `missouri run --record --run-id test -d fixture`.
Missouri exits with code 1 (test failure). The transition script captures this:

```sh
#!/bin/sh
"$MISSOURI_BIN" run --record --run-id test -d fixture > output.txt 2>&1
echo $? > exit_code.txt
```

The "after" state has `exit_code.txt` containing `1`.

Expected recording output:

```
fixture/.missouri/runs/test/
  results.json
  path-0/
    step-0.cast          # exists — transition ran, succeeded
    step-0.gif           # exists
    step-1.cast          # exists — transition ran, failed
    step-1.gif           # exists
                         # NO step-2.cast or step-2.gif — never ran
```

**Illinois config for "before" state:**

```yaml
transitions:
  - name: "missouri run --record on 15-fail-mid-path"
    command: "run-missouri-record"
    target: "../after"
    comparators:
      files:
        - path: "output.txt"
          ignore: true
        - path: "fixture/"
          ignore: true
        - path: "fixture/.missouri/runs/test/results.json"
          command: "compare-results-json-fail"
        - path: "fixture/.missouri/runs/test/path-0/step-0.cast"
          command: "compare-cast"
        - path: "fixture/.missouri/runs/test/path-0/step-0.gif"
          command: "compare-gif"
        - path: "fixture/.missouri/runs/test/path-0/step-1.cast"
          command: "compare-cast"
        - path: "fixture/.missouri/runs/test/path-0/step-1.gif"
          command: "compare-gif"
```

No comparator entries for `step-2.*` — those files must not exist. The "after"
state also has no placeholder files for them, so if missouri incorrectly creates
them, the comparison will detect extra files and fail.

`compare-results-json-fail` validates:
- `run_id` equals "test"
- `total` 1, `passed` 0, `failed` 1
- `paths[0].passed` is false
- `paths[0].steps` has length 2 (only steps that ran)
- `paths[0].steps[0].passed` true, `exit_code` 0
- `paths[0].steps[1].passed` false, `exit_code` 1
- Both steps have `cast_file` and `gif_file` strings

This test confirms three things:
1. Successful steps produce recordings even when the path fails
2. The failed step itself produces a recording (its output was captured)
3. Steps after the failure produce NO recording files

### CLI tests (`tests/cli.rs`)

**Recording:**

`record_produces_output_directory`:
- Run `missouri run --record --run-id clitest -d <03-branching>`
- Assert `.missouri/runs/clitest/` directory exists
- Assert `results.json` exists inside it
- Assert `path-0/step-0.cast` exists
- Assert `path-0/step-0.gif` exists

`record_cast_files_per_step`:
- Run `missouri run --record --run-id clitest -d <03-branching>`
- Assert exactly 2 `.cast` files total (one per transition across both paths)
- Assert exactly 2 `.gif` files total

`record_does_not_break_pass_fail`:
- Run `missouri run --record -d <03-branching>`
- Assert exit code 0
- Assert stdout contains "PASS"
- Assert stdout contains "2 passed"

`record_with_failing_fixture`:
- Create a temp fixture with 2 transitions where the second fails
- Run `missouri run --record --run-id clitest -d <fixture>`
- Assert exit code 1
- Assert `path-0/step-0.cast` and `path-0/step-0.gif` exist (step that ran)
- Assert `path-0/step-1.cast` and `path-0/step-1.gif` exist (step that failed — it ran, it just failed)
- If there's a step-2, assert it does NOT exist (never ran)
- `results.json` exists and shows the failure

`record_run_id_flag`:
- Run `missouri run --record --run-id my-custom-id -d <fixture>`
- Assert `.missouri/runs/my-custom-id/` exists (not a timestamp)

`record_default_run_id_is_timestamp`:
- Run `missouri run --record -d <fixture>`
- Assert `.missouri/runs/` contains exactly one subdirectory
- Assert directory name matches `\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}` pattern

`record_conflicts_with_check_only`:
- Run `missouri run --record --check-only -d <fixture>`
- Assert failure (clap conflict)

`record_works_with_no_check`:
- Run `missouri run --record --no-check -d <03-branching>`
- Assert exit code 0
- Assert .cast and .gif files produced

**Reporting:**

`report_terminal_default`:
- Run `missouri run --record --run-id r1 -d <fixture>`
- Run `missouri report -d <fixture>`
- Assert stdout contains pass/fail summary
- Assert stdout contains path names

`report_html_generates_file`:
- Run `missouri run --record --run-id r1 -d <fixture>`
- Run `missouri report --format html -d <fixture>`
- Assert a `.html` file is produced
- Assert file contains `<!DOCTYPE html>`
- Assert file contains `<img` tags
- Assert file contains path names from the test run
- Assert file contains "PASS" or "FAIL"

`report_html_is_self_contained`:
- Same setup
- Assert HTML contains `data:image/gif;base64,` (inlined gifs)
- Assert no external `<script src=` or `<link href=` tags

`report_md_generates_file`:
- Run `missouri run --record --run-id r1 -d <fixture>`
- Run `missouri report --format md -d <fixture>`
- Assert `.md` file produced
- Assert contains `![` (image references)
- Assert contains path names and pass/fail indicators

`report_specific_run`:
- Run `missouri run --record --run-id run-a -d <fixture>`
- Run `missouri run --record --run-id run-b -d <fixture>`
- Run `missouri report --run run-a -d <fixture>`
- Assert report references run-a, not run-b

`report_no_runs_errors`:
- Run `missouri report -d <fixture>` with no prior `--record` run
- Assert failure with "no recorded runs found" or similar

**Serving:**

`serve_starts_and_responds`:
- Run `missouri run --record --run-id r1 -d <fixture>`
- Start `missouri serve -d <fixture>` in background
- HTTP GET `http://localhost:<port>/` returns 200 with HTML content
- Kill server process
- Port printed to stdout on startup

`serve_no_runs_errors`:
- Run `missouri serve -d <fixture>` with no prior `--record` run
- Assert failure with meaningful error

### Unit tests

**Recorder (src/recorder.rs):**

`cast_header_format`:
- Create a CastHeader with width=80, height=24
- Serialize to JSON
- Assert contains `"version":2`, `"width":80`, `"height":24`, `"timestamp":`

`cast_event_format`:
- Create a CastEvent with time=1.5, event_type="o", data="hello\r\n"
- Serialize to JSON
- Assert output is `[1.5,"o","hello\\r\\n"]`

`cast_events_monotonic`:
- Record a sequence of events
- Assert each event's timestamp >= previous event's timestamp

**GIF renderer (src/renderer.rs):**

`render_empty_screen`:
- Create an 80x24 terminal screen (blank)
- Render to pixel buffer
- Assert dimensions are correct (width * cell_width, height * cell_height)
- Assert all pixels are background color

`render_text_produces_nonzero_pixels`:
- Create a screen with "Hello, world!" at row 0
- Render to pixel buffer
- Assert some pixels in the text region differ from background color

`render_colors`:
- Create a screen with colored text (red fg, blue bg)
- Render to pixel buffer
- Assert text region pixels contain red channel > 0
- Assert background region pixels contain blue channel > 0

`cast_to_gif_produces_valid_output`:
- Create a minimal .cast (header + 2-3 events)
- Run full pipeline: parse → emulate → render → encode
- Assert output starts with `GIF89a`
- Assert output > 100 bytes

`cast_to_gif_multiple_frames`:
- Create a .cast with events spread across time (0.0s, 1.0s, 2.0s)
- Run full pipeline
- Assert GIF contains multiple frames

**Report generation:**

`results_json_roundtrip`:
- Create PathResult structs
- Serialize to results.json format
- Deserialize back
- Assert all fields preserved

`html_report_structure`:
- Generate HTML from results + recording paths
- Assert contains DOCTYPE, head, body
- Assert one section per test path
- Assert img tags with data URIs

`md_report_structure`:
- Generate markdown from results + recording paths
- Assert contains pass/fail table/list
- Assert contains image links to gif files

## Implementation notes

### Recording capture (src/recorder.rs)

Modify command execution in `executor.rs` to support timestamped chunked reads
instead of collecting all stdout at once. Stderr merged into stdout stream.
Write asciicast v2 format:

```
{"version": 2, "width": 80, "height": 24, "timestamp": 1234567890}
[0.5, "o", "hello world\r\n"]
[1.2, "o", "done\r\n"]
```

Pure Rust, `serde_json` for serialization.

### GIF rendering pipeline (src/renderer.rs)

```
.cast → vt100 (terminal emulation) → fontdue (glyph raster) → tiny-skia (composite) → gif (encode)
```

All pure Rust, permissive licenses:
- `vt100` — feed bytes, get screen grid with colors/attributes
- `fontdue` — rasterize individual glyphs from embedded font
- `tiny-skia` — composite glyph bitmaps onto pixel buffer with fg/bg colors
- `gif` — encode frames to GIF89a

Embed JetBrains Mono Regular (~264KB, OFL-1.1) via `include_bytes!()`.

### Report generation (extend src/report.rs)

- `--format html`: self-contained HTML file, gifs inlined as base64 data URIs,
  pass/fail table, one section per test path
- `--format md`: markdown with pass/fail tables, relative paths to .gif files

### Local server (src/serve.rs)

Minimal HTTP server (`tiny_http` or raw `TcpListener`) serving the HTML report
on localhost. Opens browser automatically or prints URL.

## New dependencies

| Crate | Purpose | License |
|-------|---------|---------|
| `vt100` | terminal emulation | MIT |
| `fontdue` | font rasterization | MIT/Apache/Zlib |
| `tiny-skia` | 2D compositing | BSD-3 |
| `gif` | GIF encoding | MIT |
| `serde_json` | .cast serialization | MIT/Apache |
| `tiny_http` | local report server | MIT/Apache |

Binary size impact: ~300-400KB for embedded font + crate code.
