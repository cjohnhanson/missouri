<!-- metadata
title: "What is Missouri?"
description: "Why filesystem state graphs and how missouri's testing model works"
type: explanation
-->

# What is Missouri?

*Show-me-state: e2e testing as directed graphs of filesystem states.*

Missouri tests a CLI tool with one claim, repeated many times. The claim
is this: run this command against this filesystem state, and the result
must match that filesystem state. Every state is a directory on disk.
Every transition is a shell command. Every assertion is a diff.

## States and transitions

A state is a directory. It holds the exact files that must exist at that
point in the test. A transition is a command. Missouri runs the command
against a copy of the source state, and the result must match the target
state. Missouri verifies the result with a recursive diff between the
command output and the target state directory. An extra file, a missing
file, or a content mismatch is a failure.

There is no assertion language. The expected state *is the directory*.

## Why directed graphs

A linear test runs state A, then state B, then state C. This works for a
simple sequence. But a CLI tool does not behave in one line. Several
commands can apply to the same start state. Several follow-up commands
can be worth testing after one command runs. The result is a directed
graph.

Missouri models the graph directly. Each state directory holds a
`.missouri/missouri.yml` file that declares its transitions. A transition
names the command to run and the state directory that the result must
match. A state can have several outgoing transitions. This is branching.
Several states can also point at the same target. This is convergence.

Define the initial state once, even when 30 test paths share it. To add a
test scenario, add a directory and a transition. The existing states do
not change. The directory tree is the test suite. Walk the tree to see
every state that the tool under test can produce.

## State discovery

Missouri finds the graph by walking the filesystem. Any subdirectory that
holds a `.missouri/missouri.yml` file is a state. The project root's
`missouri.yml` file is not a state. It is the project-level config, and it
holds environment variables, setup commands, and sandbox settings. The
project can also keep this file at `.missouri/missouri.yml`.

Discovery is recursive. A state can sit at any depth below the root.
Missouri skips hidden directories, which are the ones that start with `.`.
Missouri builds the graph in four phases:

1. **Collect** — find every directory that holds `.missouri/missouri.yml`.
2. **Build nodes** — parse each config and assign a state ID. Merge the
   environment variables. The project environment is the base, and the
   state environment overrides it.
3. **Resolve edges** — resolve each transition's relative `target` path to
   a state ID. This builds the adjacency list.
4. **Resolve assertions** — attach each assertion command to its state.

A root state has no inbound transitions. Path enumeration starts from the
root states. Missouri walks the graph with depth-first search and finds
every simple path from each root. A simple path visits no state twice.
Each path runs as an independent test.

## How paths run

Missouri runs each test path one step at a time:

1. **Copy to the temp directory** — Missouri copies the source state's
   files into a fresh temp directory. It excludes the `.missouri/` config
   directory from the copy. It restores each `dot-<name>/` directory
   inside `.missouri/` as `.<name>/` in the temp directory. This lets a
   fixture carry dotfile state such as `.git/` or `.clc/`, which git
   cannot track directly.

2. **Run the command** — Missouri runs the transition's shell command
   inside the temp directory. The environment is controlled: `env_clear`,
   then the declared environment variables, then a constructed PATH. The
   command sees only what the config declares.

3. **Diff the result** — Missouri compares the temp directory against the
   target state directory. It walks both trees and excludes the
   `.missouri/` directory from both sides. The project-level ignore
   patterns in `.missouri/ignore` remove more files from the comparison.
   These patterns use gitignore syntax. Missouri compares each file
   byte-for-byte. An extra file, a missing file, or a content mismatch is
   a failure.

4. **Chain forward** — Missouri carries the temp directory forward when
   the path has more steps. The command has already changed that
   directory, and it becomes the working directory for the next
   transition. The output of one step is the input of the next.

Missouri runs the paths in parallel with rayon. Each path gets its own
temp directory, so the paths share no mutable state.

## Why `env_clear`

Every command starts with `env_clear()`. The process inherits nothing from
the host environment: no `HOME`, no `LANG`, and no `EDITOR`. Environment
variables come from three sources only. The first is the project-level
`env` config. The second is the state-level `env` config, which overrides
the project level. The third is `PATH`, which Missouri builds from the
project's `bin/` directory and the system paths.

A test can pass because `$TERM` is set on your machine, then fail in CI.
That is a coincidence, not a test. `env_clear` forces the test config to
declare every variable that the test needs.

## What gets compared, what doesn't

**Compared by default:**
- Every file and directory in both trees. The actual tree is the temp
  directory after the command. The expected tree is the target state
  directory.
- File contents, byte-for-byte.
- An extra file in the actual tree is a failure.
- A file that is in the expected tree but not the actual tree is a
  failure.
- A content mismatch is a failure. Missouri prints the diff.

**Excluded automatically:**
- The `.missouri/` config directory on both sides.
- Every file that matches a pattern in `.missouri/ignore`. This file uses
  gitignore syntax, such as `*.log` and `__pycache__/`.

**Excluded per transition:**
- A file comparator with `ignore: true` skips one path.
- A directory comparator, which ends in `/`, skips a whole subtree.

**Custom comparison:**
- A file comparator with a `command` runs that command. Missouri passes
  the actual path and the expected path as arguments. Exit code 0 means
  the files match.
- An environment variable comparator works the same way.
- A network comparator matches HTTP requests. Use it on a transition that
  intercepts traffic with mitmproxy.

**Environment variables:**
- Missouri compares them only when the target state declares `env`, or
  when the transition declares environment comparators. It skips the
  comparison when the target state has no `env` config.

**Stdout and stderr:**
- Missouri compares them only when the transition config or the assertion
  config sets `stdout` or `stderr`. The comparison is an exact string
  match.

## Assertions beyond filesystem state

Some properties do not appear as files on disk. A state's
`.missouri/missouri.yml` can declare `assertions` for these properties. An
assertion is a shell command. It runs against the state's files, and its
exit code decides whether it passes. An assertion can also declare an
expected `stdout` and `stderr` for an exact match. Set `should_fail: true`
to require a non-zero exit code.

An assertion runs *inside* a temp copy of the state, so it cannot change
the fixture. Use an assertion to verify a computed property. For example:
does `jq` parse this file, does this command print the expected output, or
does this config file hold the right key.

## How missouri relates to other test approaches

**Unit tests** verify one function at a time. Missouri does not replace
them. Missouri tests the assembled tool from the outside.

**Integration test harnesses** are Rust tests that call your CLI binary.
They work, but they move the fixture setup into code. The fixture is
whatever the test function builds at runtime. Missouri turns this around.
Its fixtures are real directories. You can read, copy, and diff them
outside any test framework.

**Snapshot testing** captures output and compares it against a stored
snapshot. Missouri is a form of snapshot testing. Its snapshot is a whole
directory tree, and the transitions between snapshots are part of the
model.

**Docker-based e2e tests** give you isolation, but a container costs time,
and containers do not model a state graph well. Missouri gets its
isolation from temp directories and `env_clear`. That approach is cheaper
and needs no container runtime. For stronger isolation, missouri also
supports nix shell sandboxes and microsandbox microVMs.

## Getting started

Read [Getting Started](/missouri/getting-started) for the setup steps and
a walkthrough of your first test. Read
[CLI Reference](/missouri/cli-reference) for the CLI commands.
