use std::env;

/// Output mode: human-readable text or machine-readable JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Text,
    Json,
}

/// Verbosity level for output detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verbosity {
    Quiet,
    Normal,
    Verbose,
}

/// Color mode for terminal output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Auto,
    Always,
    Never,
}

/// Central configuration for all output formatting.
#[derive(Debug, Clone)]
pub struct OutputConfig {
    pub mode: OutputMode,
    pub color: ColorMode,
    pub verbosity: Verbosity,
    pub terminal_width: u16,
}

impl OutputConfig {
    /// Create config from CLI flags and environment detection.
    pub fn from_env(json: bool, no_color: bool, verbose: bool, quiet: bool) -> Self {
        let mode = if json {
            OutputMode::Json
        } else {
            OutputMode::Text
        };

        let color = if no_color || json || env_no_color() {
            ColorMode::Never
        } else {
            ColorMode::Auto
        };

        let verbosity = if quiet {
            Verbosity::Quiet
        } else if verbose {
            Verbosity::Verbose
        } else {
            Verbosity::Normal
        };

        let terminal_width = detect_terminal_width();

        Self {
            mode,
            color,
            verbosity,
            terminal_width,
        }
    }

    /// Whether colors should be emitted.
    pub fn colors_enabled(&self) -> bool {
        match self.color {
            ColorMode::Always => true,
            ColorMode::Never => false,
            ColorMode::Auto => true,
        }
    }

    /// Whether output should be JSON.
    pub fn is_json(&self) -> bool {
        self.mode == OutputMode::Json
    }
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self::from_env(false, false, false, false)
    }
}

/// Check if `NO_COLOR` environment variable is set (any value).
fn env_no_color() -> bool {
    env::var_os("NO_COLOR").is_some()
}

/// Detect terminal width from `$COLUMNS`, with default 80.
pub fn detect_terminal_width() -> u16 {
    env::var("COLUMNS")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .filter(|&w| w > 0)
        .unwrap_or(80)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn default_config_is_text_normal() {
        // Clear NO_COLOR to ensure predictable test
        unsafe {
            env::remove_var("NO_COLOR");
        }
        let cfg = OutputConfig::default();
        assert_eq!(cfg.mode, OutputMode::Text);
        assert_eq!(cfg.verbosity, Verbosity::Normal);
        assert_eq!(cfg.color, ColorMode::Auto);
    }

    #[test]
    fn json_flag_disables_color() {
        let cfg = OutputConfig::from_env(true, false, false, false);
        assert_eq!(cfg.mode, OutputMode::Json);
        assert_eq!(cfg.color, ColorMode::Never);
        assert!(!cfg.colors_enabled());
    }

    #[test]
    fn no_color_flag_disables_color() {
        let cfg = OutputConfig::from_env(false, true, false, false);
        assert_eq!(cfg.color, ColorMode::Never);
        assert!(!cfg.colors_enabled());
    }

    #[test]
    fn verbose_and_quiet_flags() {
        let verbose = OutputConfig::from_env(false, false, true, false);
        assert_eq!(verbose.verbosity, Verbosity::Verbose);

        let quiet = OutputConfig::from_env(false, false, false, true);
        assert_eq!(quiet.verbosity, Verbosity::Quiet);
    }

    #[test]
    #[serial]
    fn width_detection_with_columns_env() {
        unsafe {
            env::set_var("COLUMNS", "120");
        }
        let width = detect_terminal_width();
        assert_eq!(width, 120);
        unsafe {
            env::remove_var("COLUMNS");
        }
    }

    #[test]
    #[serial]
    fn width_detection_default_80() {
        unsafe {
            env::remove_var("COLUMNS");
        }
        let width = detect_terminal_width();
        assert_eq!(width, 80);
    }

    #[test]
    #[serial]
    fn width_detection_invalid_columns() {
        unsafe {
            env::set_var("COLUMNS", "not_a_number");
        }
        let width = detect_terminal_width();
        assert_eq!(width, 80);
        unsafe {
            env::remove_var("COLUMNS");
        }
    }
}
