//! Diagnostics with the Godot gogo's G-codes where one exists. At build
//! time they fail `cargo build` (file:line); at hot reload the overlay
//! shows them.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub line: u32,
    pub message: String,
}

impl Diagnostic {
    pub fn error(code: &str, line: u32, message: impl Into<String>) -> Diagnostic {
        Diagnostic { severity: Severity::Error, code: code.to_string(), line, message: message.into() }
    }

    pub fn warning(code: &str, line: u32, message: impl Into<String>) -> Diagnostic {
        Diagnostic { severity: Severity::Warning, code: code.to_string(), line, message: message.into() }
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sev = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(f, "{sev} {} line {}: {}", self.code, self.line, self.message)
    }
}

pub fn has_errors(diags: &[Diagnostic]) -> bool {
    diags.iter().any(|d| d.severity == Severity::Error)
}
