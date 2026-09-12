<!-- metadata
title: "missouri CLI Reference"
description: "Complete command reference for the missouri test framework"
type: reference
-->

# missouri CLI Reference

```
missouri [OPTIONS] <COMMAND>
```

End-to-end testing as directed graphs of filesystem states.

## Global options

| Flag | Description |
|------|-------------|
| `-C <DIR>` | Change to this directory before doing anything. |
| `--config-dir <NAME>` | Name of the config directory. Default: `.missouri`. |
| `--version` | Print version. |
| `-h, --help` | Print help. |

## Commands

### `missouri run`

Run all test paths discovered in the state graph.

```
missouri run [OPTIONS]
```

| Flag | Description |
|------|-------------|
| `-d, --dir <DIR>` | Root directory containing states. Default: `.` |
| `-v, --verbose` | Increase verbosity. Stackable: `-v`, `-vv`, `-vvv`. |
| `-q, --quiet` | Suppress non-essential output. |
| `--keep-temp` | Keep temp directories after the run (for debugging). |
| `--check-only` | Run the state assertions only. Skip the transitions and the filesystem comparison. Conflicts with `--no-check` and `--record`. |
| `--no-check` | Skip all assertions. Run the transitions and the filesystem comparison only. Conflicts with `--check-only`. |
| `--record` | Record the transition output to asciicast (`.cast`) files. Conflicts with `--check-only`. |
| `--run-id <ID>` | Set a custom run ID for the recording output directory. Default: a timestamp (`YYYY-MM-DDTHH-MM-SS`). Requires `--record`. |

Exit codes: `0` every path passed. `1` one path or more failed. `2` configuration error. `130` interrupted.

### `missouri list`

List states, transitions, or test paths in the state graph.

```
missouri list [OPTIONS]
```

| Flag | Description |
|------|-------------|
| `-d, --dir <DIR>` | Root directory containing states. Default: `.` |
| `--show <KIND>` | What to list. Default: `paths`. |

`--show` accepts:

| Value | Description |
|-------|-------------|
| `states` | Print all discovered states. |
| `transitions` | Print all discovered transitions. |
| `paths` | Print all enumerated test paths (root-to-leaf walks). |
| `graph` | Same as `paths`. |

### `missouri validate`

Validate the `missouri.yml` files. This command runs nothing else. It checks that every config parses, that every transition target resolves to a real state, and that at least one root state exists.

```
missouri validate [OPTIONS]
```

| Flag | Description |
|------|-------------|
| `-d, --dir <DIR>` | Root directory containing states. Default: `.` |

Prints a summary: `valid: N state(s), N transition(s), N root(s)`.

### `missouri init`

Initialize a new missouri project. This command creates the config directory structure.

```
missouri init [OPTIONS]
```

| Flag | Description |
|------|-------------|
| `-d, --dir <DIR>` | Root directory for the project. Default: `.` |

### `missouri state add`

Add a new state to the project.

```
missouri state add <NAME> [OPTIONS]
```

| Argument / Flag | Description |
|-----------------|-------------|
| `<NAME>` | Name of the new state (becomes the directory name). |
| `-d, --dir <DIR>` | Root directory containing states. Default: `.` |
| `--from <STATE>` | Copy from an existing state and create a placeholder transition from it to the new state. |

### `missouri report`

Generate a report from recorded runs.

```
missouri report [OPTIONS]
```

| Flag | Description |
|------|-------------|
| `-d, --dir <DIR>` | Root directory containing states. Default: `.` |
| `--format <FMT>` | Report format. Default: `terminal`. |
| `--run <ID>` | Specific run ID to report on. Default: latest. |

`--format` accepts:

| Value | Output |
|-------|--------|
| `terminal` | Print to stdout. |
| `html` | Write `report.html` to the run directory. |
| `md` | Write `report.md` to the run directory. |

### `missouri agent eval`

Run an agent evaluation. This command reads `<config_dir>/<name>.md`. It
parses the YAML frontmatter as the agent configuration. It then starts a
Claude agent and passes the markdown body as the evaluation prompt. The
agent returns a verdict. To do so, it calls `missouri agent pass` or
`missouri agent fail <details>`.

```
missouri agent eval <NAME> [OPTIONS]
```

| Argument / Flag | Description |
|-----------------|-------------|
| `<NAME>` | Eval name (matches `<config_dir>/<name>.md`). |
| `-d, --dir <DIR>` | Root directory containing the state. Default: `.` |

Exit codes: `0` pass, `1` fail or no verdict.

### `missouri agent pass`

Record a passing verdict. The evaluation agent calls this command during
an eval. Do not call it directly.

### `missouri agent fail`

Record a failing verdict with details. The evaluation agent calls this
command.

