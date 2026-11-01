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

Release 0.2.5 removes temporary directories even with `--keep-temp`, and it does not print their paths. The unreleased fix retains them and reports each path on stderr. In a binary containing the fix, run:

```sh
missouri run --keep-temp -v
```

Read the directory named by `missouri: kept temporary directory: <path>`. It contains the actual test result. Remove that retained directory when the investigation is finished.

For the released 0.2.5 tutorial suite, reproduce the transition in a separate directory:

```sh
suite_root=$PWD
failure_dir=$(mktemp -d)
cp -R "$suite_root/clean/." "$failure_dir/"
(
  cd "$failure_dir"
  sh -c 'echo hello > output.txt'
)
printf 'Reproduction directory: %s\n' "$failure_dir"
cat "$failure_dir/output.txt"
diff -u "$suite_root/built/output.txt" "$failure_dir/output.txt"
```

This example reproduces the tutorial's exact command without modifying either fixture. For another transition, substitute its source state, target, and command. A manual shell does not recreate Missouri's configured setup, environment, shared scripts, or container/Nix sandbox. Reproduce those inputs too, or use a fixed binary when they affect the failure.

## Verify the change

Review the implementation and fixture diff together. If an expected result changed, state why the new result is correct. Then run:

```sh
missouri validate
missouri run -v
```

Both commands must succeed. A passing `--no-check` run alone leaves assertions untested. If the suite contains agent assertions, the full run invokes their configured Claude Code evaluations; review those requirements before running it. See [agent assertions](writing-tests.md#agent-assertions) for that boundary.
