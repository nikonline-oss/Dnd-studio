//! Safe theme palettes (P1, ADR-0030). No CSS expressions or resource references.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::RejectReason;

/// Color tokens declared by the desktop's shared/theme/tokens.css.
pub const THEME_COLOR_VARIABLES: [&str; 16] = [
    "--color-bg",
    "--color-surface",
    "--color-surface-alt",
    "--color-surface-raised",
    "--color-hover",
    "--color-border",
    "--color-border-strong",
    "--color-text",
    "--color-text-muted",
    "--color-accent",
    "--color-accent-soft",
    "--color-accent-contrast",
    "--color-danger",
    "--color-danger-soft",
    "--color-success",
    "--color-focus",
];

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ThemeDto {
    #[serde(default)]
    pub light: BTreeMap<String, String>,
    #[serde(default)]
    pub dark: BTreeMap<String, String>,
}

/// Shared by package loading and authoritative core validation.
pub fn validate_theme(theme: &ThemeDto) -> Result<(), RejectReason> {
    let invalid = || RejectReason::invalid("theme");
    if theme.light.is_empty() && theme.dark.is_empty() {
        return Err(invalid());
    }
    for palette in [&theme.light, &theme.dark] {
        if palette.len() > THEME_COLOR_VARIABLES.len() {
            return Err(invalid());
        }
        for (key, color) in palette {
            if !THEME_COLOR_VARIABLES.contains(&key.as_str())
                || !matches!(color.len(), 4 | 7 | 9)
                || !color.starts_with('#')
                || !color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
            {
                return Err(invalid());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_hex_formats_only() {
        for color in ["#abc", "#AbCdEf", "#12345678"] {
            let theme = ThemeDto {
                light: BTreeMap::from([("--color-bg".into(), color.into())]),
                ..ThemeDto::default()
            };
            assert!(validate_theme(&theme).is_ok());
        }
        for color in [
            "",
            "#",
            "#abcd",
            "#12345",
            "#1234567",
            "#123456789",
            "#ggg",
            " #abc",
            "#abc\n",
            "red",
            "transparent",
            "rgb(1 2 3)",
            "var(--x)",
            "url(x)",
            "#abc;display:none",
            "#１２３",
        ] {
            let theme = ThemeDto {
                dark: BTreeMap::from([("--color-bg".into(), color.into())]),
                ..ThemeDto::default()
            };
            assert!(validate_theme(&theme).is_err(), "{color}");
        }
    }

    #[test]
    fn whitelist_matches_css_color_tokens() {
        let css = include_str!("../../../apps/desktop/src/shared/theme/tokens.css");
        let actual: std::collections::BTreeSet<_> = css
            .lines()
            .filter_map(|line| line.trim().split_once(':').map(|(key, _)| key))
            .filter(|key| key.starts_with("--color-"))
            .collect();
        assert_eq!(actual, THEME_COLOR_VARIABLES.into_iter().collect());
    }
}
