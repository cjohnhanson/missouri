# missouri

> Show-me state. End-to-end testing as directed graphs of filesystem
> states.

Missouri tests a CLI tool by modeling its behavior as a directed graph of
filesystem states. A state is a directory. It holds the exact files that
must exist at that point in the test. A transition is a shell command.
Missouri runs the command, then compares the resulting directory against
the target state directory, byte for byte.

There is no assertion language. The expected state is the directory.

## How it works

A test suite is a set of directories, and each directory is a state. Each
state's `.missouri/missouri.yml` declares its transitions. A transition
names a command to run and the state directory that the filesystem must
match after the command. Missouri finds every path through the graph,
runs each path in a temp directory with a cleared environment, and diffs
the result against the target.

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
✓ [1/1]
PASS clean → built 14ms

1 passed, 0 failed, 1 step in 14ms
```

A state can have several outgoing transitions, which is branching, and
several states can point at the same target, which is convergence. The
directory tree is the test suite.

## Isolation

Every command runs with `env_clear()`. The process inherits no
environment variable from the host. Declare every variable a test needs
in the test config. The test then behaves the same on your machine and in
CI.

For stronger isolation, missouri also runs a command inside a nix shell
or a Docker container.

## Beyond filesystem diffs

A state can also declare assertions. An assertion is a shell command, and
its exit code and output decide whether it passes. A transition can
declare a custom comparator for a file that a byte-for-byte diff reads
wrong, and a background service for a command that needs a server
running. An agent assertion hands a judgment call to an LLM.

## Install

The package is `msri` on PyPI and npm, because `missouri` was taken. The
command is `missouri` everywhere, and both names install together.

Not released yet. Until the first tag, build from source:

```sh
cargo install --locked --git https://github.com/cjohnhanson/missouri
```

Requires Rust 1.88 and a C compiler. macOS and Linux, x86-64 and arm64.

From the first release onward:

```sh
cargo install --locked missouri
brew install cjohnhanson/tap/missouri
uv tool install msri
npm install -g msri
```

Or run it without installing:

```sh
uvx msri run
npx msri run
```

From that point the [releases
page](https://github.com/cjohnhanson/missouri/releases) also carries
prebuilt archives and a `.deb`. Each archive holds the binary and the man
page. Install a `.deb` with `dpkg -i`: it
is a file, not a repository, so `apt-get install` does not reach it.

Check the install with `missouri --version`.

## Usage

```
missouri init              # set up a new project
missouri state add <name>  # create a state directory
missouri run               # execute all test paths
missouri run -v            # verbose output
missouri list              # show states, transitions, paths
missouri validate          # check graph is well-formed
missouri report            # generate a report from a recorded run
missouri docs [topic]      # bundled documentation
```

## Documentation

- [What is Missouri?](docs/what-is-missouri.md). The testing model, why graphs, and how a path runs.
- [Getting Started](docs/getting-started.md). A walkthrough of a first test suite.
- [Writing Tests](docs/writing-tests.md). Transitions, assertions, comparators, and services.
- [CLI Reference](docs/cli-reference.md). Every command and every config field.

## Related

- [tisket](https://github.com/cjohnhanson/tisket). An issue tracker. Markdown issues with YAML frontmatter, in the repository.
- [zettel](https://github.com/cjohnhanson/zettel). Zettelkasten notes for a repository.
- [almanac](https://github.com/cjohnhanson/almanac). An agent skill index over pluggable sources.
- [gaff](https://github.com/cjohnhanson/gaff). A context-lifecycle handler for coding agents.
- [mdstore](https://github.com/cjohnhanson/mdstore). The frontmattered markdown library that tisket, zettel, and almanac store documents with.

## License

MIT.
