<!-- metadata
title: "Writing Missouri Tests"
description: "How to model tests as state graphs with transitions, assertions, and services"
type: guide
-->

# Writing Missouri Tests

A state is a directory. A transition is a shell command. Missouri copies
a state to a temp directory, runs the transition command, and diffs the
result against the target state directory.

Say your CLI starts with the files in state A. You run `my-tool init`,
and the result must match the files in state B. So state A holds a
transition with the command `my-tool init` and the target state B.

A state can also carry assertions. An assertion is a command that
verifies a property that the filesystem snapshot does not hold, such as
an exit code, the stdout content, or the behavior of the tool.

## Directory structure

```
my-project/tests/missouri/
├── .missouri/
│   ├── missouri.yml      # project-level config
│   ├── ignore            # gitignore-syntax patterns to exclude from comparison
│   └── bin/              # scripts on PATH during test runs
├── state-a/
│   ├── .missouri/
│   │   └── missouri.yml  # state config (transitions, assertions)
│   ├── file.txt          # fixture files for this state
│   └── src/
│       └── main.rs
├── state-b/
│   ├── .missouri/
│   │   └── missouri.yml
│   └── ...
```

Each state directory holds two things:

1. A `.missouri/missouri.yml` config file. It declares the transitions,
   the assertions, and the environment variables.
2. The files that make up this state. These files are the fixture.

Missouri finds the states by walking the tree and looking for
`<config_dir>/missouri.yml` files. The project root's config is the
project-level config, not a state.

## Project-level config

The project-level `missouri.yml` lives in one of two places. It sits at
the test suite root (`tests/missouri/.missouri/missouri.yml`) or as a
root-level file (`tests/missouri/missouri.yml`). The root-level file wins
when both files exist.

```yaml
# Project-level environment variables. Every state inherits these.
env:
  NO_COLOR: "1"

# Setup commands. They run once, before any test path.
setup:
  - name: "build project"
    command: "cargo build --quiet --manifest-path ../../Cargo.toml"

# Nix packages to provide during a test run
packages:
  - git
  - jq

# Optional: start state discovery in this subdirectory
test_dir: tests/smoke

# Optional: workspace mode. Run several member suites in turn.
members:
  - clc/tests/missouri
  - tisket/tests/missouri
```

> **Missouri clears the environment.** A command runs with `PATH` and the variables you declare in an `env` block. It gets nothing else: no `HOME`, no `TMPDIR`, and no `SHELL`. When a command fails for no clear reason, check whether it needs a variable that you did not declare.

### Field reference (ProjectConfig)

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `env` | map | `{}` | Environment variables that every state inherits |
| `setup` | list | `[]` | Commands to run before any test path |
| `packages` | list | `[]` | Nix packages to provide through `nix shell` |
| `test_dir` | string | none | The directory where state discovery starts, relative to the config |
| `members` | list | `[]` | Workspace member directories |

### Setup commands

Setup commands run in order, before any test path. They run on the host,
never inside a sandbox, and they run from the project root directory. The
common use is to build the binary under test.

```yaml
setup:
  - name: "build tisket"
    command: "cargo build --quiet --manifest-path ../../Cargo.toml"
  - command: "db-seed"
    shell: false
```

Each setup command needs a `command`. The `name` and `shell` fields are
optional. `shell` defaults to `true`, so the command runs through `sh -c`.
Set `shell: false` to run the command directly.

### The ignore file

Put an `ignore` file at `.missouri/ignore`. It uses gitignore syntax.
Missouri removes every path that matches a pattern here from the
filesystem comparison, for every transition.

```
# .missouri/ignore
.git/
```

The clc test suite ignores `.git/` because the git internals are not
deterministic. The comparison engine uses the `ignore` crate, so the full
gitignore syntax works: `*`, `**`, `!` for negation, `#` for a comment,
and a trailing `/` for a directory.

### The bin directory

Missouri prepends `.missouri/bin/` to PATH during a test run. Put your
custom comparators, test helpers, and wrapper scripts there.

```
.missouri/bin/
├── validate-settings    # custom comparator
├── compare-issue        # another custom comparator
└── setup-divergent-branch  # test helper
```

## Writing states

### Fixture files

A state directory holds the files that you expect at that point in the
test. Missouri runs a transition in three steps. It copies the source
state to a temp directory. It runs the command there. It then diffs the
temp directory against the target state directory.

Files in `.missouri/` are never part of the fixture. They are config.

