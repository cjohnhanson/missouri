use std::io::Write;
use std::process::Stdio;

use camino::{Utf8Path, Utf8PathBuf};
use clap::Parser;
use miette::IntoDiagnostic;

#[derive(Parser)]
#[command(
    name = "missouri",
    version,
    about = "Show-me-state: e2e testing as directed graphs of filesystem states",
    max_term_width = 98
)]
pub struct Args {
    /// Change to this directory before doing anything
    #[arg(short = 'C', global = true)]
    pub directory: Option<Utf8PathBuf>,

    /// Name of the config directory (default: .missouri)
    #[arg(long, global = true, default_value = ".missouri")]
    pub config_dir: String,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Parser)]
pub enum Command {
    /// Run all test paths
    Run(RunArgs),

    /// Write the man pages into a directory (the package build uses this)
    #[command(hide = true)]
    GenMan {
        /// Output directory for the section-1 pages
        dir: Utf8PathBuf,
    },

    /// Print a shell completion script (the package build uses this)
    #[command(hide = true)]
    GenCompletions {
        /// Target shell
        shell: clap_complete::Shell,
    },

    /// List states, transitions, or test paths
    List(ListArgs),

    /// Validate missouri.yml files without running
    Validate(ValidateArgs),

    /// Initialize a new missouri project
    Init(InitArgs),

    /// Manage states
    State(StateArgs),

    /// Generate a report from recorded runs
    Report(ReportArgs),

    /// Serve an HTML report locally
    Serve(ServeArgs),

    /// Browse bundled documentation
    Docs(DocsArgs),

    /// Generate documentation from test suites
    #[command(name = "docgen")]
    Doc(DocArgs),

    /// Agent evaluation commands
    Agent(AgentArgs),
}

#[derive(Parser)]
pub struct DocsArgs {
    /// Topic slug to display, or "search" to search
    pub topic: Option<String>,

    /// Search query (when topic is "search")
    pub query: Option<String>,
}

#[derive(Parser)]
pub struct DocArgs {
    /// Root directory containing states
    #[arg(short, long, default_value = ".")]
    pub dir: Utf8PathBuf,

    /// Output format
    #[arg(long, default_value = "markdown")]
    pub format: DocFormat,

    /// Path index to render (1-based, default: 1)
    #[arg(long, default_value = "1")]
    pub path: usize,
}

#[derive(Clone, clap::ValueEnum)]
pub enum DocFormat {
    Markdown,
    Json,
}

#[derive(Parser)]
pub struct RunArgs {
    /// Root directory containing states
    #[arg(short, long, default_value = ".")]
    pub dir: Utf8PathBuf,

    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Suppress non-essential output
    #[arg(short, long)]
    pub quiet: bool,

    /// Keep temp directories after run (for debugging)
    #[arg(long)]
    pub keep_temp: bool,

    /// Run only state assertions (skip transitions and filesystem comparison)
    #[arg(long, conflicts_with_all = ["no_check", "record"])]
    pub check_only: bool,

    /// Skip all assertions (run only transitions and filesystem comparison)
    #[arg(long, conflicts_with = "check_only")]
    pub no_check: bool,

    /// Record transition output to asciicast files
    #[arg(long, conflicts_with = "check_only")]
    pub record: bool,

    /// Custom run ID for recording output directory (default: timestamp)
    #[arg(long, requires = "record")]
    pub run_id: Option<String>,
}

#[derive(Parser)]
pub struct ListArgs {
    /// Root directory containing states
    #[arg(short, long, default_value = ".")]
    pub dir: Utf8PathBuf,

    /// What to list
    #[arg(long, default_value = "paths")]
    pub show: ListKind,
}

#[derive(Clone, clap::ValueEnum)]
pub enum ListKind {
    States,
    Transitions,
    Paths,
    Graph,
}

#[derive(Parser)]
pub struct ValidateArgs {
    /// Root directory containing states
    #[arg(short, long, default_value = ".")]
    pub dir: Utf8PathBuf,
}

#[derive(Parser)]
pub struct InitArgs {
    /// Root directory for the project
    #[arg(short, long, default_value = ".")]
    pub dir: Utf8PathBuf,
}

#[derive(Parser)]
pub struct StateArgs {
    #[command(subcommand)]
    pub command: StateCommand,
}

#[derive(Parser)]
pub enum StateCommand {
    /// Add a new state
    Add(StateAddArgs),
}

#[derive(Parser)]
pub struct StateAddArgs {
    /// Name of the new state
    pub name: String,

    /// Root directory containing states
    #[arg(short, long, default_value = ".")]
    pub dir: Utf8PathBuf,

