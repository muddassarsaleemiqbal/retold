use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum EngineError {
    #[error("cancelled; saved work and unresolved provider holds were retained")]
    Cancelled,
    #[error("{0}")]
    Setup(String),

    #[error("could not use {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{path}: {message}")]
    Config { path: PathBuf, message: String },

    #[error("template error: {0}")]
    Template(String),

    #[error("a video already exists at {}", path.display())]
    #[diagnostic(help(
        "see it with `retold status {slug}`, or pick a different title, year or part"
    ))]
    RunExists { path: PathBuf, slug: String },

    #[error(
        "{} is being worked on by another retold process{}",
        path.display(),
        holder.as_ref().map(|h| format!(" ({h})")).unwrap_or_default()
    )]
    #[diagnostic(help("wait for it to finish, or stop it with Ctrl-C in its terminal"))]
    Locked {
        path: PathBuf,
        holder: Option<String>,
    },
}

impl EngineError {
    pub(crate) fn io(path: &Path, source: std::io::Error) -> Self {
        Self::Io {
            path: path.to_owned(),
            source,
        }
    }
}
