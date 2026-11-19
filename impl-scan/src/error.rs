use std::fmt;
use std::path::PathBuf;

/// An error from scanning, checking or rendering.
///
/// When the problem has a source location it is printed rustc-style, so a failing
/// build script points at the line to fix:
///
/// ```text
/// error: duplicate name `MAX_FREE_SLOT` in group `strategies`
///   --> src/strategies/max_free_slot.rs:12
/// ```
#[derive(Debug)]
pub struct Error {
    message: String,
    file: Option<PathBuf>,
    line: Option<usize>,
    help: Option<String>,
}

impl Error {
    /// An error with a message only.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            file: None,
            line: None,
            help: None,
        }
    }

    /// Attaches a source location.
    pub fn at(mut self, file: impl Into<PathBuf>, line: Option<usize>) -> Self {
        self.file = Some(file.into());
        self.line = line;
        self
    }

    /// Attaches a hint printed after the location.
    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// The main message, without location or hint.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The file the error points at, if any.
    pub fn file(&self) -> Option<&std::path::Path> {
        self.file.as_deref()
    }

    /// The 1-based line the error points at, if any.
    pub fn line(&self) -> Option<usize> {
        self.line
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error: {}", self.message)?;
        if let Some(file) = &self.file {
            write!(f, "\n  --> {}", file.display())?;
            if let Some(line) = self.line {
                write!(f, ":{line}")?;
            }
        }
        if let Some(help) = &self.help {
            write!(f, "\n  = help: {help}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {}

impl From<handlebars::RenderError> for Error {
    fn from(err: handlebars::RenderError) -> Self {
        Error::new(format!("template rendering failed: {err}"))
    }
}

impl From<handlebars::TemplateError> for Error {
    fn from(err: handlebars::TemplateError) -> Self {
        Error::new(format!("template does not parse: {err}"))
    }
}