### Dotfile fixtures via dot- directories

Git cannot track a directory such as `.git/` or `.clc/` inside a test
fixture. Missouri works around this with the `dot-` convention. At
runtime, it restores a directory named `.missouri/dot-<name>/` as
`.<name>/` in the temp directory.

```
initialized/.missouri/
├── missouri.yml
├── dot-git/         # becomes .git/ at runtime
│   ├── HEAD
│   └── config
└── dot-clc/         # becomes .clc/ at runtime
    └── .gitkeep     # .gitkeep files are skipped during restoration
```

Missouri skips every `.gitkeep` file inside a `dot-` directory. Those
files exist only to make git track an empty directory.

### Entrypoints

By default, missouri traces each path from a root state. A root state has
no inbound transitions. Set `entrypoint: true` to mark a state as a valid
start point for a subgraph:

```yaml
entrypoint: true

assertions:
  - name: "everything looks right"
    command: "test -d .clc"
```

Use an entrypoint when a state costs a lot to reach through transitions
and you want to test from a pre-built snapshot.

### Environment variables

A state inherits the project-level environment. It can also override a
variable or add its own:

```yaml
env:
  APP_ENV: test
  DB_URL: "postgres://localhost/test"
```

The project environment is the base. The state environment overrides it.
Missouri clears the environment before it runs a command (`env_clear`), so
a command sees `PATH` and the declared variables only.

## Writing transitions

A transition connects two states. It says this: run this command on the
source state, and the result must match the target state.

```yaml
transitions:
  - name: "initialize project"
    command: "my-tool init"
    target: "../initialized"
```

### Field reference (TransitionConfig)

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | string | auto-generated | A label for the output |
| `command` | string | **required** | The shell command to run |
| `target` | path | **required** | The relative path to the target state directory |
| `shell` | bool | `true` | Run the command through `sh -c` |
| `comparators` | object | none | Change the comparison for specific files, environment variables, or network requests |
| `network` | object | none | The network interception config, either replay or record |
| `stdout` | string | none | The exact stdout to expect |
| `stderr` | string | none | The exact stderr to expect |
| `services` | list | `[]` | Background services to run during this transition |

### Target resolution

A target is a relative path. Missouri resolves it from the source state
directory. The common pattern is `../sibling-state`:

```
states/
├── before/          # source
│   └── .missouri/missouri.yml  →  target: "../after"
└── after/           # target
```

Deeper nesting also works, such as `../../other-suite/some-state`. The
path must resolve to a directory that holds a `.missouri/missouri.yml`
file.

### Multi-step transitions

A state can have several outgoing transitions, and a target state can have
its own transitions. Missouri finds every path through the graph and tests
each one. In a chained path such as A -> B -> C, the output of the A->B
transition becomes the input to the B->C transition.

```yaml
# state-a/missouri.yml
transitions:
  - name: "step one"
    command: "init-tool"
    target: "../state-b"

# state-b/missouri.yml
transitions:
  - name: "step two"
    command: "run-tool"
    target: "../state-c"
```

Missouri finds the path `state-a -> state-b -> state-c` and runs both
transitions in order.

### Branching

One state can have several transitions to different targets. Use this to
model different outcomes:

```yaml
transitions:
  - name: "close issue"
    command: "tisket issue close fix-the-widget"
    target: "../issue-closed"
  - name: "edit issue"
    command: "tisket issue edit fix-the-widget --status todo"
    target: "../issue-edited"
  - name: "create second issue"
    command: "tisket issue create 'Write tests' -p bugs"
    target: "../has-two-issues"
```

Each branch becomes its own test path.

### Shell vs direct execution

By default a command runs through `sh -c`, so pipes, redirects, and
multi-statement commands work:

```yaml
command: "git init -q -b main && my-tool init"
```

Set `shell: false` to run the command directly. The shell then reads
nothing:

```yaml
command: "/usr/bin/my-tool"
shell: false
```

### Stdout and stderr assertions on transitions

Check the exact command output next to the filesystem diff:

```yaml
transitions:
  - name: "echo test"
    command: "echo hello"
    target: "../next"
    stdout: "hello\n"
    stderr: ""
```

These are exact-match comparisons. Missouri checks no output when you omit
both fields.

## Writing assertions

An assertion is a command attached to a state. It verifies a property that
the filesystem snapshot does not hold. It runs against a copy of the
state's fixture in a temp directory.