    /// Copy from an existing state and create a placeholder transition
    #[arg(long)]
    pub from: Option<String>,
}

#[derive(Parser)]
pub struct ReportArgs {
    /// Root directory containing states
    #[arg(short, long, default_value = ".")]
    pub dir: Utf8PathBuf,

    /// Report format
    #[arg(long, default_value = "terminal")]
    pub format: ReportFormat,

    /// Specific run ID to report on (default: latest)
    #[arg(long)]
    pub run: Option<String>,
}

#[derive(Clone, clap::ValueEnum)]
pub enum ReportFormat {
    Terminal,
    Html,
    Md,
}

#[derive(Parser)]
pub struct ServeArgs {
    /// Root directory containing states
    #[arg(short, long, default_value = ".")]
    pub dir: Utf8PathBuf,

    /// Specific run ID to serve (default: latest)
    #[arg(long)]
    pub run: Option<String>,

    /// Port to serve on
    #[arg(long, default_value = "8080")]
    pub port: u16,
}

#[derive(Parser)]
pub struct AgentArgs {
    #[command(subcommand)]
    pub command: AgentCommand,
}

#[derive(Parser)]
pub enum AgentCommand {
    /// Run an agent evaluation
    Eval(AgentEvalArgs),

    /// Record a passing verdict (called by the agent)
    Pass,

    /// Record a failing verdict (called by the agent)
    Fail(AgentFailArgs),
}

#[derive(Parser)]
pub struct AgentEvalArgs {
    /// Name of the eval (matches .missouri/<name>.md)
    pub name: String,

    /// Root directory containing the state
    #[arg(short, long, default_value = ".")]
    pub dir: Utf8PathBuf,
}

#[derive(Parser)]
pub struct AgentFailArgs {
    /// Failure details
    pub details: Vec<String>,
}

/// Run missouri with the given arguments. Handles -C directory change.
pub fn run(args: Args) -> miette::Result<bool> {
    if let Some(dir) = &args.directory {
        std::env::set_current_dir(dir.as_std_path()).into_diagnostic()?;
    }
    run_command(&args.config_dir, args.command)
}

