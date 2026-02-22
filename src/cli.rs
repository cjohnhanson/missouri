use camino::Utf8PathBuf;
use clap::Parser;

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
