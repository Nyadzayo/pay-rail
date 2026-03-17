use owo_colors::OwoColorize;

use crate::config::OutputConfig;

/// Apply success styling (green) if colors are enabled.
pub fn success(config: &OutputConfig, text: &str) -> String {
    if config.colors_enabled() {
        format!("{}", text.green())
    } else {
        text.to_string()
    }
}

/// Apply warning styling (yellow) if colors are enabled.
pub fn warning(config: &OutputConfig, text: &str) -> String {
    if config.colors_enabled() {
        format!("{}", text.yellow())
    } else {
        text.to_string()
    }
}

/// Apply error styling (red) if colors are enabled.
pub fn error(config: &OutputConfig, text: &str) -> String {
    if config.colors_enabled() {
        format!("{}", text.red())
    } else {
        text.to_string()
    }
}

/// Apply info styling (dimmed) if colors are enabled.
pub fn info(config: &OutputConfig, text: &str) -> String {
    if config.colors_enabled() {
        format!("{}", text.dimmed())
    } else {
        text.to_string()
    }
}

/// Apply accent styling (cyan) if colors are enabled.
pub fn accent(config: &OutputConfig, text: &str) -> String {
    if config.colors_enabled() {
        format!("{}", text.cyan())
    } else {
        text.to_string()
    }
}

/// Apply emphasis styling (bold white) if colors are enabled.
pub fn emphasis(config: &OutputConfig, text: &str) -> String {
    if config.colors_enabled() {
        format!("{}", text.bold())
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_color_config() -> OutputConfig {
        OutputConfig::from_env(false, true, false, false)
    }

    #[test]
    fn no_color_returns_plain_text() {
        let cfg = no_color_config();
        assert_eq!(success(&cfg, "ok"), "ok");
        assert_eq!(warning(&cfg, "warn"), "warn");
        assert_eq!(error(&cfg, "err"), "err");
        assert_eq!(info(&cfg, "note"), "note");
        assert_eq!(accent(&cfg, "link"), "link");
        assert_eq!(emphasis(&cfg, "bold"), "bold");
    }

    #[test]
    fn color_returns_ansi_sequences() {
        // Force colors on via Always-like config
        let cfg = OutputConfig {
            mode: crate::config::OutputMode::Text,
            color: crate::config::ColorMode::Always,
            verbosity: crate::config::Verbosity::Normal,
            terminal_width: 80,
        };
        let s = success(&cfg, "ok");
        assert!(s.contains("\x1b["), "expected ANSI escape in: {s}");
        assert!(s.contains("ok"));
    }
}
