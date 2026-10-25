---
title: Resolve a suite's nix packages once, not once per command
status: in_progress
priority: null
assignee: null
due_date: null
labels:
- performance
- nix
depends_on: []
created: 2026-10-25T00:29:23Z
updated: 2026-10-25T00:29:23Z
---

## Problem

When project config sets `packages`, missouri wraps every transition command, assertion command, and file comparator command in its own `nix shell` call. Each wrapper runs `nix shell <pinned nixpkgs>#<pkg> ... --command sh -c '<command>'`.

Each call waits on the nix daemon. One call takes about 0.8 s on an idle machine and 5 to 20 s when many commands run in parallel. The wait grows with the number of commands that run at the same time because they share one daemon.

For a 37-path suite that lists packages, a run takes 13.1 min instead of 23.5 min with packages already on `PATH` and no wrapper. Summed path time falls from 204 min to 123 min. Every path gives the same verdict. A one-line `jq` assertion takes under 0.05 s instead of 5.0 s.

## Change

1. missouri already pins `nixpkgs` to a flake URL once during warm-up. Resolve packages after this pin, before the parallel run. Run one `nix shell <packages> --command /usr/bin/env -0` with a cleared environment and a marker value as `PATH`.
2. Capture the text before the marker in `PATH`. This text contains the package output `bin` directories that `nix shell` puts before `PATH`. Capture every other variable the call adds. On macOS, nix also passes `__CF_USER_TEXT_ENCODING` to its child.
3. Root those store paths with `nix build --out-link` in a temporary directory. Keep the directory for the entire run. These roots prevent garbage collection from deleting the store paths during the run.
4. Run each command directly as `sh -c '<command>'`. Set `PATH` to the captured directories followed by the `PATH` missouri builds today. Pass the captured variables to each command. Keep the command string unchanged.
5. If capture fails or the marker is not where expected, print one warning. Wrap each command in `nix shell` as today.
6. Include the change in release 0.2.4. Keep config keys, flags, and output formats unchanged. A suite that passes today passes after the change.

## Acceptance

1. Run the example graphs that list packages. Confirm that they pass as today.
2. Run a test with a stand-in `nix` program that counts its calls. Confirm one `nix shell` capture per suite and no `nix shell` calls per command.
3. Run a test whose stand-in `nix` returns a malformed capture. Confirm one warning and one `nix shell` wrapper per command. Confirm the same verdict.
4. Read the parser unit tests. Confirm coverage for a good capture, a missing marker, and an extra variable.

## Scratch Notes

## Options weighed

1. Resolve once per suite (chosen). One `nix shell ... --command /usr/bin/env -0` capture in warm-up, then plain `sh -c` per command. Exact by construction, because the capture is the environment `nix shell` gives a child. On by default. Fallback to today's wrapper when the capture has an unexpected shape.
2. Build PATH from `nix build --print-out-paths`. Rejected: `nix shell` also follows propagated packages and picks the outputs to install, so a PATH built by hand can differ from the one `nix shell` gives.
3. `nix print-dev-env`. Rejected: it gives a build environment, not the `nix shell` environment, so commands would see different variables.
4. Run a suite's graph as a tree, so a shared prefix runs once. Not now: it changes what a run proves (a transition that is not deterministic is shared, not replayed), so it must be opt-in, and after the per-command cost is gone the replay that remains in a large suite is small.
5. Run workspace members at the same time on one pool. Not now: a large workspace already keeps the pool near full, so the gain is the tail of each member.

## Risks and answers

1. Garbage collection during a run deletes a store path the PATH names. Answer: `nix build --out-link` roots each captured store path in a temporary directory that lives for the run.
2. A package that ships `sh`. Answer: missouri finds `sh` on the same PATH the wrapper gave it, so the same `sh` runs.
3. A nix version that changes what `nix shell` sets. Answer: the capture records every variable the call adds and applies it, and the fallback covers an output it cannot read.
4. Stderr: `nix shell` can print warnings into a command's stderr today. Without the wrapper those lines are gone. A suite that pins nix's own warning text in an expected stderr would see a difference. No example graph does.
