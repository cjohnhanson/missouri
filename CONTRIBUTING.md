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

Some tests need other programs on `PATH`: `node` for the npm wrapper,
and `nix` for the illinois scenarios, which skip without it.

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
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cd tests/missouri && missouri run
```

After those, the check reads a review note from the head commit of the
branch. It reads the branch head, not the merge commit GitHub builds for
CI, because no reviewer saw that merge commit.

A second workflow, `docs`, runs on every pull request and every push to
`main`. It runs the checks described under [Docs tests](#docs-tests).
Pushes to `main` also check web links.

The review note is the maintainer's part. `.gaff/gaff.yml` names the
reviews a change must pass (`gaff reviews` prints them): `review-tests`,
`review-docs`, `review-code`, `review-deps`, and `review-usability`. For
each one, an agent that did not write the change reads it against the
criteria in `.agents/skills/<name>/SKILL.md`. It returns one line, which
names the commit it read:

```text
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

## Docs tests

`tests/docs.rs` runs every `console` block in `README.md` and `docs/*.md`
against the built binary with trycmd. Each page runs in its own scratch
directory with its own `HOME`, XDG directories and `TMPDIR`, and a cleared
environment.

Missouri prints colour codes even with `NO_COLOR` set. An open issue in
`.tisket/` tracks this. The test removes the codes so the page shows plain
text. Write each duration as `[..]`, which matches any text within one
line. `[CWD]` stands for the page's working directory.

A fence such as
`` ```yaml {filename="clean/.missouri/missouri.yml"} ``
declares a file the reader writes. The test writes that file before
running the commands that follow it.

Fixtures for each page live in `tests/docs/<page>/`. `setup.md` holds
blocks that run before the page. The test copies `work/` into the working
directory and `home/` into `HOME`.

The same test compares each visible command's `--help` output with a file
in `tests/snapshots/help/`. `tests/cli_reference.rs` generates
`docs/cli-reference.md` from the same doc comments in `src/cli.rs`.
Change help text in those comments, never in a snapshot or the generated
page.

These commands write the current output to the expected files:

```sh
TRYCMD=overwrite cargo test --test docs
SNAPSHOTS=overwrite cargo test --test docs
UPDATE_CLI_REFERENCE=1 cargo test --test cli_reference
```

Read the diff after running them.

In `README.md` and `docs/*.md`, put every missouri command in a `console`
block, except under `## Install`. A `sh`, `text` or other tagged block
never runs. Give every fence a language: trycmd runs fences that have
none.

The `docs` workflow calls `.github/workflows/family-checks.yml` in
[cjohnhanson/diataxis](https://github.com/cjohnhanson/diataxis).
The same checks run for missouri, tundish, gaff, tisket, zettel, almanac
and diataxis. They read `README.md` and `docs/` and fail when:

- `tests/docs.rs` is missing or does not name trycmd, `README.md` and
  `docs`.
- A command for any of those tools appears outside a `console` block,
  except under Install.
- A fence has no language or uses the `trycmd` language.
- The README does not open with `# missouri`, or has no paragraph before
  its first section.
- The README has no `## Install` section or does not end with
  `## License`.
- A link to a file or heading in the repository is broken.
- typos finds a misspelling.

Pushes to `main` also check web links. They skip the hosts in
`.lycheeignore`, which refuse automated requests.

The scripts and their rules live under `scripts/family/` in the diataxis
repository. Each script prints its rules with `--help`.

## Running the gates locally

`.gaff/gaff.yml` declares the gates once. `gaff init --github` writes
the workflows from it, and `gaff init --git` writes the hooks. To run
the gates against `HEAD` without a commit:

```sh
cargo install --locked --git https://github.com/cjohnhanson/gaff
gaff ci
```

The pre-push hook refuses a push that carries no review note. For a
one-off contribution, leave the hooks uninstalled and let CI run the
gates on the pull request.

## Pull requests

Branch from `main` and open the pull request from a fork. Keep a change
and its tests in one pull request. Write the commit message in the
imperative present: what the change does, and why, where the diff does
not show it.

## Releases

Before a tag, run `sh scripts/prepublish.sh` from the repository root.
It needs `cargo-hack`, `cargo-audit`, `python3`, `curl`, and `rustup`
with the toolchain `rust-toolchain.toml` pins. It checks every feature
combination. It runs the tests, clippy, and fmt under the pinned
toolchain, and audits the lock. It runs a publish dry run from a clean
checkout and confirms the version is not on crates.io. The first
publish of a name to crates.io is a manual `cargo publish` with a
token, because trusted publishing cannot create a crate.

PyPI needs its own step before the first tag, and only a person can do
it. Trusted publishing does create a new project, but only through a
pending publisher, and a pending publisher exists only once someone
registers it. Register one at
<https://pypi.org/manage/account/publishing/> with project `msri`,
owner `cjohnhanson`, repository `missouri`, workflow `release.yml`, and
no environment. Without it the tag publishes the crate and then fails
at `uv publish`, and crates.io never releases the version it took.

After both,
bump the version in `Cargo.toml`, commit, tag `v<version>`, and push
the tag. `.github/workflows/release.yml` builds and publishes from
there. A `workflow_dispatch` run of that workflow rehearses every
build and publishes nothing.

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
