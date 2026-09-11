#!/bin/sh
# Everything that must be true before this crate publishes.
#
# `cargo test --all-features` covers one feature set, and a consumer
# can take any other. cargo-hack reads the feature list from cargo
# metadata and walks the powerset, so no combination goes unchecked.
set -eu
[ -f rust-toolchain.toml ] || {
	echo "  prepublish: run this from the repository root, where rust-toolchain.toml is." >&2
	exit 1
}
# The toolchain rust-toolchain.toml pins, so the version has one home.
TC="${TOOLCHAIN:-$(sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml)}"
fail=0
say() { printf '  %-46s %s\n' "$1" "$2"; }

# A refusal prints why. Without the captured output every failure reads
# as a bare FAILS, and a full disk looks like a packaging fault.
run() {
	label="$1"
	shift
	# The assignment sits in the `if` test, where `set -e` does not
	# fire. As a plain statement, a failing check exited the script
	# before the else branch could print anything.
	if out=$("$@" 2>&1); then
		say "$label" ok
	else
		say "$label" FAILS
		printf '%s\n' "$out" | tail -12 | sed 's/^/      /'
		fail=1
	fi
}

# Every program the checks call, named before the first check runs. A
# missing one is then one line, not a failure halfway through.
for tool in cargo-hack cargo-audit rustup python3 curl; do
	case "$tool" in
	cargo-*) cargo "${tool#cargo-}" --version >/dev/null 2>&1 && continue ;;
	*) command -v "$tool" >/dev/null 2>&1 && continue ;;
	esac
	case "$tool" in
	cargo-*) echo "  prepublish: $tool is missing. cargo install $tool" >&2 ;;
	rustup) echo "  prepublish: rustup is missing. https://rustup.rs" >&2 ;;
	*) echo "  prepublish: $tool is missing. Install it with the system package manager." >&2 ;;
	esac
	exit 1
done

# --no-dev-deps and --all-targets are mutually exclusive in cargo-hack,
# so the two checks below run separately.
run "check, every feature alone" rustup run "$TC" cargo hack check --each-feature --no-dev-deps
run "check, every feature, tests" rustup run "$TC" cargo hack check --each-feature --all-targets
# The powerset runs without --all-targets. Building every test once per
# combination exhausts memory.
run "check, the feature powerset" rustup run "$TC" cargo hack check --feature-powerset --no-dev-deps
run "test --all-features" rustup run "$TC" cargo test --all-features
run "clippy" rustup run "$TC" cargo clippy --all-targets --all-features -- -D warnings
run "fmt" rustup run "$TC" cargo fmt --check
run "audit" cargo audit --deny yanked --deny warnings

# A clean checkout is what a runner and a consumer both get. A patch
# file in the working tree hides an unresolvable dependency.
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
git archive HEAD | tar -x -C "$tmp"
run "clean checkout resolves" sh -c "cd '$tmp' && rustup run '$TC' cargo metadata --format-version 1"
run "publish dry run" sh -c "cd '$tmp' && rustup run '$TC' cargo publish --locked --dry-run --allow-dirty"

# A version already on the registry cannot be republished, and finding
# that out during the upload wastes the release.
#
# Read the package from the metadata's package list, not from the first
# "name" in the json. A build-script target carries one too, and
# crates.io answers 404 for a name it does not have.
#
# Read the status code, not curl's exit code. `curl -sf` exits nonzero
# for a 404, a DNS failure, a refused connection and a 5xx alike, and
# only the 404 means the version is free. This check refuses any other
# answer.
meta=$(rustup run "$TC" cargo metadata --no-deps --format-version 1)
name=$(printf '%s' "$meta" | python3 -c 'import json,sys; print(json.load(sys.stdin)["packages"][0]["name"])')
ver=$(printf '%s' "$meta" | python3 -c 'import json,sys; print(json.load(sys.stdin)["packages"][0]["version"])')
code=$(curl -s -o /dev/null -w '%{http_code}' \
	-H "User-Agent: prepublish (cody@codyjhanson.com)" \
	"https://crates.io/api/v1/crates/$name/$ver" 2>/dev/null || echo 000)
case "$code" in
404) say "version $ver is unpublished" ok ;;
200)
	say "version $ver is unpublished" "FAILS (already published)"
	fail=1
	;;
*)
	say "version $ver is unpublished" "FAILS (crates.io answered $code)"
	fail=1
	;;
esac

[ "$fail" -eq 0 ] && echo "  PRE-PUBLISH: ok" || echo "  PRE-PUBLISH: REFUSED"
exit "$fail"