```yaml
assertions:
  - name: "config file exists"
    command: "test -f .clc/config.yml"

  - name: "config show reflects custom value"
    command: "clc config show 2>&1 | grep 'main_branch: trunk'"

  - name: "issue list is empty"
    command: "tisket issue list"
    stdout: ""
```

### Field reference (AssertionConfig)

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | string | auto-generated | A label for the output |
| `command` | string | **required** | The command to run |
| `shell` | bool | `true` | Run the command through `sh -c` |
| `stdout` | string | none | The exact stdout to expect |
| `stderr` | string | none | The exact stderr to expect |
| `should_fail` | bool | `false` | Pass when the command exits non-zero |
| `services` | list | `[]` | Background services to run during this assertion |

### When to use assertions vs transitions

Use a **transition** to test a state change. It says that command X on
state A produces state B. The filesystem diff is the main check.

Use an **assertion** to test a property of a state in place. Examples are
a command exit code, the stdout content, and behavior that depends on
runtime state such as a git branch.

A state can hold both transitions and assertions. An assertion runs
against the state fixture. A transition runs against the fixture and then
diffs the result.

### Expecting failure

Check that a command *must* fail:

```yaml
assertions:
  - name: "init when already initialized fails"
    command: "tisket init"
    should_fail: true
    stderr: "error: already initialized (tisket.yml exists)\n"
```

With `should_fail: true`, the assertion passes when the command exits
non-zero. Add `stderr` to verify the error message too.

### States with only assertions (no transitions)

A terminal state has no outgoing transitions. Such a state often carries
assertions only. The assertions verify the result of the transition that
reached the state:

```yaml
# issue-closed/.missouri/missouri.yml
assertions:
  - name: "issue status is done"
    command: "grep -q 'status: done' .tisket/default/fix-the-widget.md"
```

A root state can also hold assertions only. Add `entrypoint: true` to
verify a pre-built snapshot:

```yaml
entrypoint: true

assertions:
  - name: ".clc directory exists"
    command: "test -d .clc"
  - name: "settings.local.json is valid JSON"
    command: "jq empty .claude/settings.local.json"
```

## Agent assertions

An agent assertion uses an LLM to check a state property that a
deterministic command checks poorly. Set an `agent:` field instead of a
`command:` field. The `agent:` field names a markdown eval file in the
config directory.

```yaml
assertions:
  - agent: eval-skill-commands
  - agent: eval-output-quality
    name: "output meets quality bar"
```

### Eval files

The eval file lives at `<config_dir>/<name>.md`, for example
`.missouri/eval-skill-commands.md`. It can hold YAML frontmatter that
configures the agent:

```markdown
---
model: haiku
max_cost_cents: 50
allowed_tools:
  - "Bash(npm test*)"
---

Verify that every CLI command mentioned in this skill file
exists on PATH and accepts the flags shown. Run each command
with --help or equivalent. If any command is missing or rejects
its flags, fail with details about which command and what went wrong.
```

### Frontmatter fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `model` | string | sonnet | The model to use for the evaluation |
| `max_cost_cents` | integer | none | The budget cap, in cents |
| `max_turns` | integer | none | The maximum number of agent turns. Missouri reserves this field and does not enforce it yet |
| `allowed_tools` | list | none | Extra tools the agent can use, beyond the defaults |
| `extra_args` | list | none | Extra CLI arguments for the agent |

Every field is optional. The markdown body after the frontmatter becomes
the agent's evaluation prompt.

### How it works

The eval agent gets the markdown body as its prompt. Missouri adds a
preamble that names the working directory and describes the verdict
protocol. The agent reads files and runs read-only commands. It then
returns a verdict by calling `missouri agent pass` or
`missouri agent fail <details>`.

By default an eval agent can use `Read`, `Glob`, `Grep`, and
`Bash(missouri agent*)`. Grant more tools with the `allowed_tools`
frontmatter field.

### When to use agent assertions

Use an agent assertion for a property that needs judgment. Three
examples:

- Do the error messages in this module follow the style guide?
- Does this skill file name commands that exist?
- Is this generated documentation clear and complete?

Use a command assertion for a deterministic check, such as a file that
must exist, output that must match, or an exit code. A command assertion
is faster, cheaper, and repeatable.

## Custom comparators

By default missouri runs a recursive file-by-file diff between the actual
output and the expected target state. Change that diff for specific paths
like this:

