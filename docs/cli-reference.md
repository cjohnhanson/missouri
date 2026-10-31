<!-- metadata
title: missouri CLI reference
description: Every command, argument, and flag of the missouri binary, generated from its definition
type: reference
-->

# missouri CLI reference

- **Version:** 0.2.5

Show-me-state: e2e testing as directed graphs of filesystem states

Every command takes `-h` or `--help`. `-V` and `--version` work at the
top level only, as `missouri --version`; a subcommand refuses them.

- **Usage:** `missouri [OPTIONS] <COMMAND>`

## Global Flags

### `-C <DIRECTORY>`

Change to this directory before doing anything

### `--config-dir <CONFIG_DIR>`

Name of the config directory (default: .missouri)

**Default:** `.missouri`

## Flags

### `-h --help`

Print help (see a summary with '-h')

### `-V --version`

Print version

## `missouri run`

- **Usage:** `missouri run [OPTIONS]`

Run all test paths

Run all test paths discovered in the state graph.

Exit codes: `0` every path passed. `1` one path or more failed. `2` configuration error. `130` interrupted.

### Flags

#### `-d --dir <DIR>`

Root directory containing states

**Default:** `.`

#### `-v --verbose`

Increase verbosity (-v, -vv, -vvv)

Stackable: `-v`, `-vv`, `-vvv`.

**Default:** `0`

#### `-q --quiet`

Suppress non-essential output

**Default:** `false`

#### `--keep-temp`

Keep temp directories after run (for debugging)

**Default:** `false`

#### `--check-only`

Run only state assertions (skip transitions and filesystem comparison)

Run the state assertions only. Skip the transitions and the filesystem comparison. Conflicts with `--no-check` and `--record`.

**Default:** `false`

#### `--no-check`

Skip all assertions (run only transitions and filesystem comparison)

Skip all assertions. Run the transitions and the filesystem comparison only. Conflicts with `--check-only`.

**Default:** `false`

#### `--record`

Record transition output to asciicast files

Record the transition output to asciicast (`.cast`) files. Conflicts with `--check-only`.

**Default:** `false`

#### `--run-id <RUN_ID>`

Custom run ID for recording output directory (default: timestamp)

Set a custom run ID for the recording output directory. Default: a timestamp (`YYYY-MM-DDTHH-MM-SS`). Requires `--record`.

#### `-h --help`

Print help (see a summary with '-h')

## `missouri list`

- **Usage:** `missouri list [OPTIONS]`

List states, transitions, or test paths

List states, transitions, or test paths in the state graph.

`list` refuses a directory that holds no state, and names the directory. The refusal names `missouri state add`, and names `missouri init` for the case where no project exists yet.

Workspace mode refuses each member the same way, and names the member. Where a `members` entry names no directory at all, `list`, `validate` and `run` all name that entry and the key that declared it.

`list` also refuses `paths` and `graph` on a graph with no root state, because there is nothing to walk. A root state is one with no inbound transition. `states` and `transitions` still print for such a graph, since that listing is how a reader finds the loop.

### Flags

#### `-d --dir <DIR>`

Root directory containing states

**Default:** `.`

#### `--show <SHOW>`

What to list

`states`: print all discovered states.

`transitions`: print all discovered transitions.

`paths`: print all enumerated test paths (root-to-leaf walks).

`graph`: same as `paths`.

**Choices:**

- `states`
- `transitions`
- `paths`
- `graph`

**Default:** `paths`

#### `-h --help`

Print help (see a summary with '-h')

## `missouri validate`

- **Usage:** `missouri validate [OPTIONS]`

Validate missouri.yml files without running

Validate the `missouri.yml` files. This command runs nothing else. It checks that every config parses, that every transition target resolves to a real state, and that at least one root state exists.

Prints a summary: `valid: N state(s), N transition(s), N root(s)`.

### Flags

#### `-d --dir <DIR>`

Root directory containing states

**Default:** `.`

#### `-h --help`

Print help (see a summary with '-h')

## `missouri init`

- **Usage:** `missouri init [OPTIONS]`

Initialize a new missouri project

This command creates the config directory structure.

### Flags

#### `-d --dir <DIR>`

Root directory for the project

**Default:** `.`

#### `-h --help`

Print help (see a summary with '-h')

## `missouri state`

- **Usage:** `missouri state [OPTIONS] <COMMAND>`

Manage states