/// Run a missouri subcommand with the given config directory.
pub fn run_command(config_dir: &str, command: Command) -> miette::Result<bool> {
    match command {
        Command::GenMan { dir } => {
            use clap::CommandFactory as _;
            std::fs::create_dir_all(&dir).into_diagnostic()?;
            crate::mangen::write_man_pages(&Args::command(), dir.as_std_path())
                .into_diagnostic()?;
            Ok(true)
        }

        Command::GenCompletions { shell } => {
            use clap::CommandFactory as _;
            clap_complete::generate(
                shell,
                &mut Args::command(),
                "missouri",
                &mut std::io::stdout(),
            );
            Ok(true)
        }

        Command::Run(run_args) => {
            let dir = resolve_dir(&run_args.dir)?;

            // Workspace mode. When the config sets members, run each member on its own.
            if let Some(members) =
                crate::graph::load_workspace_members(&dir, config_dir).into_diagnostic()?
            {
                return run_workspace_members(&members, &dir, config_dir, &run_args);
            }

            let graph = crate::graph::StateGraph::discover(&dir, config_dir).into_diagnostic()?;

            let roots = graph.roots();
            if roots.is_empty() {
                return Err(crate::error::Error::NoRoots.into());
            }

            let paths = crate::paths::enumerate_subgraph_paths(&graph);
            if paths.is_empty() {
                eprintln!("no test paths found");
                return Ok(true);
            }

            let sandbox =
                crate::executor::detect_sandbox(&graph).map_err(|e| miette::miette!("{e}"))?;

            let check_mode = if run_args.check_only {
                crate::executor::CheckMode::CheckOnly
            } else if run_args.no_check {
                crate::executor::CheckMode::NoCheck
            } else {
                crate::executor::CheckMode::Full
            };

            let recording = if run_args.record {
                let run_id = run_args.run_id.unwrap_or_else(|| {
                    chrono::Local::now().format("%Y-%m-%dT%H-%M-%S").to_string()
                });
                let output_dir = dir.join(config_dir).join("runs").join(&run_id);
                Some(crate::executor::RecordingConfig { output_dir, run_id })
            } else {
                None
            };

            let opts = crate::executor::RunOptions {
                keep_temp: run_args.keep_temp,
                verbose: run_args.verbose > 0,
                sandbox,
                check_mode,
                recording: recording.clone(),
            };

            // Run setup commands before test paths (if any)
            if !graph.setup.is_empty() {
                let setup_results = crate::executor::run_setup_phase(&graph, &opts);
                let setup_passed =
                    crate::report::print_setup_results(&setup_results, run_args.verbose > 0);
                if !setup_passed {
                    return Ok(false);
                }
            }

            let mut progress = crate::report::ProgressReporter::new();
            progress.prepare(&paths, &graph);
            let results = crate::executor::run_all_paths(
                &graph,
                &paths,
                &opts,
                Some(&|event| progress.on_event(event)),
            );
            progress.finish();
            let all_passed = crate::report::print_results(&results, run_args.verbose > 0);

            // Write results.json if recording
            if let Some(rc) = &recording {
                let mut recorded_paths = Vec::new();
                for (path_idx, (path, result)) in paths.iter().zip(results.iter()).enumerate() {
                    let mut recorded_steps = Vec::new();
                    for (step_idx, step) in result.steps.iter().enumerate() {
                        recorded_steps.push(crate::recorder::RecordedStep {
                            index: step_idx,
                            transition_name: step.transition_name.clone(),
                            source: step.source_name.clone(),
                            target: step.target_name.clone(),
                            passed: step.passed,
                            exit_code: step.exit_code,
                            cast_file: format!("path-{path_idx}/step-{step_idx}.cast"),
                        });
                    }
                    recorded_paths.push(crate::recorder::RecordedPath {
                        name: path.display(&graph),
                        passed: result.passed,
                        steps: recorded_steps,
                    });
                }

                let run_results = crate::recorder::RunResults {
                    run_id: rc.run_id.clone(),
                    passed: results.iter().filter(|r| r.passed).count(),
                    failed: results.iter().filter(|r| !r.passed).count(),
                    paths: recorded_paths,
                };

                crate::recorder::write_results(&rc.output_dir, &run_results).into_diagnostic()?;
            }

            Ok(all_passed)
        }
        Command::List(list_args) => {
            let dir = resolve_dir(&list_args.dir)?;

            if let Some(members) =
                crate::graph::load_workspace_members(&dir, config_dir).into_diagnostic()?
            {
                return list_workspace_members(&members, &dir, config_dir, &list_args);
            }

            let graph = crate::graph::StateGraph::discover(&dir, config_dir).into_diagnostic()?;

            match list_args.show {
                ListKind::States => crate::report::print_states(&graph),
                ListKind::Transitions => crate::report::print_transitions(&graph),
                ListKind::Paths | ListKind::Graph => {
                    let paths = crate::paths::enumerate_subgraph_paths(&graph);
                    crate::report::print_paths(&paths, &graph);
                }
            }
            Ok(true)
        }
        Command::Validate(validate_args) => {
            let dir = resolve_dir(&validate_args.dir)?;

            if let Some(members) =
                crate::graph::load_workspace_members(&dir, config_dir).into_diagnostic()?
            {
                return validate_workspace_members(&members, &dir, config_dir);
            }

            let graph = crate::graph::StateGraph::discover(&dir, config_dir).into_diagnostic()?;

            let roots = graph.roots();
            if roots.is_empty() {
                return Err(crate::error::Error::NoRoots.into());
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
            crate::scaffold::init_project(&dir, config_dir).into_diagnostic()?;
            println!("initialized missouri project at {}", dir.join(config_dir));
            Ok(true)
        }
        Command::State(state_args) => match state_args.command {
            StateCommand::Add(add_args) => {
                let dir = resolve_dir(&add_args.dir)?;
                crate::scaffold::add_state(
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
                crate::recorder::find_run_dir(&dir, config_dir, report_args.run.as_deref())?;

            match report_args.format {
                ReportFormat::Terminal => {
                    crate::recorder::print_terminal_report(&run_dir).into_diagnostic()?;
                }
                ReportFormat::Html => {
                    let html = crate::recorder::generate_html_report(&run_dir).into_diagnostic()?;
                    let report_path = run_dir.join("report.html");
                    std::fs::write(&report_path, &html).into_diagnostic()?;
                    println!("HTML report written to {report_path}");
                }
                ReportFormat::Md => {
                    let md = crate::recorder::generate_md_report(&run_dir).into_diagnostic()?;
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
                crate::recorder::find_run_dir(&dir, config_dir, serve_args.run.as_deref())?;
            // Serve is a placeholder. It only checks that a run exists.
            println!("serving on http://localhost:{}", serve_args.port);
            Ok(true)
        }

        Command::Docs(args) => match args.topic.as_deref() {
            None | Some("list") => {
                crate::docs::list();
                Ok(true)
            }
            Some("search") => {
                let query = args.query.as_deref().unwrap_or("");
                if query.is_empty() {
                    eprintln!("usage: missouri docs search <query>");
                    return Ok(false);
                }
                crate::docs::search(query);
                Ok(true)
            }
            Some(identifier) => {
                if crate::docs::show(identifier) {
                    Ok(true)
                } else {
                    eprintln!("unknown doc: {identifier}");
                    eprintln!();
                    crate::docs::list();
                    Ok(false)
                }
            }
        },

        Command::Agent(agent_args) => match agent_args.command {
            AgentCommand::Pass => {
                let cwd = std::env::current_dir().into_diagnostic()?;
                crate::agent_eval::write_pass(&cwd).into_diagnostic()?;
                Ok(true)
            }
            AgentCommand::Fail(fail_args) => {
                let cwd = std::env::current_dir().into_diagnostic()?;
                let details = fail_args.details.join(" ");
                crate::agent_eval::write_fail(&cwd, &details).into_diagnostic()?;
                Ok(true)
            }
            AgentCommand::Eval(eval_args) => run_agent_eval(&eval_args, config_dir),
        },

        Command::Doc(doc_args) => {
            let dir = resolve_dir(&doc_args.dir)?;

            let graph = crate::graph::StateGraph::discover(&dir, config_dir).into_diagnostic()?;
            let roots = graph.roots();
            if roots.is_empty() {
                return Err(crate::error::Error::NoRoots.into());
            }

            let paths = crate::paths::enumerate_subgraph_paths(&graph);
            if paths.is_empty() {
                eprintln!("no test paths found");
                return Ok(true);
            }

            let idx = doc_args.path.saturating_sub(1);
            if idx >= paths.len() {
                eprintln!(
                    "path {} not found (have {} paths)",
                    doc_args.path,
                    paths.len()
                );
                return Ok(false);
            }
            let path = &paths[idx];

            match doc_args.format {
                DocFormat::Markdown => {
                    let md = crate::docgen::render_markdown(&graph, path);
                    print!("{md}");
                }
                DocFormat::Json => {
                    let json = crate::docgen::render_json(&graph, path);
                    println!("{}", json);
                }
            }
            Ok(true)
        }
    }
}

/// Run an agent evaluation. Start a Claude agent with the eval prompt and
/// wait for it to write a verdict sentinel. Exit 0 on pass and 1 on fail.
fn run_agent_eval(eval_args: &AgentEvalArgs, config_dir: &str) -> miette::Result<bool> {
    let dir = resolve_dir(&eval_args.dir)?;
    let (spec, body) = crate::agent_eval::load_eval(&dir, config_dir, &eval_args.name)
        .map_err(|e| miette::miette!("{e}"))?;

    // Build the initial prompt: preamble + markdown body from the eval file.
    let preamble = format!(
        "Working directory: {dir}\n\n\
         When evaluation is complete, call `missouri agent pass` or \
         `missouri agent fail <details>` to record the verdict.\n\n\
         ---\n\n",
    );
    let initial_prompt = format!("{preamble}{body}");

    let defaults = crate::agent_cli::AgentDefaults {
        model: "sonnet".to_string(),
        system_prompt: "You are an evaluation agent for missouri. Your job is to evaluate \
             the state described in the prompt, then call `missouri agent pass` \
             or `missouri agent fail <details>`. Do not stop without rendering \
             a verdict."
            .to_string(),
        initial_prompt: initial_prompt.clone(),
        extra_args: vec![],
        allowed_tools: vec![
            "Bash(missouri agent*)".to_string(),
            "Read".to_string(),
            "Glob".to_string(),
            "Grep".to_string(),
        ],
    };

    let agent_config = spec.to_agent_config(&defaults);
    let mut cmd = crate::agent_cli::build_start_command(&agent_config, dir.as_std_path());

    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::inherit());

    eprintln!("eval: {}", eval_args.name);
    eprintln!("model: {}", agent_config.model);

    let mut child = cmd
        .spawn()
        .into_diagnostic()
        .map_err(|e| miette::miette!("failed to spawn agent: {e}"))?;

    // Send the initial prompt via stream-json stdin protocol.
    if let Some(mut stdin) = child.stdin.take() {
        let input = crate::agent_cli::InputMessage::user(&initial_prompt);
        let json = serde_json::to_string(&input).into_diagnostic()?;
        let _ = writeln!(stdin, "{json}");
        let _ = stdin.flush();
        // Drop stdin to signal EOF — the agent reads the prompt and runs.
        drop(stdin);
    }

    let status = child.wait().into_diagnostic()?;
    eprintln!("agent exited with status: {}", status);

    // Read the verdict sentinel.
    match crate::agent_eval::read_verdict(dir.as_std_path()) {
        Some(verdict) => {
            if verdict.passed {
                eprintln!("verdict: PASS");
                Ok(true)
            } else {
                let details = verdict.details.as_deref().unwrap_or("(no details)");
                eprintln!("verdict: FAIL — {details}");
                Ok(false)
            }
        }
        None => {
            eprintln!("verdict: agent did not write a verdict");
            Ok(false)
        }
    }
}

/// Build a short member label from the directory path. Use the directory
/// basename. Use the last two components when the basename is generic.
fn member_label(path: &Utf8Path, workspace_root: &Utf8Path) -> String {
    path.strip_prefix(workspace_root)
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|_| path.file_name().unwrap_or(path.as_str()).to_string())
}

/// Print a member section header.
fn print_member_header(label: &str) {
    println!("── {label} ──");
}

/// Run all test paths for each workspace member, printing per-member results.
fn run_workspace_members(
    members: &[Utf8PathBuf],
    workspace_root: &Utf8Path,
    config_dir: &str,
    run_args: &RunArgs,
) -> miette::Result<bool> {
    let mut all_passed = true;

    for member_dir in members {
        let label = member_label(member_dir, workspace_root);
        print_member_header(&label);

        let graph = crate::graph::StateGraph::discover(member_dir, config_dir).into_diagnostic()?;

        let roots = graph.roots();
        if roots.is_empty() {
            return Err(crate::error::Error::NoRoots.into());
        }

        let paths = crate::paths::enumerate_subgraph_paths(&graph);
        if paths.is_empty() {
            eprintln!("no test paths found in {label}");
            continue;
        }

        let sandbox =
            crate::executor::detect_sandbox(&graph).map_err(|e| miette::miette!("{e}"))?;

        let check_mode = if run_args.check_only {
            crate::executor::CheckMode::CheckOnly
        } else if run_args.no_check {
            crate::executor::CheckMode::NoCheck
        } else {
            crate::executor::CheckMode::Full
        };

        let opts = crate::executor::RunOptions {
            keep_temp: run_args.keep_temp,
            verbose: run_args.verbose > 0,
            sandbox,
            check_mode,
            recording: None,
        };

        // Run setup commands before test paths (if any)
        if !graph.setup.is_empty() {
            let setup_results = crate::executor::run_setup_phase(&graph, &opts);
            let setup_passed =
                crate::report::print_setup_results(&setup_results, run_args.verbose > 0);
            if !setup_passed {
                all_passed = false;
                continue;
            }
        }

        let mut progress = crate::report::ProgressReporter::new();
        progress.prepare(&paths, &graph);
        let results = crate::executor::run_all_paths(
            &graph,
            &paths,
            &opts,
            Some(&|event| progress.on_event(event)),
        );
        progress.finish();
        let member_passed = crate::report::print_results(&results, run_args.verbose > 0);

        if !member_passed {
            all_passed = false;
        }
    }

    Ok(all_passed)
}

/// List states/transitions/paths for each workspace member.
fn list_workspace_members(
    members: &[Utf8PathBuf],
    workspace_root: &Utf8Path,
    config_dir: &str,
    list_args: &ListArgs,
) -> miette::Result<bool> {
    for member_dir in members {
        let label = member_label(member_dir, workspace_root);
        print_member_header(&label);

        let graph = crate::graph::StateGraph::discover(member_dir, config_dir).into_diagnostic()?;

        match list_args.show {
            ListKind::States => crate::report::print_states(&graph),
            ListKind::Transitions => crate::report::print_transitions(&graph),
            ListKind::Paths | ListKind::Graph => {
                let paths = crate::paths::enumerate_subgraph_paths(&graph);
                crate::report::print_paths(&paths, &graph);
            }
        }
    }
    Ok(true)
}

/// Validate each workspace member.
fn validate_workspace_members(
    members: &[Utf8PathBuf],
    workspace_root: &Utf8Path,
    config_dir: &str,
) -> miette::Result<bool> {
    for member_dir in members {
        let label = member_label(member_dir, workspace_root);
        let graph = crate::graph::StateGraph::discover(member_dir, config_dir).into_diagnostic()?;

        let roots = graph.roots();
        if roots.is_empty() {
            return Err(crate::error::Error::NoRoots.into());
        }

        println!(
            "{label}: valid: {} state(s), {} transition(s), {} root(s)",
            graph.states.len(),
            graph.transitions.len(),
            roots.len()
        );
    }
    Ok(true)
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
