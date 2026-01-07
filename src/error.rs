use std::fmt;

#[derive(Debug)]
pub enum LauncherError {
    NiriConnection(String),
    NiriRequest(String),
    DesktopEntry(String),
    Io(std::io::Error),
    ParseInt(std::num::ParseIntError),
}

impl fmt::Display for LauncherError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LauncherError::NiriConnection(msg) => write!(f, "Niri connection error: {}", msg),
            LauncherError::NiriRequest(msg) => write!(f, "Niri request error: {}", msg),
            LauncherError::DesktopEntry(msg) => write!(f, "Desktop entry error: {}", msg),
            LauncherError::Io(err) => write!(f, "IO error: {}", err),
            LauncherError::ParseInt(err) => write!(f, "Parse error: {}", err),
        }
    }
}

impl std::error::Error for LauncherError {}

impl From<std::io::Error> for LauncherError {
    fn from(err: std::io::Error) -> Self {
        LauncherError::Io(err)
    }
}

impl From<std::num::ParseIntError> for LauncherError {
    fn from(err: std::num::ParseIntError) -> Self {
        LauncherError::ParseInt(err)
    }
}

pub type Result<T> = std::result::Result<T, LauncherError>;