### Flags

#### `-h --help`

Print help

## `missouri state add`

- **Usage:** `missouri state add [OPTIONS] <NAME>`

Add a new state

### Arguments

#### `<NAME>`

Name of the new state (becomes the directory name)

### Flags

#### `-d --dir <DIR>`

Root directory containing states

**Default:** `.`

#### `--from <FROM>`

Copy from an existing state and create a placeholder transition from it to the new state

#### `-h --help`

Print help

## `missouri report`

- **Usage:** `missouri report [OPTIONS]`

Generate a report from recorded runs

### Flags

#### `-d --dir <DIR>`

Root directory containing states

**Default:** `.`

#### `--format <FORMAT>`

Report format

`terminal`: print to stdout.

`html`: write `report.html` to the run directory.

`md`: write `report.md` to the run directory.

**Choices:**

- `terminal`
- `html`
- `md`

**Default:** `terminal`

#### `--run <RUN>`

Specific run ID to report on (default: latest)

#### `-h --help`

Print help (see a summary with '-h')

## `missouri serve`

- **Usage:** `missouri serve [OPTIONS]`

Serve an HTML report locally (not implemented yet)

Not implemented yet. The command checks that a recorded run exists, prints a message on stderr, and exits 1. To read a report, run `missouri report --format html` and open the file it writes.

### Flags

#### `-d --dir <DIR>`

Root directory containing states

**Default:** `.`

#### `--run <RUN>`

Specific run ID to serve (default: latest)

The run to check for. Default: latest.

#### `--port <PORT>`

Accepted and unused until serve exists

**Default:** `8080`

#### `-h --help`

Print help (see a summary with '-h')

## `missouri docs`

- **Usage:** `missouri docs [OPTIONS] [TOPIC] [QUERY]`

Browse bundled documentation

Print a page of this documentation. Missouri compiles the pages into the binary, so the command works from any directory.

Omit `[TOPIC]` to list the topics.

### Arguments

#### `[TOPIC]`

Topic slug to show, or "search" to search the docs

#### `[QUERY]`

Search query (when topic is "search")

### Flags

#### `-a --all`

Print every page whole, in one stream

**Default:** `false`

#### `-h --help`

Print help (see a summary with '-h')

## `missouri docgen`

- **Usage:** `missouri docgen [OPTIONS]`

Generate documentation from test suites

Render one test path as a document. The output interleaves the state prose from each `doc:` field, the file tree of each state, the transition command, and the expected stdout.

### Flags

#### `-d --dir <DIR>`

Root directory containing states

**Default:** `.`

#### `--format <FORMAT>`

Output format

**Choices:**

- `markdown`
- `json`

**Default:** `markdown`

#### `--path <PATH>`

Path index to render (1-based, default: 1)

**Default:** `1`

#### `-h --help`

Print help (see a summary with '-h')

## `missouri agent`

- **Usage:** `missouri agent [OPTIONS] <COMMAND>`

Agent evaluation commands

### Flags

#### `-h --help`

Print help

## `missouri agent eval`

- **Usage:** `missouri agent eval [OPTIONS] <NAME>`

Run an agent evaluation

Run an agent evaluation. This command reads `<config_dir>/<name>.md`. It parses the YAML frontmatter as the agent configuration. It then starts a Claude agent and passes the markdown body as the evaluation prompt. The agent returns a verdict. To do so, it calls `missouri agent pass` or `missouri agent fail <details>`.

Exit codes: `0` pass, `1` fail or no verdict.

### Arguments

#### `<NAME>`

Eval name (matches `<config_dir>/<name>.md`)

### Flags

#### `-d --dir <DIR>`

Root directory containing the state

**Default:** `.`

#### `-h --help`

Print help (see a summary with '-h')

## `missouri agent pass`

- **Usage:** `missouri agent pass [OPTIONS]`

Record a passing verdict (called by the agent)

Record a passing verdict. The evaluation agent calls this command during an eval. Do not call it directly.

### Flags

#### `-h --help`

Print help (see a summary with '-h')

## `missouri agent fail`

- **Usage:** `missouri agent fail [OPTIONS] [DETAILS]...`

Record a failing verdict (called by the agent)

Record a failing verdict with details. The evaluation agent calls this command.

### Arguments

#### `[DETAILS]…`

Failure details

### Flags

#### `-h --help`

Print help (see a summary with '-h')
