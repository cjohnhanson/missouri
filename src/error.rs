use camino::Utf8PathBuf;
use miette::Diagnostic;
use thiserror::Error;

#[derive(Error, Debug, Diagnostic)]
pub enum Error {
    #[error("config file not found: {path}")]
    #[diagnostic(
        code(missouri::config::not_found),
        help("each state directory must contain .missouri/missouri.yml")
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

    #[error("transition target not found: {target} (from {from_state})")]
    #[diagnostic(
        code(missouri::graph::missing_target),
        help("the target path must point to a directory containing .missouri/missouri.yml")
    )]
    MissingTarget {
        from_state: Utf8PathBuf,
        target: Utf8PathBuf,
    },

    #[error("no entry points found (all states have inbound transitions)")]
    #[diagnostic(
        code(missouri::graph::no_roots),
        help("at least one state must have no inbound transitions to serve as a test entry point")
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
        help("check glob syntax in <config_dir>/ignore")
    )]
    IgnorePattern { pattern: String, detail: String },

    #[error(".flox/ found at {root} but `flox` binary not found on PATH")]
    #[diagnostic(
        code(missouri::sandbox::flox_not_found),
        help("install flox (https://flox.dev) or remove the .flox/ directory")
    )]
    FloxNotFound { root: Utf8PathBuf },

    #[error(transparent)]
    #[diagnostic(code(missouri::io))]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    #[diagnostic(code(missouri::walkdir))]
    WalkDir(#[from] walkdir::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
