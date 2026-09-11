#!/bin/sh
# The merge gate runs on pre-push, and CI runs the same commands on a
# pull request. A pre-push hook is advisory, because `--no-verify`
# skips it, so the required CI check is what decides a merge into
# protected `main`.
#
# A push needs passing tests and a review note that names every review
# .gaff/gaff.yml declares. `gaff reviews check` holds the note
# requirement, so no review name appears here. The missouri suite runs
# where tests/missouri exists.
#
# The suites test the working tree, not the pushed commit.
set -e

# Every required review needs vendored criteria, and every vendored
# review needs to be required. A name with no criteria is a review
# nobody can perform. A criterion nobody requires is a check one edit
# dropped, so the loops below read both directions.
command -v gaff >/dev/null || {
	echo "merge-gate: gaff is not on PATH, so the review check cannot run." >&2
	echo "  cargo install --git https://github.com/cjohnhanson/gaff" >&2
	exit 1
}
required=$(gaff reviews)
# No pathname expansion while the names are split into words.
set -f
for name in $required; do
	if [ ! -f ".agents/skills/$name/SKILL.md" ]; then
		echo "merge-gate: $name is required and has no criteria in .agents/skills." >&2
		echo "  Vendor it: almanac add github:cjohnhanson/skills --path skills/$name --name $name --accept" >&2
		exit 1
	fi
done
set +f
for dir in .agents/skills/review-*/; do
	[ -d "$dir" ] || continue
	name=${dir#.agents/skills/}
	name=${name%/}
	if ! printf '%s\n' "$required" | grep -qxF "$name"; then
		echo "merge-gate: $name is vendored and required by nothing." >&2
		echo "  Name it under reviews: in .gaff/gaff.yml, or remove it." >&2
		exit 1
	fi
done

# git sends the ref list on stdin, and the first reader consumes it.
# Capture it before any other program reads it. A test runner that
# reads stdin first leaves the check below at EOF, so it checks nothing.
gate_refs=$(cat)

echo "merge-gate: cargo test"
# --all-features, because a feature that is off by default is still
# shipped code. Capture the output. A reader needs the failing test's
# name first, and /dev/null hides it from the CI log.
test_out=$(cargo test --workspace --all-features --quiet 2>&1 </dev/null) || {
	echo "merge-gate: cargo test failed." >&2
	printf '%s\n' "$test_out" | tail -40 >&2
	exit 1
}

# The CI runner has no nix, but it preinstalls the packages the
# suites declare. When CI is set, missouri uses the preinstalled
# backend. A local run keeps the nix backend.
if [ -n "${CI:-}" ]; then
	MISSOURI_SANDBOX=preinstalled
	export MISSOURI_SANDBOX
fi

if [ -d tests/missouri ]; then
	command -v missouri >/dev/null || {
		echo "merge-gate: missouri is not on PATH and tests/missouri exists." >&2
		exit 1
	}
	echo "merge-gate: missouri run"
	out=$(cd tests/missouri && missouri run </dev/null 2>&1) || {
		echo "merge-gate: the missouri suite failed." >&2
		printf '%s\n' "$out" | tail -20 >&2
		exit 1
	}
	# The exit code decides, and the summary adds a second check. The run
	# must report one or more passed paths and zero failures, so an empty
	# suite does not pass.
	printf '%s\n' "$out" | grep -E '[1-9][0-9]* passed, 0 failed' >&2 || {
		echo "merge-gate: the suite reported no passing path." >&2
		exit 1
	}
fi

# A POSIX pipeline exits with its final command, so this call stays
# last. A command after it would discard the refusal.
printf '%s\n' "$gate_refs" | gaff reviews check
echo "merge-gate: ok"
