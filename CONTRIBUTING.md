# Contributing

## Use of AI

Say in the pull request when a coding assistant wrote part of the
change. Be able to explain the change in your own words, and write your
own comments on the pull request. A pull request the author cannot
explain is closed.

## Setup

```sh
git clone https://github.com/cjohnhanson/missouri
cd missouri
cargo build
cargo test --workspace --all-features
```

## Before a large change

Open a GitHub issue first. A small fix needs none.

The maintainer tracks work in markdown files under `.tisket/`. Read
them with `cat`, or with [tisket](https://github.com/cjohnhanson/tisket)
(`tisket issue list`, `tisket issue show <id>`). Do not edit them in a
pull request.

## What CI checks

`main` is protected. One required check, `gate`, decides a merge. It
runs on every pull request, and it runs the same commands the local git
hooks run:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-features
cd tests/missouri && missouri run
```

After those, the check reads a review note from the head commit of the
branch. It reads the branch head, not the merge commit GitHub builds for
CI, because no reviewer saw that merge commit.

The review note is the maintainer's part. `.gaff/gaff.yml` names the
reviews a change must pass (`gaff reviews` prints them): `review-tests`,
`review-docs`, `review-code`, `review-deps`, and `review-usability`. For
each one, an agent that did not write the change reads it against the
criteria in `.agents/skills/<name>/SKILL.md`. It returns one line, which
names the commit it read:

```
signoff[review-tests] PASS 4f1c2ab removed a guard, a_failed_signoff went red
```

Every line goes in one git note on that commit:

```sh
git notes --ref=reviews add -m '<the lines>' <sha>
```

`.agents/skills/signoff-driver/SKILL.md` has the procedure. As a
contributor from outside, open the pull request and stop there. The
maintainer runs the reviews on its head commit and writes the note.

The check refuses a note with any of these faults:

- no line for a declared review
- a `FAIL` verdict
- a line that names another commit, or a sha under seven characters
- two lines for one review
- fewer than three words of evidence

Prose around the lines is ignored, so a note can also carry a narrative.

## Running the gates locally

`.gaff/gaff.yml` declares the gates once, and CI reads that file, so a
local run and CI cannot differ. To run them against `HEAD` without a
commit:

```sh
cargo install --git https://github.com/cjohnhanson/gaff
gaff ci
```

`gaff init --git` installs the same gates as git hooks. The pre-push
hook refuses a push that carries no review note. For a one-off
contribution, leave the hooks uninstalled and let CI run the gates on
the pull request.

## Pull requests

Branch from `main` and open the pull request from a fork. Keep a change
and its tests in one pull request. Write the commit message in the
imperative present: what the change does, and why, where the diff does
not show it.

## Releases

Before a tag, run `sh scripts/prepublish.sh`. It needs `cargo-hack` and
`cargo-audit`. It checks every feature combination, runs the tests,
clippy, and fmt under the pinned toolchain, audits the lock, runs a
publish dry run from a clean checkout, and confirms the version is not
on crates.io. Then bump the version in `Cargo.toml`, commit, tag
`v<version>`, and push the tag. `.github/workflows/release.yml` builds
and publishes from there. A `workflow_dispatch` run of that workflow
rehearses every build and publishes nothing.

## What not to commit

`.gitignore` covers `target/`, editor and agent state, and
`.cargo/config.toml`. Two things it cannot catch. An absolute path that
names a home directory exposes an account name and breaks every other
clone. A `path = "..."` dependency override that points outside the
repository does the same; put it in `.cargo/config.toml`.

## Questions

Open a GitHub issue.

## Security

Do not open a public issue for a vulnerability. See
[SECURITY.md](SECURITY.md).

## License

Contributions are licensed under MIT, the same as the project.
