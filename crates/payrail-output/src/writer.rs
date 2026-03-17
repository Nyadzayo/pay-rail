use serde::Serialize;

use crate::colors;
use crate::config::{OutputConfig, Verbosity};
use crate::symbols;

/// Trait for structured output that adapts to text vs JSON modes.
pub trait OutputWriter {
    /// Write a success result.
    fn write_success(&self, config: &OutputConfig, message: &str);

    /// Write a warning result.
    fn write_warning(&self, config: &OutputConfig, message: &str);

    /// Write an error result.
    fn write_error(&self, config: &OutputConfig, message: &str);

    /// Write a structured result as JSON (for --json mode) or formatted text.
    fn write_result<T: Serialize>(&self, config: &OutputConfig, result: &T);
}

/// Default implementation of OutputWriter that writes to stdout/stderr.
pub struct StdWriter;

impl StdWriter {
    /// Format a success message as a string without printing.
    pub fn format_success(config: &OutputConfig, message: &str) -> String {
        if config.is_json() {
            let val = serde_json::json!({
                "status": "success",
                "message": message,
            });
            serde_json::to_string(&val)
                .unwrap_or_else(|e| format!("{{\"error\":\"serialization failed: {e}\"}}"))
        } else {
            let sym = symbols::pass_symbol(config);
            colors::success(config, &format!("{sym} {message}"))
        }
    }

    /// Format a warning message as a string without printing.
    pub fn format_warning(config: &OutputConfig, message: &str) -> String {
        if config.is_json() {
            let val = serde_json::json!({
                "status": "warning",
                "message": message,
            });
            serde_json::to_string(&val)
                .unwrap_or_else(|e| format!("{{\"error\":\"serialization failed: {e}\"}}"))
        } else {
            let sym = symbols::verify_symbol(config);
            colors::warning(config, &format!("{sym} {message}"))
        }
    }

    /// Format an error message as a string without printing.
    pub fn format_error(config: &OutputConfig, message: &str) -> String {
        if config.is_json() {
            let val = serde_json::json!({
                "status": "error",
                "message": message,
            });
            serde_json::to_string(&val)
                .unwrap_or_else(|e| format!("{{\"error\":\"serialization failed: {e}\"}}"))
        } else {
            let sym = symbols::fail_symbol(config);
            colors::error(config, &format!("{sym} {message}"))
        }
    }

    /// Format a result as a string without printing.
    pub fn format_result<T: Serialize>(config: &OutputConfig, result: &T) -> Option<String> {
        if config.is_json() {
            Some(
                serde_json::to_string(result)
                    .unwrap_or_else(|e| format!("{{\"error\":\"serialization failed: {e}\"}}")),
            )
        } else if config.verbosity >= Verbosity::Normal {
            Some(
                serde_json::to_string_pretty(result)
                    .unwrap_or_else(|e| format!("{{\"error\":\"serialization failed: {e}\"}}")),
            )
        } else {
            None
        }
    }
}

impl OutputWriter for StdWriter {
    fn write_success(&self, config: &OutputConfig, message: &str) {
        if config.is_json() {
            let val = serde_json::json!({
                "status": "success",
                "message": message,
            });
            println!(
                "{}",
                serde_json::to_string(&val)
                    .unwrap_or_else(|e| format!("{{\"error\":\"serialization failed: {e}\"}}"))
            );
        } else {
            let sym = symbols::pass_symbol(config);
            let styled = colors::success(config, &format!("{sym} {message}"));
            println!("{styled}");
        }
    }

    fn write_warning(&self, config: &OutputConfig, message: &str) {
        if config.is_json() {
            let val = serde_json::json!({
                "status": "warning",
                "message": message,
            });
            println!(
                "{}",
                serde_json::to_string(&val)
                    .unwrap_or_else(|e| format!("{{\"error\":\"serialization failed: {e}\"}}"))
            );
        } else {
            let sym = symbols::verify_symbol(config);
            let styled = colors::warning(config, &format!("{sym} {message}"));
            println!("{styled}");
        }
    }

    fn write_error(&self, config: &OutputConfig, message: &str) {
        if config.is_json() {
            let val = serde_json::json!({
                "status": "error",
                "message": message,
            });
            eprintln!(
                "{}",
                serde_json::to_string(&val)
                    .unwrap_or_else(|e| format!("{{\"error\":\"serialization failed: {e}\"}}"))
            );
        } else {
            let sym = symbols::fail_symbol(config);
            let styled = colors::error(config, &format!("{sym} {message}"));
            eprintln!("{styled}");
        }
    }

    fn write_result<T: Serialize>(&self, config: &OutputConfig, result: &T) {
        if config.is_json() {
            println!(
                "{}",
                serde_json::to_string(result)
                    .unwrap_or_else(|e| format!("{{\"error\":\"serialization failed: {e}\"}}"))
            );
        } else if config.verbosity >= Verbosity::Normal {
            println!(
                "{}",
                serde_json::to_string_pretty(result)
                    .unwrap_or_else(|e| format!("{{\"error\":\"serialization failed: {e}\"}}"))
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ColorMode, OutputMode};

    fn json_config() -> OutputConfig {
        OutputConfig {
            mode: OutputMode::Json,
            color: ColorMode::Never,
            verbosity: Verbosity::Normal,
            terminal_width: 80,
        }
    }

    fn text_config() -> OutputConfig {
        OutputConfig {
            mode: OutputMode::Text,
            color: ColorMode::Never,
            verbosity: Verbosity::Normal,
            terminal_width: 80,
        }
    }

    #[test]
    fn format_success_json_is_valid() {
        let cfg = json_config();
        let output = StdWriter::format_success(&cfg, "payment processed");
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["message"], "payment processed");
        assert!(!output.contains("\x1b["));
    }

    #[test]
    fn format_warning_json_is_valid() {
        let cfg = json_config();
        let output = StdWriter::format_warning(&cfg, "slow response");
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["status"], "warning");
        assert_eq!(parsed["message"], "slow response");
    }

    #[test]
    fn format_error_json_is_valid() {
        let cfg = json_config();
        let output = StdWriter::format_error(&cfg, "connection failed");
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["status"], "error");
        assert_eq!(parsed["message"], "connection failed");
    }

    #[test]
    fn format_result_json_serializes_struct() {
        let cfg = json_config();
        #[derive(Serialize)]
        struct TestResult {
            count: u32,
            name: String,
        }
        let result = TestResult {
            count: 5,
            name: "test".into(),
        };
        let output = StdWriter::format_result(&cfg, &result).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["count"], 5);
        assert_eq!(parsed["name"], "test");
    }

    #[test]
    fn format_success_text_contains_message() {
        let cfg = text_config();
        let output = StdWriter::format_success(&cfg, "all good");
        assert!(output.contains("all good"));
        // No ANSI codes because color is Never
        assert!(!output.contains("\x1b["));
    }

    #[test]
    fn format_result_quiet_returns_none() {
        let cfg = OutputConfig {
            mode: OutputMode::Text,
            color: ColorMode::Never,
            verbosity: Verbosity::Quiet,
            terminal_width: 80,
        };
        #[derive(Serialize)]
        struct Dummy {
            x: u32,
        }
        assert!(StdWriter::format_result(&cfg, &Dummy { x: 1 }).is_none());
    }
}
