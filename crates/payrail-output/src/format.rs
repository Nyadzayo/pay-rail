use crate::colors;
use crate::config::OutputConfig;

/// Truncate a line to fit within the terminal width.
/// Appends an ellipsis character if truncation occurs.
fn truncate_to_width(text: &str, width: u16) -> String {
    let w = width as usize;
    if w == 0 || text.len() <= w {
        return text.to_string();
    }
    // Leave room for the ellipsis
    let truncated: String = text.chars().take(w.saturating_sub(1)).collect();
    format!("{truncated}\u{2026}")
}

/// Format a summary line (bold, colored) — the first visible output line.
pub fn summary_line(config: &OutputConfig, text: &str) -> String {
    let truncated = truncate_to_width(text, config.terminal_width);
    colors::emphasis(config, &truncated)
}

/// Format a section header (bold white).
pub fn section_header(config: &OutputConfig, text: &str) -> String {
    let inner = colors::emphasis(config, text);
    let line = format!("  {inner}");
    truncate_to_width(&line, config.terminal_width)
}

/// Format a detail line with 2-space indent per level.
pub fn detail_line(text: &str, indent_level: u8) -> String {
    let indent = "  ".repeat(indent_level as usize);
    format!("{indent}{text}")
}

/// Format a detail line with truncation based on terminal width.
pub fn detail_line_truncated(config: &OutputConfig, text: &str, indent_level: u8) -> String {
    let line = detail_line(text, indent_level);
    truncate_to_width(&line, config.terminal_width)
}

/// Format a help/suggestion line (cyan).
pub fn help_line(config: &OutputConfig, text: &str) -> String {
    let inner = format!("  {}", colors::accent(config, &format!("help: {text}")));
    truncate_to_width(&inner, config.terminal_width)
}

/// Format an error message following PayRail convention: `[WHAT] [WHY] [WHAT TO DO]`.
pub fn error_message(config: &OutputConfig, what: &str, why: &str, what_to_do: &str) -> String {
    let msg = format!("{what}: {why}\n  {}", colors::accent(config, what_to_do));
    colors::error(config, &msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain_config() -> OutputConfig {
        OutputConfig::from_env(false, true, false, false)
    }

    fn narrow_config() -> OutputConfig {
        OutputConfig {
            mode: crate::config::OutputMode::Text,
            color: crate::config::ColorMode::Never,
            verbosity: crate::config::Verbosity::Normal,
            terminal_width: 20,
        }
    }

    #[test]
    fn detail_line_indentation() {
        assert_eq!(detail_line("hello", 0), "hello");
        assert_eq!(detail_line("hello", 1), "  hello");
        assert_eq!(detail_line("hello", 2), "    hello");
        assert_eq!(detail_line("hello", 3), "      hello");
    }

    #[test]
    fn help_line_format() {
        let cfg = plain_config();
        let output = help_line(&cfg, "try again");
        assert!(output.contains("help: try again"));
    }

    #[test]
    fn error_message_contains_all_parts() {
        let cfg = plain_config();
        let output = error_message(&cfg, "Connection failed", "timeout", "check network");
        assert!(output.contains("Connection failed"));
        assert!(output.contains("timeout"));
        assert!(output.contains("check network"));
    }

    #[test]
    fn section_header_has_indent() {
        let cfg = plain_config();
        let output = section_header(&cfg, "Results");
        assert!(output.starts_with("  "));
        assert!(output.contains("Results"));
    }

    #[test]
    fn truncate_to_width_short_string_unchanged() {
        assert_eq!(truncate_to_width("hello", 80), "hello");
    }

    #[test]
    fn truncate_to_width_long_string_truncated() {
        let long = "a".repeat(100);
        let result = truncate_to_width(&long, 20);
        // 19 chars + ellipsis = 20 visible units
        assert!(result.len() <= 22); // ellipsis is 3 bytes in UTF-8
        assert!(result.ends_with('\u{2026}'));
    }

    #[test]
    fn summary_line_truncates_for_narrow_terminal() {
        let cfg = narrow_config();
        let long_text = "This is a very long summary that exceeds the width";
        let output = summary_line(&cfg, long_text);
        assert!(output.len() < long_text.len());
        assert!(output.contains('\u{2026}'));
    }

    #[test]
    fn detail_line_truncated_respects_width() {
        let cfg = narrow_config();
        let output = detail_line_truncated(&cfg, "a long detail line text here", 1);
        assert!(output.contains('\u{2026}'));
    }
}