```
missouri agent fail [DETAILS...]
```

### `missouri docs`

Print a page of this documentation. Missouri compiles the pages into the
binary, so the command works from any directory.

```
missouri docs [OPTIONS] [TOPIC] [QUERY]
```

| Argument / Flag | Description |
|-----------------|-------------|
| `[TOPIC]` | The topic slug to show, or `search` to search the pages. Omit it to list the topics. |
| `[QUERY]` | The search query. Use it when the topic is `search`. |
| `-a, --all` | Print every page whole, in one stream. |

### `missouri docgen`

Render one test path as a document. The output interleaves the state
prose from each `doc:` field, the file tree of each state, the transition
command, and the expected stdout.

```
missouri docgen [OPTIONS]
```

| Flag | Description |
|------|-------------|
| `-d, --dir <DIR>` | Root directory containing states. Default: `.` |
| `--format <FMT>` | `markdown` or `json`. Default: `markdown`. |
| `--path <N>` | Which test path to render, counting from 1. Default: `1`. |

### `missouri serve`

Not implemented yet. The command checks that a recorded run exists,
prints a message on stderr, and exits 1. To read a report, run
`missouri report --format html` and open the file it writes.

```
missouri serve [OPTIONS]
```

| Flag | Description |
|------|-------------|
| `-d, --dir <DIR>` | Root directory containing states. Default: `.` |
| `--run <ID>` | The run to check for. Default: latest. |
| `--port <PORT>` | Accepted and unused. |

---

## Configuration reference

Missouri uses YAML config files named `missouri.yml`. There are two levels. The project-level config has one file per project root. The state-level config has one file per state directory.

### Config file locations

Missouri loads the project-level config from the first file it finds, in this order:

1. `<root>/missouri.yml`, the root-level config. It can include `test_dir`.
2. `<root>/<config_dir>/missouri.yml`, the config-dir-level config.

The root-level file wins when both files exist.

The state-level config lives at `<state_dir>/<config_dir>/missouri.yml`.

### Project-level `missouri.yml`

```yaml
# The directory that holds the test states, relative to this config file.
# When set, state discovery starts here instead of at the project root.
test_dir: tests/smoke

# Environment variables that every state inherits.
# A state-level variable of the same name overrides the value here.
env:
  RUST_BACKTRACE: "1"
  APP_ENV: test

# Commands that run in order before any test path.
# Execution stops at the first failure.
setup:
  - name: "build project"        # optional label
    command: "cargo build --release"
    shell: true                   # default: true
  - command: "db-seed"
    shell: false

# Nix packages to provide through `nix shell`.
# When this list is not empty, every command runs inside a nix shell
# with these packages.
packages:
  - python3
  - uv
  - git

# Workspace mode: a list of member directories.
# When set, `missouri run` runs each member on its own.
members:
  - my-tool/tests/missouri
  - other-tool/tests/missouri
```

#### Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `test_dir` | string | (none) | Start state discovery in this subdirectory. |
| `env` | map<string, string> | `{}` | Project-level environment variables. |
| `setup` | list of [SetupCommand](#setupcommand) | `[]` | Commands to run before the test paths. |
| `packages` | list of string | `[]` | Nixpkgs packages for the sandbox. |
| `members` | list of string | `[]` | Workspace member directories. |
| `docker` | bool | `false` | Run every transition inside a Docker container with no network access. It overrides `packages`. An assertion, a comparator, and a service run on the host, so missouri refuses each under `docker: true`. |
| `docker_image` | string | `debian:bookworm-slim` | The image the containers run. Requires `docker: true`. |

#### SetupCommand

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | string | auto-generated (`setup[N]`) | A label for the output. |
| `command` | string | **required** | The command to run. |
| `shell` | bool | `true` | Run the command through `sh -c`. When false, missouri splits the command on whitespace and runs it directly. |

### State-level `missouri.yml`

```yaml
# Environment variables for this state.
# Missouri merges these over the project-level env.
# A state variable of the same name wins.
env:
  DB_URL: "postgres://localhost/test"

# Transitions out of this state.
transitions:
  - name: "build"                 # optional label
    command: "make build"
    target: "../built"            # relative path to the target state directory
    shell: true                   # default: true
    stdout: "expected stdout\n"   # optional exact-match assertion
    stderr: ""                    # optional exact-match assertion

    # Background services (see Services section)
    services:
      - command: "my-server --port 0"

    # Network interception (see Network section)
    network:
      replay: recordings/worker.flow

    # Comparison overrides (see Comparators section)
    comparators:
      files:
        - path: "dist/manifest.json"
          command: "compare-json"
        - path: "logs/"
          ignore: true
      env:
        - name: BUILD_TIMESTAMP
          ignore: true
      network:
        - path: "api.example.com/v1/*"
          command: "compare-api"
        - path: "*.googleapis.com/**"
          ignore: true

# Assertions to verify properties of this state.
assertions:
  - name: "check output"         # optional label
    command: "echo hello"
    shell: true                   # default: true
    stdout: "hello\n"            # optional exact-match
    stderr: ""                   # optional exact-match
    should_fail: false           # default: false
    services:                    # optional background services
      - command: "my-server --port 0"
```

An empty config (`{}`) is valid. It declares a terminal state with no outgoing transitions and no assertions.

#### Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `env` | map | `{}` | Environment variables for this state. Missouri merges them over the project-level env, and a state variable of the same name wins. |
| `transitions` | list of [Transition](#transition) | `[]` | The transitions out of this state. |
| `assertions` | list of [Assertion](#assertion) | `[]` | The assertions that check this state. |
| `entrypoint` | bool | `false` | When true, this state's fixture is a complete start point. A path can begin here, and a path that arrives here stops. |
| `doc` | string | (none) | Prose that describes this state. `missouri docgen` renders it before the state's file tree. |

#### Transition

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | string | auto-generated (`statename[N]`) | A label for the output. |
| `command` | string | **required** | The command to run. |
| `shell` | bool | `true` | Run the command through `sh -c`. When false, missouri splits the command on whitespace. |
| `target` | string | **required** | The relative path to the target state directory. |
| `stdout` | string | (none) | The exact stdout to expect. Missouri checks it in every mode. |
| `stderr` | string | (none) | The exact stderr to expect. Missouri checks it in every mode. |
| `services` | list of [Service](#services) | `[]` | Background services to run during this transition. |
| `network` | [NetworkConfig](#network-interception) | (none) | The network interception config. |
| `comparators` | [Comparators](#comparators) | (none) | Change how missouri compares specific files, environment variables, or network requests. |
| `doc` | string | (none) | Prose that describes this transition. `missouri docgen` renders it. |

#### Assertion

Set `command` or `agent`, but not both.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | string | auto-generated | A label for the output. |
| `command` | string | (none) | The command to run. Required unless you set `agent`. |
| `agent` | string | (none) | The agent eval name. It matches `<config_dir>/<name>.md`. Conflicts with `command`. |
| `shell` | bool | `true` | Run the command through `sh -c`. Applies to command assertions only. |
| `stdout` | string | (none) | The exact stdout to expect. Applies to command assertions only. |
| `stderr` | string | (none) | The exact stderr to expect. Applies to command assertions only. |
| `should_fail` | bool | `false` | When true, the assertion passes if the command exits non-zero. Missouri still matches `stdout` and `stderr` when you set them. Applies to command assertions only. |
| `services` | list of [Service](#services) | `[]` | Background services to run during this assertion. |

### Comparators

A comparator changes how missouri compares a path, an environment variable, or a network request during a transition. Declare comparators under the `comparators` key on a transition.

#### File comparators (`comparators.files`)

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `path` | string | **required** | A relative path. A trailing `/` means a directory subtree. |
| `command` | string | (none) | A custom comparator command. Missouri passes two paths as arguments: actual, then expected. |
| `ignore` | bool | `false` | Remove this path from the comparison. |

Set `command` or `ignore: true`. When an entry sets both, `ignore` wins
and the command never runs.

#### Env comparators (`comparators.env`)

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | string | **required** | The environment variable name. |
| `command` | string | (none) | A custom comparator command. Missouri passes the two values as arguments. |
| `ignore` | bool | `false` | Remove this environment variable from the comparison. |

#### Network comparators (`comparators.network`)

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `path` | string | **required** | A URL path pattern, such as `"api.example.com/v1/messages"` or `"*.googleapis.com/**"`. |
| `command` | string | (none) | A custom comparator command. |
| `ignore` | bool | `false` | Remove the matching requests from the comparison. |

### Network interception

Configure network interception on each transition under the `network` key. Missouri uses mitmdump, from mitmproxy, to intercept HTTP and HTTPS traffic. Each transition uses one mode.

Replay mode replays traffic that you recorded earlier:

```yaml
network:
  replay: recordings/worker.flow
```

Missouri resolves the `replay` path against the source state's `<config_dir>/` directory.

Record mode captures the traffic during the transition:

```yaml
network:
  record: true
```

Missouri writes each recorded flow to `<source_state>/<config_dir>/recordings/<transition_name>.flow`.

When network interception is active, missouri sets these environment variables for the transition command:

| Variable | Value |
|----------|-------|
| `HTTPS_PROXY` | `http://127.0.0.1:<port>` |
| `HTTP_PROXY` | `http://127.0.0.1:<port>` |
| `NODE_EXTRA_CA_CERTS` | `~/.mitmproxy/mitmproxy-ca-cert.pem` |

mitmdump must be on PATH. Add `mitmproxy` to `packages`, or install it yourself.

### Services

Attach a background service to a transition or to an assertion. A service is a long-running process. Missouri starts it before the command runs. Missouri then stops it after the command finishes, first with SIGTERM and then with SIGKILL.

```yaml
services:
  - command: "my-server --port 0"
    shell: true                              # default: true
    port_pattern: "Serving on port (\\d+)"   # regex with one capture group
    ready: "curl -sf http://localhost:$PORT/health"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `command` | string | **required** | The command that starts the service. |
| `shell` | bool | `true` | Run the command through `sh -c`. |
| `port_pattern` | string | `listening.*:(\d+)` (runtime default) | A regex that reads the port number from stderr. It must hold exactly one capture group. Missouri applies the default at runtime, not in the config schema. Omit the field to use the default pattern. |
| `ready` | string | (none) | A readiness check command. Missouri retries it with exponential backoff, from 100ms to 5s, up to 10 times. `$PORT` is set in the environment. |

The service starts and prints its port to stderr. Missouri reads the port and sets these environment variables:

- With one service, `$PORT`.
- With several services, `$PORT` holds the first service's port, and `$PORT_0`, `$PORT_1`, and so on hold each port in order.

Missouri starts each service in its own process group, so cleanup can stop the whole process tree. Port detection times out after 30 seconds. If the service prints no matching line to stderr in that time, missouri stops the service and fails the step.

### Ignore patterns

The file `<config_dir>/ignore`, for example `.missouri/ignore`, uses gitignore syntax. Its patterns remove paths from the filesystem comparison for every transition.

```
# .missouri/ignore
*.log
tmp/
!important.log
```

The standard gitignore rules apply. A trailing `/` matches a directory. A `!` negates a pattern. A `**` matches across directory boundaries. A `#` starts a comment.

When you set `test_dir`, missouri loads the ignore file from the test directory's config directory, for example `tests/.missouri/ignore`.

### Shared bin directory

Missouri prepends `<config_dir>/bin/` to PATH for every command. This works at two levels:

- The project bin, `<root>/<config_dir>/bin/`, reaches every state and transition.
- A state bin, `<state>/<config_dir>/bin/`, reaches that state's assertions and the transitions that start there.

The PATH order is state bin, then project bin, then the base PATH.

When you set `test_dir`, missouri looks for the project bin in the test directory first. It then falls back to the root config directory.

### Sandbox and packages

When the project config sets `packages`, every command runs inside `nix shell nixpkgs#pkg1 nixpkgs#pkg2 ... --command`. Missouri resolves the nixpkgs flake reference to a pinned commit hash during a warm-up phase. The warm-up runs before parallel execution starts. This keeps parallel paths from competing for the registry file.

The `MISSOURI_SANDBOX` environment variable overrides the sandbox behavior:

| Value | Effect |
|-------|--------|
| `preinstalled` | Skip the nix shell. Assume that every package is already on PATH. Use this inside a nix derivation where the packages are `nativeCheckInputs`. |

Missouri exits with an error when `packages` is not empty, `nix` is not on PATH, and `MISSOURI_SANDBOX` is not `preinstalled`.

---

## State graph model

Missouri models a test suite as a directed graph. The states are the nodes, and the transitions are the edges.

A state is a directory on disk. Its contents are a snapshot of the filesystem at one point in the test. Each state directory holds a `<config_dir>/missouri.yml` file that declares its outgoing transitions and its assertions.

A transition is a command that changes the filesystem from one state to another. Missouri runs the command in a temp copy of the source state. It then diffs the result against the expected target state.

A root state has no inbound transitions. Test path enumeration starts at the root states.

A terminal state has no outgoing transitions. Its config is empty, or it holds assertions only. A test path ends here.

A test path is a walk through the graph from a root state. It ends at a terminal state, at a state that sets `entrypoint: true`, or at a cycle where every successor is already visited. Missouri enumerates every such path and runs the paths in parallel. A graph that branches (`A -> B` and `A -> C`) produces two paths.

A chained path is a multi-step path such as `A -> B -> C`. Missouri carries the temp directory from one transition forward as the input to the next. It does not copy the intermediate state from disk again.

Assertions run at the state boundaries. Full mode is the default. In Full mode, the source state's assertions run before the first transition, and the target state's assertions run after each transition. In CheckOnly mode, only the assertions run. Missouri skips the transitions and the filesystem comparison. In NoCheck mode, missouri skips the assertions.

In workspace mode, set by a `members` list in the project config, missouri treats each member directory as its own project. It runs the members one after another and reports the results for each member.
