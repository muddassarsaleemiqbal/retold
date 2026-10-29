use miette::{Diagnostic, NamedSource, SourceSpan};

/// Everything that can go wrong loading a guide or channel. Each variant says which file is
/// at fault and, where possible, points at the line.
#[derive(Debug, thiserror::Error, Diagnostic)]
#[non_exhaustive]
pub enum LoadError {
    #[error("{origin} has no {file}")]
    #[diagnostic(
        code(retold::load::missing),
        help(
            "every guide folder needs a guide.toml; see `retold guides create <id>` for an example"
        )
    )]
    Missing { origin: String, file: &'static str },

    #[error("could not read {path}")]
    #[diagnostic(code(retold::load::io))]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("{file}: {message}")]
    #[diagnostic(code(retold::load::syntax))]
    Syntax {
        file: String,
        message: String,
        #[source_code]
        src: NamedSource<String>,
        #[label("{message}")]
        span: Option<SourceSpan>,
    },

    #[error("{file} uses format {found}, but this Retold understands up to {supported}")]
    #[diagnostic(
        code(retold::load::schema),
        help("update Retold, or use an older copy of this file")
    )]
    NewerSchema {
        file: String,
        found: u32,
        supported: u32,
    },

    #[error("{file} has {count} problem{s}", count = problems.len(), s = if problems.len() == 1 { "" } else { "s" })]
    #[diagnostic(code(retold::load::invalid))]
    Invalid {
        file: String,
        problems: Vec<String>,
        #[help]
        help: String,
    },
}

impl LoadError {
    pub(crate) fn syntax(file: &str, src: &str, err: &toml::de::Error) -> Self {
        Self::Syntax {
            file: file.to_owned(),
            message: err.message().to_owned(),
            src: NamedSource::new(file, src.to_owned()),
            span: err.span().map(Into::into),
        }
    }

    pub(crate) fn invalid(file: &str, problems: Vec<String>) -> Self {
        let help = problems
            .iter()
            .map(|p| format!("• {p}"))
            .collect::<Vec<_>>()
            .join("\n");
        Self::Invalid {
            file: file.to_owned(),
            problems,
            help,
        }
    }
}
