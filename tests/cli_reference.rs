//! docs/cli-reference.md is generated from the clap definition, so the
//! page cannot drift from `--help`. This test fails when the committed
//! page differs from what the definition renders. To rewrite the page:
//!
//!     UPDATE_CLI_REFERENCE=1 cargo test --test cli_reference

use clap::CommandFactory as _;

const PAGE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/docs/cli-reference.md");

const HEADER: &str = "<!-- metadata
title: missouri CLI reference
description: Every command, argument, and flag of the missouri binary, generated from its definition
type: reference
-->

# missouri CLI reference
";

fn rendered() -> String {
    // Through the KDL text and back, as `usage generate md` reads a spec.
    // The direct conversion in usage-lib 7.1 leaves each positional
    // argument's usage string empty, and the page then names no argument.
    let spec: usage::Spec = usage::Spec::from(missouri::cli::Args::command())
        .to_string()
        .parse()
        .expect("the usage spec parses back");
    let body = usage::docs::markdown::MarkdownRenderer::new(spec)
        .with_theme(usage::docs::markdown::MarkdownTheme::Detailed)
        .render_spec()
        .expect("render the usage spec");
    // The renderer opens with its own marker comment and an H1 naming
    // the binary. The page's H1 comes from HEADER instead.
    let body = body
        .lines()
        .skip_while(|l| l.starts_with("<!--") || l.starts_with("# ") || l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    format!("{HEADER}\n{}\n", body.trim_end())
}

#[test]
fn the_cli_reference_page_matches_the_clap_definition() {
    let want = rendered();
    if std::env::var_os("UPDATE_CLI_REFERENCE").is_some() {
        std::fs::write(PAGE, &want).expect("write the page");
        return;
    }
    let have = std::fs::read_to_string(PAGE).expect("read the page");
    assert!(
        have == want,
        "docs/cli-reference.md differs from the clap definition. Rewrite it:\n  UPDATE_CLI_REFERENCE=1 cargo test --test cli_reference"
    );
}
