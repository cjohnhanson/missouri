<!-- metadata
title: "Getting Started with Missouri"
description: "Create your first state graph test suite"
type: tutorial
-->

# Getting Started with Missouri

Missouri tests a CLI tool by modeling its behavior as a graph of filesystem states. A transition says that this command, run on these files, must produce those files. In this tutorial you build a two-state graph, watch it pass, add an assertion, and then watch it fail. Read [What is Missouri?](what-is-missouri.md) for the concepts behind the model.

## Install missouri

Build missouri from source:

```sh
cargo install --locked --git https://github.com/cjohnhanson/missouri
```

Check that the binary is available:

```console
$ missouri --version
missouri [..]

```

In this output, `[..]` stands for the version number.

## Initialize a project

Create a directory for the project and change into it:

```sh
mkdir my-project && cd my-project
```

Initialize the project with `missouri init`. In this output, `[CWD]` stands for the project directory.

```console
$ missouri init
initialized missouri project at [CWD]/.missouri

```

This command creates a `.missouri/` directory that holds:

```text
.missouri/
  missouri.yml    # project-level config (empty for now)
  bin/            # shared scripts available on PATH during test runs
  ignore          # gitignore-syntax patterns to exclude from comparison
```

The project-level `missouri.yml` can declare environment variables, setup commands, and nix packages. Keep the empty default for now.

## Create two states

A state is a directory with a `.missouri/missouri.yml` file. The files in the directory are the expected filesystem at that point in the test.

Create the first state. It is an empty starting point:

```console
$ missouri state add clean
created state "clean"

$ cat clean/.missouri/missouri.yml
{}

```

This command creates `clean/.missouri/missouri.yml` with an empty config (`{}`).

Now create the second state. It holds the files that must exist *after* the command runs:

```console
$ missouri state add built
created state "built"

```

Create `built/output.txt` with the single line `hello`:

```text {filename="built/output.txt"}
hello
```

The directory tree now looks like:

```text
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

```yaml {filename="clean/.missouri/missouri.yml"}
transitions:
  - name: "create output"
    command: "echo hello > output.txt"
    target: ../built
```

The fields:

- `name`. An optional label for the test output.
- `command`. The shell command to run. It runs through `sh -c` by default.
- `target`. The relative path to the expected target state directory.

Missouri copies the source state's files to a temp directory. It runs the command there. It then compares the result against the target state's files. The transition passes when the two match.

## Run the tests

Run the suite with `missouri run`. Each `[..]` in the output stands for a duration, which differs on every run.

```console
$ missouri run
✓ [1/1]
PASS clean → built [..]

1 passed, 0 failed, 1 step in [..]

```

Missouri found two states and one transition from `clean` to `built`. It ran the command. It then confirmed that the resulting filesystem matched the `built` state.

## Validate without running

Check that the graph is well-formed. This command runs nothing:

```console
$ missouri validate
valid: 2 state(s), 1 transition(s), 1 root(s)

```

List the test paths that missouri would run:

```console
$ missouri list
1. clean → built

1 path(s)

```

## Add an assertion

An assertion is a command that verifies a property of a state. It does not change the state. Missouri runs a target state's assertions after each transition into it, and a source state's assertions before the first transition out of it.

Edit `built/.missouri/missouri.yml`:

```yaml {filename="built/.missouri/missouri.yml"}
assertions:
  - name: "output contains hello"
    command: "cat output.txt"
    stdout: "hello\n"
```

The fields:

- `name`. An optional label for the test output.
- `command`. The command to run in the state's directory.
- `stdout`. The exact stdout to expect. The assertion fails when the actual output differs.

Run the tests again. Missouri prints a passing assertion only with `-v`:

```console
$ missouri run -v
✓ [1/1]
PASS clean → built [..]
  ✓ clean → built (create output) [..]
    ✓ assert: output contains hello [..]

1 passed, 0 failed, 1 step, 1 assertion in [..]

```

The assertion ran after the transition. It verified the file contents.

## See a test fail

Replace the contents of `built/output.txt` with `goodbye`. The transition still writes `hello`, so its output no longer matches the expected file in the `built` state.

```text {filename="built/output.txt"}
goodbye
```

Run:

```console
$ missouri run
? 1
✗ [1/1]
FAIL clean → built [..]
  ✗ clean → built (create output) [..]
    differs: output.txt
      expected:
      goodbye
      
      actual:
      hello

0 passed, 1 failed, 1 step, 1 assertion in [..]

```

The line `? 1` means the command exited with status 1. Missouri names the file that differs and prints its expected and actual content. Write `hello` back into `built/output.txt` to make the suite pass again. A failed assertion also fails the suite; a stdout mismatch prints `stdout expected:` and `stdout actual:`.

## Next steps

- Add more states and chain the transitions into multi-step paths. Missouri finds every root-to-leaf path for you.
- Add `comparators` to a transition to skip a volatile file or to run a custom diff command. Read the [configuration format](config-format.md) for the full `missouri.yml` schema.
- Add `env` to a state or to the project config to set environment variables.
- Put shared scripts in `.missouri/bin/`. Missouri adds that directory to PATH during a test run.
- Use `--verbose` (`-v`) for detailed output.
