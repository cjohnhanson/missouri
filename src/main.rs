use std::process::ExitCode;

use camino::Utf8PathBuf;
use clap::Parser;
use miette::IntoDiagnostic;

mod cli;

use cli::{Args, Command, ListKind, ReportFormat, StateCommand};

fn main() -> ExitCode {
    let args = Args::parse();

    ctrlc::set_handler(|| {
        if missouri::signal::is_interrupted() {
            // Second Ctrl+C: force exit
            missouri::signal::set_force_exit();
            missouri::signal::kill_all_children(libc::SIGKILL);
            std::process::exit(130);
        }
        // First Ctrl+C: graceful shutdown
        missouri::signal::set_interrupted();
        missouri::signal::kill_all_children(libc::SIGTERM);
    })
    .ok();

    // Set up miette for nice error render
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

    let result = run(args);

    // Exit 130 if interrupted (standard SIGINT convention)
    if missouri::signal::is_interrupted() {
        return ExitCode::from(130);
    }

    match result {
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
    if let Some(dir) = &args.directory {
        std::env::set_current_dir(dir.as_std_path()).into_diagnostic()?;
    }
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

            let recording = if run_args.record {
                let run_id = run_args.run_id.unwrap_or_else(|| {
                    chrono::Local::now().format("%Y-%m-%dT%H-%M-%S").to_string()
                });
                let output_dir = dir.join(config_dir).join("runs").join(&run_id);
                Some(missouri::executor::RecordingConfig { output_dir, run_id })
            } else {
                None
            };

            let opts = missouri::executor::RunOptions {
                keep_temp: run_args.keep_temp,
                verbose: run_args.verbose > 0,
                sandbox,
                check_mode,
                recording: recording.clone(),
            };

            // Run setup commands before test paths (if any)
            if !graph.setup.is_empty() {
                let setup_results = missouri::executor::run_setup_phase(&graph, &opts);
                let setup_passed =
                    missouri::report::print_setup_results(&setup_results, run_args.verbose > 0);
                if !setup_passed {
                    return Ok(false);
                }
            }

            let progress = missouri::report::ProgressReporter::new();
            let results = missouri::executor::run_all_paths(
                &graph,
                &paths,
                &opts,
                Some(&|event| progress.on_event(event)),
            );
            progress.finish();
            let all_passed = missouri::report::print_results(&results, run_args.verbose > 0);

            // Write results.json if recording
            if let Some(rc) = &recording {
                let mut recorded_paths = Vec::new();
                for (path_idx, (path, result)) in paths.iter().zip(results.iter()).enumerate() {
                    let mut recorded_steps = Vec::new();
                    for (step_idx, step) in result.steps.iter().enumerate() {
                        recorded_steps.push(missouri::recorder::RecordedStep {
                            index: step_idx,
                            transition_name: step.transition_name.clone(),
                            source: step.source_name.clone(),
                            target: step.target_name.clone(),
                            passed: step.passed,
                            exit_code: step.exit_code,
                            cast_file: format!("path-{path_idx}/step-{step_idx}.cast"),
                        });
                    }
                    recorded_paths.push(missouri::recorder::RecordedPath {
                        name: path.display(&graph),
                        passed: result.passed,
                        steps: recorded_steps,
                    });
                }

                let run_results = missouri::recorder::RunResults {
                    run_id: rc.run_id.clone(),
                    passed: results.iter().filter(|r| r.passed).count(),
                    failed: results.iter().filter(|r| !r.passed).count(),
                    paths: recorded_paths,
                };

                missouri::recorder::write_results(&rc.output_dir, &run_results)
                    .into_diagnostic()?;
            }

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
        Command::Init(init_args) => {
            let dir = resolve_dir(&init_args.dir)?;
            missouri::scaffold::init_project(&dir, config_dir).into_diagnostic()?;
            println!("initialized missouri project at {}", dir.join(config_dir));
            Ok(true)
        }
        Command::State(state_args) => match state_args.command {
            StateCommand::Add(add_args) => {
                let dir = resolve_dir(&add_args.dir)?;
                missouri::scaffold::add_state(
                    &dir,
                    config_dir,
                    &add_args.name,
                    add_args.from.as_deref(),
                )
                .into_diagnostic()?;
                println!("created state \"{}\"", add_args.name);
                Ok(true)
            }
        },
        Command::Report(report_args) => {
            let dir = resolve_dir(&report_args.dir)?;
            let run_dir =
                missouri::recorder::find_run_dir(&dir, config_dir, report_args.run.as_deref())?;

            match report_args.format {
                ReportFormat::Terminal => {
                    missouri::recorder::print_terminal_report(&run_dir).into_diagnostic()?;
                }
                ReportFormat::Html => {
                    let html =
                        missouri::recorder::generate_html_report(&run_dir).into_diagnostic()?;
                    let report_path = run_dir.join("report.html");
                    std::fs::write(&report_path, &html).into_diagnostic()?;
                    println!("HTML report written to {report_path}");
                }
                ReportFormat::Md => {
                    let md = missouri::recorder::generate_md_report(&run_dir).into_diagnostic()?;
                    let report_path = run_dir.join("report.md");
                    std::fs::write(&report_path, &md).into_diagnostic()?;
                    println!("Markdown report written to {report_path}");
                }
            }
            Ok(true)
        }
        Command::Serve(serve_args) => {
            let dir = resolve_dir(&serve_args.dir)?;
            let _run_dir =
                missouri::recorder::find_run_dir(&dir, config_dir, serve_args.run.as_deref())?;
            // Serve is a placeholder for now — just verify runs exist
            println!("serving on http://localhost:{}", serve_args.port);
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
