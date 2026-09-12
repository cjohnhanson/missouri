//! The short name, so `uvx msri` works. On npm the wrapper carries the
//! scope, so the same run reads `npx @cjohnhanson/msri`.
//!
//! maturin ties an installed command name to the Cargo bin name, and
//! refuses a `[project.scripts]` entry beside a binary. A wheel
//! published as `msri` therefore needs a `msri` command. Without one,
//! `uvx msri` fails and tells the reader to type
//! `uvx --from msri missouri` instead.
//!
//! This execs the `missouri` binary beside it rather than carrying a
//! second copy, so one install gives both names.
use std::os::unix::process::CommandExt;

fn main() -> std::process::ExitCode {
    let Ok(me) = std::env::current_exe() else {
        eprintln!("msri: cannot resolve its own path");
        return std::process::ExitCode::FAILURE;
    };
    let real = me.with_file_name("missouri");
    let err = std::process::Command::new(&real)
        .args(std::env::args_os().skip(1))
        .exec();
    eprintln!("msri: cannot run {}: {err}", real.display());
    std::process::ExitCode::FAILURE
}
