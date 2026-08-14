//! Man page and shell completion generation. The package build calls
//! this through hidden subcommands, so the pages always match the real
//! CLI definitions and cannot drift from `--help`.

use std::path::Path;

/// Write one section-1 man page for the command and one for each
/// visible subcommand, recursively.
///
/// Two names matter, and they differ. The page name joins the command
/// path with hyphens, so `man missouri-run` works and the file is
/// `missouri-run.1`. The synopsis must show what a person actually
/// types, which is `missouri run`. This is the convention git follows:
/// the page is `git-commit(1)`, and its synopsis reads `git commit`.
pub fn write_man_pages(cmd: &clap::Command, dir: &Path) -> std::io::Result<()> {
    let name = cmd.get_name().to_string();
    write_pages_rec(cmd, &name, &name, dir)
}

fn write_pages_rec(
    cmd: &clap::Command,
    page_name: &str,
    invocation: &str,
    dir: &Path,
) -> std::io::Result<()> {
    // clap's builder wants a 'static name without the `string` feature.
    // The generator runs once per page and exits, so the leak is bounded.
    let leaked_page: &'static str = Box::leak(page_name.to_string().into_boxed_str());
    let leaked_bin: &'static str = Box::leak(invocation.to_string().into_boxed_str());
    let man = clap_mangen::Man::new(cmd.clone().name(leaked_page).bin_name(leaked_bin));
    let mut buf: Vec<u8> = Vec::new();
    man.render(&mut buf)?;
    std::fs::write(dir.join(format!("{page_name}.1")), buf)?;
    for sub in cmd.get_subcommands() {
        if sub.is_hide_set() || sub.get_name() == "help" {
            continue;
        }
        write_pages_rec(
            sub,
            &format!("{page_name}-{}", sub.get_name()),
            &format!("{invocation} {}", sub.get_name()),
            dir,
        )?;
    }
    Ok(())
}
