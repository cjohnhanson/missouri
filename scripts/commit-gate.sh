#!/bin/sh
# The commit gate. A commit needs formatted code and a clippy run with
# no warnings. Tests and the review note wait for the push gate.
set -e

# The checks read the working tree. When unstaged Rust changes differ
# from the index, the tree check certifies content the commit does not
# carry, so the gate refuses that state first.
if ! git diff --quiet -- '*.rs' 2>/dev/null; then
	echo "commit-gate: unstaged Rust changes differ from the index." >&2
	echo "  Stage them or stash them, so the checked tree is the committed tree." >&2
	exit 1
fi

echo "commit-gate: cargo fmt --check"
cargo fmt --check >/dev/null || {
	echo "commit-gate: the tree is not formatted. Run: cargo fmt" >&2
	exit 1
}

# `-D warnings` turns every warning into a failure, so the exit code
# decides and nothing parses the output.
echo "commit-gate: cargo clippy"
out=$(cargo clippy --workspace --all-targets --quiet -- -D warnings 2>&1) || {
	echo "commit-gate: clippy is not clean. Every warning fails this gate." >&2
	printf '%s\n' "$out" | tail -30 >&2
	exit 1
}
echo "commit-gate: ok"
