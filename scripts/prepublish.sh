#!/bin/sh
# Everything that must be true before this crate publishes.
#
# Written after a sibling crate published a lib that did not compile
# under one of its own feature sets. `cargo test --all-features` passed,
# and that was read as proof. One feature set is not a feature matrix,
# and this crate declares none today, which is a reason to keep the
# check rather than drop it: the first feature added is covered.
#
# cargo-hack reads the features from cargo metadata. A script that parses
# Cargo.toml misses a name it did not expect, and a silent skip is worse
# than no check.
set -eu
TC="${TOOLCHAIN:-1.98.0}"
fail=0
say() { printf '  %-46s %s\n' "$1" "$2"; }
run() {
	label="$1"
	shift
	if "$@" >/dev/null 2>&1; then say "$label" ok; else
		say "$label" FAILS
		fail=1
	fi
}

# `cargo hack`, not `command -v cargo-hack`. cargo finds its own
# subcommands in its bin directory, which a hook's PATH need not carry.
if ! cargo hack --version >/dev/null 2>&1; then
	echo "  prepublish: cargo-hack is missing. cargo install cargo-hack" >&2
	exit 1
fi

# --no-dev-deps and --all-targets are mutually exclusive in cargo-hack.
run "check, every feature alone" rustup run "$TC" cargo hack check --each-feature --no-dev-deps
run "check, every feature, tests" rustup run "$TC" cargo hack check --each-feature --all-targets
run "test --all-features" rustup run "$TC" cargo test --all-features
run "clippy" rustup run "$TC" cargo clippy --all-targets --all-features -- -D warnings
run "fmt" rustup run "$TC" cargo fmt --check
run "audit" cargo audit --deny yanked --deny warnings

# A clean checkout is what a runner and a consumer both get. A patch
# file in the working tree hides a dependency that cannot resolve, which
# is the failure that held this whole release.
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
git archive HEAD | tar -x -C "$tmp"
run "clean checkout resolves" sh -c "cd '$tmp' && rustup run '$TC' cargo metadata --format-version 1"
run "publish dry run" sh -c "cd '$tmp' && rustup run '$TC' cargo publish --locked --dry-run --allow-dirty"

# A version already on the registry cannot be republished, and finding
# that out during the upload wastes the release. Read the package from
# cargo metadata rather than the first "name" in the json: a build-script
# target carries one too, and asking crates.io about it always 404s,
# which reports every version unpublished.
meta=$(rustup run "$TC" cargo metadata --no-deps --format-version 1)
name=$(printf '%s' "$meta" | python3 -c 'import json,sys; print(json.load(sys.stdin)["packages"][0]["name"])')
ver=$(printf '%s' "$meta" | python3 -c 'import json,sys; print(json.load(sys.stdin)["packages"][0]["version"])')
if curl -sf -H "User-Agent: prepublish" "https://crates.io/api/v1/crates/$name/$ver" >/dev/null 2>&1; then
	say "version $ver is unpublished" FAILS
	fail=1
else
	say "version $ver is unpublished" ok
fi

[ "$fail" -eq 0 ] && echo "  PRE-PUBLISH: ok" || echo "  PRE-PUBLISH: REFUSED"
exit "$fail"
