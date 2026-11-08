<!-- metadata
title: "How to diagnose a failing test"
description: "Identify the failing comparison, reproduce a transition, and verify a repair without accepting incorrect fixtures."
type: how-to
-->

# How to diagnose a failing test

Start at the suite's root, the directory containing its states. Use `-d /path/to/suite` with the commands below if the shell is elsewhere. The [getting-started tutorial](getting-started.md) supplies a small suite with an intentional assertion failure.

## Check discovery before execution

```sh
missouri validate
missouri list --show paths
missouri list --show transitions
```

Validation checks the graph without executing transitions. If it reports an invalid target or configuration, correct that first. Confirm that the expected transition belongs to a listed path. A test cannot verify a command that the graph never executes.

Run the suite with details:

```sh
missouri run -v
```

Find the first failing step in a path. Later steps on that path do not run after failure. Separate command failure from a filesystem mismatch or an assertion failure before changing the fixture.

| Failure | Inspect | Repair |
| --- | --- | --- |
| Exit status differs | Command, expected exit code, and stderr | Correct the command or its expected status after checking the intended behavior. |
| Stdout or stderr differs | Exact expected and actual text, including newlines | Correct output handling or the assertion. Do not remove a meaningful check to hide the difference. |
| File differs or is missing | Target fixture and command result | Decide whether the implementation is wrong or the expected behavior changed. Update only the incorrect side. |
| Extra file appears | The file's purpose and whether it is stable | Include required output in the fixture. Ignore only output that the test deliberately does not verify. |
| Assertion fails | The assertion command and state where it runs | Reproduce the property check independently before changing it. |

## Separate assertions from transitions

```sh
missouri run --check-only -v
missouri run --no-check -v
```

`--check-only` checks assertions against copies of the fixture states. It does not rerun the transition or inspect a previous failed run's directory. `--no-check` runs transitions and filesystem comparisons but skips assertions. These diagnostic runs cover different properties; rerun the complete suite after repair.

## Inspect a command's resulting files

Use `--keep-temp` to keep each temporary test directory after the run.

```sh
missouri run --keep-temp -v
```

For each directory, read the path in `missouri: kept temporary directory: <path>` on stderr. Inspect the files in that directory for the actual test result. Remove the directory when the investigation ends.

## Verify the change

Review the implementation and fixture diff together. If an expected result changed, state why the new result is correct. Then run:

```sh
missouri validate
missouri run -v
```

Both commands must succeed. A passing `--no-check` run alone leaves assertions untested. If the suite contains agent assertions, the full run invokes their configured Claude Code evaluations; review those requirements before running it. See [agent assertions](writing-tests.md#agent-assertions) for that boundary.