```yaml
transitions:
  - command: "clc init"
    target: "../initialized"
    comparators:
      files:
        - path: ".claude/settings.local.json"
          command: "validate-settings"
        - path: "logs/"
          ignore: true
        - path: ".git/"
          ignore: true
```

### File comparators

Each entry under `comparators.files` holds these fields:

| Field | Type | Description |
|-------|------|-------------|
| `path` | string | A relative path. A trailing `/` matches a directory subtree |
| `command` | string | A custom comparator command. Missouri passes the actual path and the expected path as arguments |
| `ignore` | bool | Remove this path from the comparison |

A custom comparator command gets two arguments: the actual file path, then
the expected file path. Exit 0 to pass. Exit non-zero to fail.

```bash
#!/usr/bin/env bash
# .missouri/bin/validate-settings
# $1 = actual file, $2 = expected file
set -euo pipefail
jq empty "$1" || { echo "FAIL: not valid JSON"; exit 1; }
jq -e '.hooks' "$1" >/dev/null || { echo "FAIL: missing hooks"; exit 1; }
```

### Ignoring paths

Use `ignore: true` on a path that changes at random, or on a path that
your test does not cover:

```yaml
comparators:
  files:
    - path: ".clc/"
      ignore: true
    - path: ".git/"
      ignore: true
    - path: ".worktrees/"
      ignore: true
```

A comparator applies to one transition. Use the `.missouri/ignore` file to
ignore a path across the whole project.

### Environment variable comparators

Change the comparison for a specific environment variable:

```yaml
comparators:
  env:
    - name: BUILD_TIMESTAMP
      ignore: true
    - name: VERSION
      command: "compare-semver"
```

### Network request comparators

Change the comparison for a request pattern when a transition intercepts
network traffic:

```yaml
comparators:
  network:
    - path: "api.anthropic.com/v1/messages"
      command: "compare-api-calls"
    - path: "*.googleapis.com/**"
      ignore: true
```

## Background services

A transition or an assertion can start a background service, such as a
server or a daemon. Missouri starts the service before the command runs
and stops it afterward.

```yaml
transitions:
  - command: "curl http://localhost:$PORT/"
    target: "../next"
    services:
      - command: "my-server --port 0"
```

Missouri reads the service's stderr and waits for the port line. The
default pattern is `listening.*:(\d+)`. Missouri takes the port from that
line and sets `$PORT` in the environment of the transition command or the
assertion command.

### Field reference (ServiceConfig)

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `command` | string | **required** | The command that starts the service |
| `shell` | bool | `true` | Run the command through `sh -c` |
| `port_pattern` | string | `listening.*:(\d+)` | A regex that reads the port from stderr. It must hold one capture group |
| `ready` | string | none | A readiness check command. `$PORT` is set. Missouri retries it with backoff |

### Readiness checks

Use `ready` when the service needs time to start:

```yaml
services:
  - command: "/usr/bin/my-server"
    shell: false
    port_pattern: "Serving on port (\\d+)"
    ready: "curl -sf http://localhost:$PORT/health"
```

Missouri retries the readiness check up to 10 times with exponential
backoff. The wait starts at 100ms and stops growing at 5s.

### Multiple services

With several services, missouri sets `$PORT_0`, `$PORT_1`, and so on.
`$PORT` always holds the first service's port.

```yaml
services:
  - command: "server-a --port 0"
  - command: "server-b --port 0"
    ready: "curl -sf http://localhost:$PORT_1/ready"
```

### Services on assertions

A service on an assertion works the same way:

```yaml
assertions:
  - command: "curl -sf http://localhost:$PORT/"
    services:
      - command: "my-server --port 0"
```

## Network interception

A transition can intercept HTTP and HTTPS traffic through mitmproxy. Use
this to record traffic and to replay it.

### Replay mode

Replay traffic that you recorded earlier:

```yaml
transitions:
  - command: "clc dispatch test"
    target: "../next"
    network:
      replay: .missouri/recordings/worker.flow
```

Missouri resolves the `replay` path against the source state directory.

### Record mode

Record the traffic during a transition:

```yaml
transitions:
  - command: "clc dispatch test"
    target: "../next"
    network:
      record: true
```

In record mode, missouri starts mitmdump. It sets `HTTPS_PROXY`,
`HTTP_PROXY`, and `NODE_EXTRA_CA_CERTS` in the command's environment. It
then saves the captured flow file.

## Running tests

