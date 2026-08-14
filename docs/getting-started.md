<!-- metadata
title: "Getting Started with Missouri"
description: "Create your first state graph test suite"
type: tutorial
-->

# Getting Started with Missouri

Missouri tests a CLI tool by modeling its behavior as a graph of filesystem states. A transition says this: run this command on these files, and the result must match those files. In this tutorial you build a two-state graph, watch it pass, watch it fail, and add an assertion. That is enough to show whether the model fits your work. Read [What is Missouri?](/missouri/what-is-missouri) for the concepts behind the model.

## Install missouri

Build missouri from source:

```
cargo install --path missouri
```

Inside the codelikecody workspace, build it this way instead:

```
cargo build -p missouri
```

Check that the binary is available:

```
missouri --version
```

## Initialize a project

Create a directory and initialize the project:

```
mkdir my-project && cd my-project
missouri init
```

This command creates a `.missouri/` directory that holds:

```
.missouri/
  missouri.yml    # project-level config (empty for now)
  bin/            # shared scripts available on PATH during test runs
  ignore          # gitignore-syntax patterns to exclude from comparison
```

The project-level `missouri.yml` can declare environment variables, setup commands, and nix packages. Keep the empty default for now.

## Create two states

A state is a directory with a `.missouri/missouri.yml` file. The files in the directory are the expected filesystem at that point in the test.

Create the first state. It is an empty starting point:

```
missouri state add clean
```

This command creates `clean/.missouri/missouri.yml` with an empty config (`{}`).

Now create the second state. It holds the files that must exist *after* the command runs:

```
missouri state add built
```

Add the expected output file to the `built` state:

```
echo "hello" > built/output.txt
```

The directory tree now looks like:

```
my-project/
  .missouri/
    missouri.yml
    bin/
    ignore
  clean/
    .missouri/
      missouri.yml
  built/
    .missouri/
      missouri.yml
    output.txt
```

## Define a transition

Edit `clean/.missouri/missouri.yml` and declare the transition from the `clean` state to the `built` state:

```yaml
transitions:
  - name: "create output"
    command: "echo hello > output.txt"
    target: ../built
```

The fields:

- `name` -- an optional label for the test output.
- `command` -- the shell command to run. It runs via `sh -c` by default.
- `target` -- the relative path to the expected target state directory.

Missouri copies the source state's files to a temp directory. It runs the command there. It then compares the result against the target state's files. The transition passes when the two match.

## Run the tests

```
missouri run
```

The output looks like this:

```
  PASS  clean -> built (create output)

1 path, 1 passed
```

Missouri found two states and one transition from `clean` to `built`. It ran the command. It then confirmed that the resulting filesystem matched the `built` state.

## Validate without running

Check that the graph is well-formed. This command runs nothing:

```
missouri validate
```

```
valid: 2 state(s), 1 transition(s), 1 root(s)
```

List the test paths that missouri would run:

```
missouri list
```

## Add an assertion

An assertion is a command that verifies a property of a state. It does not change the state. Missouri runs the assertions for a state after it has verified every transition into that state.

Edit `built/.missouri/missouri.yml`:

```yaml
assertions:
  - name: "output contains hello"
    command: "cat output.txt"
    stdout: "hello\n"
```

The fields:

- `name` -- an optional label for the test output.
- `command` -- the command to run in the state's directory.
- `stdout` -- the exact stdout to expect. The assertion fails when the actual output differs.

Run the tests again:

```
missouri run
```

```
  PASS  clean -> built (create output)
    PASS  output contains hello

1 path, 1 passed
```

The assertion ran after the transition. It verified the file contents.

## See a test fail

Change the expected stdout to a wrong value. Edit `built/.missouri/missouri.yml`:

```yaml
assertions:
  - name: "output contains hello"
    command: "cat output.txt"
    stdout: "goodbye\n"
```

Run:

```
missouri run
```

```
  FAIL  clean -> built (create output)
    FAIL  output contains hello
      stdout mismatch:
        expected: "goodbye\n"
        actual:   "hello\n"

1 path, 0 passed, 1 failed
```

Missouri shows the exact mismatch. Change the value back to `"hello\n"` and the suite passes again.

A filesystem mismatch works the same way. Missouri reports the diff when the command produces a file that the target state does not hold, or when the contents differ.

## Next steps

- Add more states and chain the transitions into multi-step paths. Missouri finds every root-to-leaf path for you.
- Add `comparators` to a transition to skip a volatile file or to run a custom diff command. Read the [CLI reference](/missouri/cli-reference) for the full `missouri.yml` schema.
- Add `env` to a state or to the project config to set environment variables.
- Put shared scripts in `.missouri/bin/`. Missouri adds that directory to PATH during a test run.
- Use `--verbose` (`-v`) for detailed output. Use `--keep-temp` to read the temp directories that missouri creates.
