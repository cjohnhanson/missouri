# 🔀 missouri

> Show-me state. Model-based testing where system behavior is represented
> as finite state automata.

Missouri tests CLI tools by modeling their behavior as directed graphs of
filesystem states. Each state is a directory containing the exact files
that should exist at that point. Transitions are shell commands. Verification
is a recursive byte-for-byte diff between the directory after a command
runs and the directory you said it should produce.

There's no assertion DSL. The expected state *is* the directory.

## How it works

A test suite is a set of directories, each representing a state. Each
state's `.missouri/missouri.yml` declares transitions: a command to run
and which state directory the filesystem should match afterward. Missouri
discovers all paths through the state graph, executes each in an isolated
temp directory with
a cleared environment, and diffs the result against the target.

```
clean/                  # starting state (empty project)
  .missouri/
    missouri.yml        # transitions: [{command: "echo hello > out.txt", target: ../built}]
built/                  # expected state after command
  .missouri/
    missouri.yml
  out.txt               # file that should exist: contains "hello"
```

```
$ missouri run
  PASS  clean -> built (create output)

1 path, 1 passed
```

States can have multiple outgoing transitions (branching) and multiple
states can transition into the same target (convergence). The directory
tree is the test suite. Walking it shows every intermediate and final
state the tool under test can produce.

## Isolation

Every command runs with `env_clear()`. No inherited environment
variables, no `HOME`, no `LANG`. All needed variables must be declared
explicitly in the test config. This makes tests reproducible across
machines and CI environments.

For stronger isolation: nix shell sandboxes and Docker containers are
supported.

## Beyond filesystem diffs

States can also declare assertions
(shell commands that pass or fail based on exit code and stdout/stderr),
custom comparators for files that need non-byte-for-byte comparison,
services for background processes, and agent assertions that delegate
subjective evaluation to an LLM.

## Install

Nothing is published yet. Every line here fails today. Each one works from
the first tagged release.

To run it without an install:

```sh
uvx msri
npx msri
```

To install it:

```sh
cargo install missouri
uv tool install msri
npm install -g msri
brew install cjohnhanson/tap/missouri
```

On PyPI the name is `msri`, because `missouri` was taken. npm uses `msri`
to match. On crates.io and in the tap it is `missouri`. The install puts
both names on your path. Type `missouri`.

A tagged release carries four archives: macOS and Linux, on x86-64 and
arm64. Each archive holds a prebuilt binary and the man page. A release
also carries a `.deb` for Debian and Ubuntu, on the same two
architectures. Install a `.deb` with `dpkg -i`. A `.deb` is a file, not a
repository, so `apt-get install` does not reach it. The [releases
page](https://github.com/cjohnhanson/missouri/releases) holds all of them.

A source build needs two things. Rust 1.85 or later, because this crate
is edition 2024. And a C compiler, because a dependency reads a remote
over HTTPS and that TLS stack builds a C library. On Debian and Ubuntu
that is `build-essential`; on macOS, the Xcode command line tools. A
prebuilt binary needs neither.

A checkout does not build from a clone alone. `diataxis` is an unpublished
dependency, so `cargo install --git` cannot resolve it. Clone `diataxis`
and `mdstore` beside this repository. Then patch both in
`.cargo/config.toml`, under `[patch.crates-io]`. The crate names there are
`diataxis` and `mdstore-core`.

## Usage

```
missouri init              # set up a new project
missouri state add <name>  # create a state directory
missouri run               # execute all test paths
missouri run -v            # verbose output
missouri list              # show states, transitions, paths
missouri validate          # check graph is well-formed
missouri report            # generate test reports
missouri docs [topic]      # bundled documentation
```

## Documentation

- [What is Missouri?](docs/what-is-missouri.md) — the testing model, why graphs, execution details
- [Getting Started](docs/getting-started.md) — first test suite walkthrough
- [Writing Tests](docs/writing-tests.md) — transitions, assertions, comparators, services
- [CLI Reference](docs/cli-reference.md) — complete command documentation

## Related

- [tisket](https://github.com/cjohnhanson/tisket) — issue tracker. Markdown issues with YAML frontmatter, in the repository
- [zettel](https://github.com/cjohnhanson/zettel) — zettelkasten notes for a repository
- [almanac](https://github.com/cjohnhanson/almanac) — agent skill index, over pluggable sources
- [gaff](https://github.com/cjohnhanson/gaff) — context-lifecycle handler for coding agents
- [mdstore](https://github.com/cjohnhanson/mdstore) — the frontmattered markdown library the other three store documents with

## License

MIT.