```bash
# Run every test path
missouri run -d tests/missouri

# Verbose output. Show the passing steps too.
missouri run -d tests/missouri -v

# Keep the temp directories for debugging
missouri run -d tests/missouri --keep-temp

# Run the assertions only. Skip the transitions and the filesystem comparison.
missouri run -d tests/missouri --check-only

# Run the transitions and the filesystem comparison only. Skip the assertions.
missouri run -d tests/missouri --no-check

# Record the transition output
missouri run -d tests/missouri --record
```

### Check modes

| Flag | Transitions | Filesystem diff | Assertions |
|------|-------------|-----------------|------------|
| (none) | yes | yes | yes |
| `--check-only` | no | no | yes |
| `--no-check` | yes | yes | no |

Use `--check-only` to work on the assertions quickly. It does not run the
transitions again. Use `--no-check` to update the fixtures after a change.
Run the transitions, read the diff, then update the expected state.

### Debugging failures

**Verbose output** (`-v`): shows the passing steps as well as the
failures. It prints all assertion output, the command stdout and stderr,
and the comparison details.

**Keep temp directories** (`--keep-temp`): missouri keeps the temp
directories where the transitions ran instead of deleting them. It prints
the paths in the output. Read them to see what the command produced.

**List paths** before a run to understand the graph:

```bash
missouri list --show paths
missouri list states -d tests/missouri
missouri list transitions -d tests/missouri
```

## Patterns from the codebase

### Pattern: build-then-test with setup

The clc and tisket test suites both build the binary under test in the
setup phase. The binary on PATH then matches the current source:

```yaml
# .missouri/missouri.yml
setup:
  - name: "build clc"
    command: "cargo build --quiet --manifest-path ../../Cargo.toml"
packages:
  - git
  - jq
```

### Pattern: ignore non-deterministic paths per transition

Ignore the files that a transition creates or changes when those files are
not the point of the test:

```yaml
transitions:
  - name: "close issue"
    command: "tisket issue close fix-the-widget"
    target: "../issue-closed"
    comparators:
      files:
        - path: ".tisket/bugs/"
          ignore: true
```

### Pattern: assertion-heavy root states

The clc `initialized` state carries dozens of assertions. They verify the
result of `clc init`. They check that files exist, that the JSON structure
is right, that the hooks are wired, and that the commands behave. Together
they catch regressions in the initialization path. No separate transition
is needed for each check.

### Pattern: custom comparator scripts in bin/

Some files hold content that changes between runs, such as JSON with
embedded paths. Write a comparator script for those files. The script
checks the structure instead of the exact bytes:

```bash
#!/usr/bin/env bash
# .missouri/bin/validate-settings
# Missouri passes: $1 = actual file, $2 = expected file
set -euo pipefail
jq empty "$1" || { echo "FAIL: not valid JSON"; exit 1; }
jq -e '.hooks' "$1" >/dev/null || { echo "FAIL: missing hooks"; exit 1; }
```

Then name the script in the config. It is on PATH:

```yaml
comparators:
  files:
    - path: ".claude/settings.local.json"
      command: "validate-settings"
```

### Pattern: multi-command transitions

A transition command can run several shell steps. Use this when the setup
is part of the transition:

```yaml
transitions:
  - name: "setup git repo with branches"
    command: >
      git init -q -b main &&
      git -c user.name=test -c user.email=test@test add -A &&
      git -c user.name=test -c user.email=test@test commit -q -m "init" &&
      my-tool init &&
      git -c user.name=test -c user.email=test@test add -A &&
      git -c user.name=test -c user.email=test@test commit -q -m "tool init"
    target: "../ready-state"
    comparators:
      files:
        - path: ".git/"
          ignore: true
```

### Pattern: should_fail for error paths

Check that a command fails and prints the expected error message:

```yaml
assertions:
  - name: "create issue in nonexistent project fails"
    command: "tisket issue create 'Something' -p nonexistent"
    should_fail: true
    stderr: "error: project 'nonexistent' not found\n"

  - name: "close nonexistent issue fails"
    command: "tisket issue close foo"
    should_fail: true
    stderr: "error: issue 'foo' not found\n"
```

## Further reading

- [What is Missouri?](/missouri/what-is-missouri) — how a state graph works, why missouri uses env_clear, and the comparison rules
- [CLI Reference](/missouri/cli-reference) — the full command and config schema reference
- [Getting Started](/missouri/getting-started) — build your first test suite step by step
