use crate::config::OutputConfig;

/// Whether to use Unicode symbols (true) or ASCII fallbacks (false).
///
/// Uses Unicode when colors are enabled (implies capable terminal),
/// falls back to ASCII otherwise.
fn use_unicode(config: &OutputConfig) -> bool {
    config.colors_enabled()
}

/// Pass/success symbol: "✓" or "[PASS]".
pub fn pass_symbol(config: &OutputConfig) -> &'static str {
    if use_unicode(config) { "✓" } else { "[PASS]" }
}

/// Fail/error symbol: "✗" or "[FAIL]".
pub fn fail_symbol(config: &OutputConfig) -> &'static str {
    if use_unicode(config) { "✗" } else { "[FAIL]" }
}

/// Verify/warning symbol: "△" or "[VERIFY]".
pub fn verify_symbol(config: &OutputConfig) -> &'static str {
    if use_unicode(config) {
        "△"
    } else {
        "[VERIFY]"
    }
}

/// Info symbol: "ℹ" or "[INFO]".
pub fn info_symbol(config: &OutputConfig) -> &'static str {
    if use_unicode(config) { "ℹ" } else { "[INFO]" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ColorMode, OutputConfig, OutputMode, Verbosity};

    fn unicode_config() -> OutputConfig {
        OutputConfig {
            mode: OutputMode::Text,
            color: ColorMode::Always,
            verbosity: Verbosity::Normal,
            terminal_width: 80,
        }
    }

    fn ascii_config() -> OutputConfig {
        OutputConfig::from_env(false, true, false, false)
    }

    #[test]
    fn unicode_symbols_when_color_enabled() {
        let cfg = unicode_config();
        assert_eq!(pass_symbol(&cfg), "✓");
        assert_eq!(fail_symbol(&cfg), "✗");
        assert_eq!(verify_symbol(&cfg), "△");
        assert_eq!(info_symbol(&cfg), "ℹ");
    }

    #[test]
    fn ascii_fallbacks_when_no_color() {
        let cfg = ascii_config();
        assert_eq!(pass_symbol(&cfg), "[PASS]");
        assert_eq!(fail_symbol(&cfg), "[FAIL]");
        assert_eq!(verify_symbol(&cfg), "[VERIFY]");
        assert_eq!(info_symbol(&cfg), "[INFO]");
    }
}
