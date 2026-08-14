use camino::Utf8PathBuf;
use miette::Diagnostic;
use thiserror::Error;

#[derive(Error, Debug, Diagnostic)]
pub enum Error {
    #[error("config file not found: {path}")]
    #[diagnostic(
        code(missouri::config::not_found),
        help("each state directory must contain a .missouri/missouri.yml file")
    )]
    ConfigNotFound { path: Utf8PathBuf },

    #[error("failed to read config: {path}")]
    #[diagnostic(code(missouri::config::read_error))]
    ConfigRead {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid config: {path}")]
    #[diagnostic(code(missouri::config::parse_error))]
    ConfigParse {
        path: Utf8PathBuf,
        #[source]
        source: serde_yml::Error,
    },

    #[error("invalid config: {0}")]
    #[diagnostic(code(missouri::config::invalid))]
    InvalidConfig(String),

    #[error("transition target not found: {target} (from {from_state})")]
    #[diagnostic(
        code(missouri::graph::missing_target),
        help(
            "the target path must point to a directory that contains a .missouri/missouri.yml file"
        )
    )]
    MissingTarget {
        from_state: Utf8PathBuf,
        target: Utf8PathBuf,
    },

    #[error("no entry points found (all states have inbound transitions)")]
    #[diagnostic(
        code(missouri::graph::no_roots),
        help("at least one state must have no inbound transitions. that state is the test entry point")
    )]
    NoRoots,

    #[error("transition command failed with exit code {exit_code}")]
    #[diagnostic(code(missouri::exec::command_failed))]
    CommandFailed { exit_code: i32, stderr: String },

    #[error("comparator failed: {command}")]
    #[diagnostic(code(missouri::compare::comparator_failed))]
    ComparatorFailed { command: String, stderr: String },

    #[error("invalid ignore pattern \"{pattern}\": {detail}")]
    #[diagnostic(
        code(missouri::config::ignore_pattern),
        help("check the glob syntax in <config_dir>/ignore")
    )]
    IgnorePattern { pattern: String, detail: String },

    #[error("nix sandbox configured but `nix` binary not found on PATH (project root: {root})")]
    #[diagnostic(
        code(missouri::sandbox::nix_not_found),
        help("install nix (https://nixos.org), or remove the packages config from missouri.yml")
    )]
    NixNotFound { root: Utf8PathBuf },

    #[error("nix sandbox cache warm failed: {message}")]
    #[diagnostic(
        code(missouri::sandbox::warm_failed),
        help("check the nix installation and the network connection")
    )]
    SandboxWarm { message: String },

    #[error("project already initialized at {path}")]
    #[diagnostic(
        code(missouri::init::already_initialized),
        help("remove {path} to initialize the project again")
    )]
    AlreadyInitialized { path: Utf8PathBuf },

    #[error("state \"{name}\" already exists")]
    #[diagnostic(code(missouri::state::already_exists))]
    StateAlreadyExists { name: String },

    #[error("source state \"{name}\" not found")]
    #[diagnostic(
        code(missouri::state::source_not_found),
        help("the --from state must name an existing state directory")
    )]
    SourceStateNotFound { name: String },

    #[error("no recorded runs found")]
    #[diagnostic(
        code(missouri::report::no_runs),
        help("run `missouri run --record` first to make a recording")
    )]
    NoRecordedRuns,

    #[error(transparent)]
    #[diagnostic(code(missouri::io))]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    #[diagnostic(code(missouri::walkdir))]
    WalkDir(#[from] walkdir::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
