# 🔀 missouri

> Show-me state. Model-based testing where system behavior is represented
> as finite state automata.

Missouri tests CLI tools by modeling their behavior as directed graphs of
filesystem states. Each state is a directory containing the exact files
that should exist at that point. Transitions are shell commands. Verification
is a recursive byte-for-byte diff between the directory after a command
runs and the directory you said it should produce.

There's no assertion DSL. The expected state *is* the directory.

## How it works

A test suite is a set of directories, each representing a state. Each
state's `.missouri/missouri.yml` declares transitions: a command to run
and which state directory the filesystem should match afterward. Missouri
discovers all paths through the state graph, executes each in an isolated
temp directory with
a cleared environment, and diffs the result against the target.

```
clean/                  # starting state (empty project)
  .missouri/
    missouri.yml        # transitions: [{command: "echo hello > out.txt", target: ../built}]
built/                  # expected state after command
  .missouri/
    missouri.yml
  out.txt               # file that should exist: contains "hello"
```

```
$ missouri run
  PASS  clean -> built (create output)

1 path, 1 passed
```

States can have multiple outgoing transitions (branching) and multiple
states can transition into the same target (convergence). The directory
tree is the test suite. Walking it shows every intermediate and final
state the tool under test can produce.

## Isolation

Every command runs with `env_clear()`. No inherited environment
variables, no `HOME`, no `LANG`. All needed variables must be declared
explicitly in the test config. This makes tests reproducible across
machines and CI environments.

For stronger isolation: nix shell sandboxes and Docker containers are
supported.

## Beyond filesystem diffs

States can also declare assertions
(shell commands that pass or fail based on exit code and stdout/stderr),
custom comparators for files that need non-byte-for-byte comparison,
services for background processes, and agent assertions that delegate
subjective evaluation to an LLM.

## Usage

```
missouri init              # set up a new project
missouri state add <name>  # create a state directory
missouri run               # execute all test paths
missouri run -v            # verbose output
missouri list              # show states, transitions, paths
missouri validate          # check graph is well-formed
missouri report            # generate test reports
missouri docs [topic]      # bundled documentation
```

## Documentation

- [What is Missouri?](docs/what-is-missouri.md) — the testing model, why graphs, execution details
- [Getting Started](docs/getting-started.md) — first test suite walkthrough
- [Writing Tests](docs/writing-tests.md) — transitions, assertions, comparators, services
- [CLI Reference](docs/cli-reference.md) — complete command documentation
