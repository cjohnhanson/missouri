<!-- metadata
title: "How to test a project's local CLI build"
description: "Add a suite to an existing Cargo project and run its freshly built binary from Missouri."
type: how-to
-->

# How to test a project's local CLI build

Make Missouri run the binary built from your current source. A successful
build does not put its output on the test PATH. An explicit wrapper keeps
the tests from accidentally running an older installed version.

This guide uses an existing Cargo project with a binary named `my-tool`
and a `--help` command that exits successfully. Replace `my-tool` with
your Cargo binary name in the build command, wrapper, and transition.
You need Cargo and Missouri 0.2.6 on PATH. The
[first tutorial](getting-started.md) covers Missouri installation and
the state model.

## Add a suite to the project

Run these commands from the directory containing `Cargo.toml`:

```sh
mkdir -p tests/missouri
missouri init -d tests/missouri
missouri state add clean -d tests/missouri
missouri state add help -d tests/missouri
```

If this suite already exists, keep its configuration and states. Add the
setup entry below to its existing configuration. For a new suite, replace
`{}` in `tests/missouri/.missouri/missouri.yml` with this setup entry:

```yaml
setup:
  - name: build the CLI
    command: cargo build --manifest-path ../../Cargo.toml --bin my-tool --target-dir ../../target
```

Setup runs from the suite root before test paths. This command builds
`target/debug/my-tool` in the repository. If your suite uses a root-level
`tests/missouri/missouri.yml`, edit that file instead; it takes precedence
over the file under `.missouri/`. Preserve any existing setup entries.

## Put the build output on the test PATH

From the repository root, create
`tests/missouri/.missouri/bin/my-tool`:

```sh
#!/bin/sh
set -eu
script_dir=$(CDPATH= cd "$(dirname "$0")" && pwd)
exec "$script_dir/../../../../target/debug/my-tool" "$@"
```

Make the wrapper executable:

```sh
chmod +x tests/missouri/.missouri/bin/my-tool
```

Missouri adds the suite's `.missouri/bin/` to its test PATH. The wrapper
finds the repository binary relative to its own file, so it works when
the test command runs inside a temporary state directory. It forwards
all arguments and preserves the binary's exit status.

For another build system, use its build command in `setup` and point
the wrapper at its output. If the binary requires environment variables,
declare them in the suite's `env` configuration. Missouri clears the
ordinary command environment; an interactive shell's variables are not
a substitute for that configuration.

## Check the actual binary

Replace `{}` in `tests/missouri/clean/.missouri/missouri.yml` with this transition:

```yaml
transitions:
  - name: the local CLI starts
    command: my-tool --help
    target: ../help
```

Leave the `help` state configuration as `{}`. Neither state needs fixture
files for this check. It tests that the freshly built program starts
successfully without changing files. Add an exact `stdout`
expectation if the help text itself is part of the behavior you need to
verify.

Run from the repository root:

```sh
missouri validate -d tests/missouri
missouri run -d tests/missouri -v
```

The run first reports the build setup, then the passing transition. A
failed build stops the suite. If `my-tool: command not found` appears,
first check that the run command includes `-d tests/missouri`. If the
command is still missing, check the wrapper name and executable bit. If
the wrapper reports a missing binary, check the build output and the path
in the wrapper.

## Add tests for changes to files

Use the same wrapper name in transition commands. Create a source state
with the required input files and a target state with the intended output.
For example, a configuration command can start with an empty directory
and produce a target containing the project's default configuration.
Write that expected configuration from the command's requirements before
comparing it with the implementation.

[How to write missouri tests as state graphs](writing-tests.md#writing-transitions) covers target
paths, branching, and output assertions. In a full run, target assertions
inspect the result of the transition. `--check-only` checks the saved
fixtures without executing the transitions.

Keep the suite, wrapper, and expected files in the project's normal Git
review. Run `missouri run -d tests/missouri` in CI after installing
Missouri and the build toolchain. If a test fails, use
[failure diagnosis](how-to-diagnose-failures.md) to inspect the command
result before changing the expected files.
