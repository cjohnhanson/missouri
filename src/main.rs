use std::process::ExitCode;

use camino::Utf8PathBuf;
use clap::Parser;
use miette::IntoDiagnostic;

mod cli;

use cli::{Args, Command, ListKind};

fn main() -> ExitCode {
    let args = Args::parse();

    // Set up miette for nice error rendering
    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(true)
                .unicode(true)
                .color(true)
                .build(),
        )
    }))
    .ok(); // ignore if already set

    match run(args) {
        Ok(success) => {
            if success {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => {
            eprintln!("{:?}", e);
            ExitCode::from(2)
        }
    }
}

fn run(args: Args) -> miette::Result<bool> {
    let config_dir = &args.config_dir;
    match args.command {
        Command::Run(run_args) => {
            let dir = resolve_dir(&run_args.dir)?;
            let graph =
                missouri::graph::StateGraph::discover(&dir, config_dir).into_diagnostic()?;

            let roots = graph.roots();
            if roots.is_empty() {
                return Err(missouri::error::Error::NoRoots.into());
            }

            let paths = missouri::paths::enumerate_paths(&graph);
            if paths.is_empty() {
                eprintln!("no test paths found");
                return Ok(true);
            }

            let sandbox =
                missouri::executor::detect_sandbox(&graph).map_err(|e| miette::miette!("{e}"))?;

            let check_mode = if run_args.check_only {
                missouri::executor::CheckMode::CheckOnly
            } else if run_args.no_check {
                missouri::executor::CheckMode::NoCheck
            } else {
                missouri::executor::CheckMode::Full
            };

            let opts = missouri::executor::RunOptions {
                keep_temp: run_args.keep_temp,
                verbose: run_args.verbose > 0,
                sandbox,
                check_mode,
            };

            let results = missouri::executor::run_all_paths(&graph, &paths, &opts);
            let all_passed = missouri::report::print_results(&results, run_args.verbose > 0);
            Ok(all_passed)
        }
        Command::List(list_args) => {
            let dir = resolve_dir(&list_args.dir)?;
            let graph =
                missouri::graph::StateGraph::discover(&dir, config_dir).into_diagnostic()?;

            match list_args.show {
                ListKind::States => missouri::report::print_states(&graph),
                ListKind::Transitions => missouri::report::print_transitions(&graph),
                ListKind::Paths | ListKind::Graph => {
                    let paths = missouri::paths::enumerate_paths(&graph);
                    missouri::report::print_paths(&paths, &graph);
                }
            }
            Ok(true)
        }
        Command::Validate(validate_args) => {
            let dir = resolve_dir(&validate_args.dir)?;
            let graph =
                missouri::graph::StateGraph::discover(&dir, config_dir).into_diagnostic()?;

            let roots = graph.roots();
            if roots.is_empty() {
                return Err(missouri::error::Error::NoRoots.into());
            }

            println!(
                "valid: {} state(s), {} transition(s), {} root(s)",
                graph.states.len(),
                graph.transitions.len(),
                roots.len()
            );
            Ok(true)
        }
    }
}

fn resolve_dir(dir: &Utf8PathBuf) -> miette::Result<camino::Utf8PathBuf> {
    let path = if dir.is_relative() {
        let cwd = std::env::current_dir().into_diagnostic()?;
        Utf8PathBuf::try_from(cwd).into_diagnostic()?.join(dir)
    } else {
        dir.clone()
    };
    Ok(path)
}
