use std::fmt;

/// Runtime or syntax error raised by the engine. Mirrors the identifier/message pair
/// that `try/catch` exposes to user code.
#[derive(Debug, Clone)]
pub struct MError {
    pub identifier: String,
    pub message: String,
    /// "name (line N)" frames, innermost first.
    pub stack: Vec<String>,
    /// Function that raised the error ("Error using <origin>").
    pub origin: Option<String>,
    /// Raised by the Stop button / Ctrl+C; never caught by try/catch.
    pub interrupt: bool,
}

impl MError {
    pub fn new(msg: impl Into<String>) -> Self {
        MError {
            identifier: String::new(),
            message: msg.into(),
            stack: Vec::new(),
            origin: None,
            interrupt: false,
        }
    }
    pub fn with_id(id: impl Into<String>, msg: impl Into<String>) -> Self {
        MError {
            identifier: id.into(),
            message: msg.into(),
            stack: Vec::new(),
            origin: None,
            interrupt: false,
        }
    }
    pub fn interrupted() -> Self {
        MError {
            interrupt: true,
            ..MError::with_id("Beany:interrupt", "Operation terminated by user.")
        }
    }
    /// Text shown in the command window.
    pub fn report(&self) -> String {
        let mut s = String::new();
        if let Some(o) = self.origin.as_ref().filter(|o| !o.is_empty()) {
            s.push_str(&format!("Error using {}\n", o));
        }
        s.push_str(&self.message);
        for f in &self.stack {
            s.push_str(&format!("\n\nError in {}", f));
        }
        s
    }
    pub fn syntax(msg: impl Into<String>) -> Self {
        MError::with_id("Beany:syntax", msg)
    }
}

impl fmt::Display for MError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        for s in &self.stack {
            write!(f, "\nError in {}", s)?;
        }
        Ok(())
    }
}

pub type MResult<T> = Result<T, MError>;

#[macro_export]
macro_rules! merr {
    ($($arg:tt)*) => { $crate::error::MError::new(format!($($arg)*)) };
}
