//! Message types and severity levels for the notification system.
//!
//! Defines the core [`Severity`] enum and [`Message`] struct used throughout
//! katachi for classifying and routing UI messages.

use std::fmt;
use std::time::Duration;

/// Severity level for a notification message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    /// Informational — neutral, auto-dismisses quickly.
    Info,
    /// Warning — something may need attention.
    Warn,
    /// Error — something went wrong.
    Error,
}

impl Severity {
    /// Default display duration for this severity level.
    #[must_use]
    pub const fn default_duration(self) -> Duration {
        match self {
            Self::Info => Duration::from_millis(3000),
            Self::Warn => Duration::from_millis(5000),
            Self::Error => Duration::from_millis(8000),
        }
    }

    /// Neovim highlight group name for this severity.
    #[must_use]
    pub const fn highlight_group(self) -> &'static str {
        match self {
            Self::Info => "KatachiInfo",
            Self::Warn => "KatachiWarn",
            Self::Error => "KatachiError",
        }
    }

    /// Icon prefix for display.
    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Info => " ",
            Self::Warn => " ",
            Self::Error => " ",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Info => write!(f, "info"),
            Self::Warn => write!(f, "warn"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// Source of a message, used for routing decisions.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MessageSource {
    /// From the LSP progress handler.
    LspProgress,
    /// Search count messages (e.g., `[1/5]`).
    SearchCount,
    /// Neovim's built-in message system (`:messages`).
    Builtin,
    /// Plugin-generated notification.
    Plugin(String),
    /// Custom user-defined source.
    Custom(String),
}

impl fmt::Display for MessageSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LspProgress => write!(f, "lsp"),
            Self::SearchCount => write!(f, "search"),
            Self::Builtin => write!(f, "builtin"),
            Self::Plugin(name) => write!(f, "plugin:{name}"),
            Self::Custom(name) => write!(f, "custom:{name}"),
        }
    }
}

/// A notification message with content, severity, and routing metadata.
#[derive(Debug, Clone)]
pub struct Message {
    /// Unique ID for this message instance.
    pub id: u64,
    /// The message text content.
    pub content: String,
    /// Severity level.
    pub severity: Severity,
    /// Where this message originated.
    pub source: MessageSource,
    /// How long this message should display before auto-dismissing.
    /// `None` means use the severity default.
    pub duration: Option<Duration>,
    /// Optional title shown above the message body.
    pub title: Option<String>,
}

impl Message {
    /// Create a new message with the given severity and content.
    #[must_use]
    pub fn new(severity: Severity, content: &str) -> Self {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self {
            id: NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            content: content.to_string(),
            severity,
            source: MessageSource::Builtin,
            duration: None,
            title: None,
        }
    }

    /// Shorthand for an info message.
    #[must_use]
    pub fn info(content: &str) -> Self {
        Self::new(Severity::Info, content)
    }

    /// Shorthand for a warning message.
    #[must_use]
    pub fn warn(content: &str) -> Self {
        Self::new(Severity::Warn, content)
    }

    /// Shorthand for an error message.
    #[must_use]
    pub fn error(content: &str) -> Self {
        Self::new(Severity::Error, content)
    }

    /// Set the message source.
    #[must_use]
    pub fn source(mut self, source: MessageSource) -> Self {
        self.source = source;
        self
    }

    /// Override the display duration.
    #[must_use]
    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    /// Set a title for the notification.
    #[must_use]
    pub fn title(mut self, title: &str) -> Self {
        self.title = Some(title.to_string());
        self
    }

    /// Effective display duration (custom or severity default).
    #[must_use]
    pub fn effective_duration(&self) -> Duration {
        self.duration.unwrap_or_else(|| self.severity.default_duration())
    }

    /// Format the message for display as one or more lines.
    #[must_use]
    pub fn render_lines(&self) -> Vec<String> {
        let icon = self.severity.icon();
        let mut lines = Vec::new();

        if let Some(ref title) = self.title {
            lines.push(format!("{icon}{title}"));
        }

        for line in self.content.lines() {
            if lines.is_empty() {
                lines.push(format!("{icon}{line}"));
            } else {
                lines.push(format!("  {line}"));
            }
        }

        if lines.is_empty() {
            lines.push(format!("{icon}(empty)"));
        }

        lines
    }

    /// Width needed to display this message (longest line length).
    #[must_use]
    pub fn display_width(&self) -> usize {
        self.render_lines()
            .iter()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_defaults() {
        assert_eq!(Severity::Info.default_duration(), Duration::from_millis(3000));
        assert_eq!(Severity::Warn.default_duration(), Duration::from_millis(5000));
        assert_eq!(Severity::Error.default_duration(), Duration::from_millis(8000));
    }

    #[test]
    fn message_ids_are_unique() {
        let m1 = Message::info("a");
        let m2 = Message::info("b");
        assert_ne!(m1.id, m2.id);
    }

    #[test]
    fn effective_duration_uses_custom() {
        let m = Message::info("hello").duration(Duration::from_secs(1));
        assert_eq!(m.effective_duration(), Duration::from_secs(1));
    }

    #[test]
    fn effective_duration_falls_back_to_severity() {
        let m = Message::warn("oops");
        assert_eq!(m.effective_duration(), Severity::Warn.default_duration());
    }

    #[test]
    fn render_lines_simple() {
        let m = Message::info("hello world");
        let lines = m.render_lines();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("hello world"));
    }

    #[test]
    fn render_lines_with_title() {
        let m = Message::error("details here").title("Oh no");
        let lines = m.render_lines();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("Oh no"));
        assert!(lines[1].contains("details"));
    }

    #[test]
    fn render_lines_multiline() {
        let m = Message::info("line1\nline2\nline3");
        let lines = m.render_lines();
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn display_width_accounts_for_icon() {
        let m = Message::info("test");
        let width = m.display_width();
        // Icon + space + "test" should be wider than just "test"
        assert!(width > 4);
    }

    #[test]
    fn message_source_display() {
        assert_eq!(MessageSource::LspProgress.to_string(), "lsp");
        assert_eq!(MessageSource::SearchCount.to_string(), "search");
        assert_eq!(MessageSource::Plugin("foo".into()).to_string(), "plugin:foo");
    }

    #[test]
    fn severity_display() {
        assert_eq!(Severity::Info.to_string(), "info");
        assert_eq!(Severity::Warn.to_string(), "warn");
        assert_eq!(Severity::Error.to_string(), "error");
    }

    #[test]
    fn severity_highlight_groups() {
        assert_eq!(Severity::Info.highlight_group(), "KatachiInfo");
        assert_eq!(Severity::Warn.highlight_group(), "KatachiWarn");
        assert_eq!(Severity::Error.highlight_group(), "KatachiError");
    }
}
