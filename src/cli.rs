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
