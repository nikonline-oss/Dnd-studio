#![allow(clippy::unwrap_used, clippy::panic)]
use dnd_game_core::check_plugin;
use dnd_protocol::PluginDto;
use serde_json::{Value, json};
mod common;

fn plugin(kind: &str, theme: Value) -> PluginDto {
    serde_json::from_value(json!({"id":"palette","version":"1.0.0","kind":kind,"name":"Palette","license":"MIT","archive_sha256":"a".repeat(64),"system":null,"theme":theme})).unwrap()
}

#[test]
fn authoritative_validation_rejects_palette_in_other_kinds_and_unsafe_values() {
    for kind in ["content", "ui"] {
        assert!(check_plugin(&plugin(kind, json!({"light":{"--color-bg":"#fff"}}))).is_err());
    }
    for theme in [
        json!({}),
        json!({"dark":{"--color-bg":"url(x)"}}),
        json!({"light":{"--color-bg":"#fff;display:none"}}),
        json!({"light":{"--unknown":"#fff"}}),
    ] {
        assert!(check_plugin(&plugin("theme", theme)).is_err());
    }
}

#[test]
fn legacy_theme_without_palette_is_a_noop_and_valid_palette_is_accepted() {
    assert!(check_plugin(&plugin("theme", Value::Null)).is_ok());
    assert!(check_plugin(&plugin("theme", json!({"dark":{"--color-bg":"#123456"}}))).is_ok());
}

#[test]
fn theme_installation_uses_existing_authorization_and_events() {
    use common::{base_state, run};
    use dnd_protocol::{Intent, Role};
    for role in [Role::Gm, Role::CoGm, Role::Player, Role::Spectator] {
        let mut state = base_state();
        let theme = plugin("theme", json!({"light":{"--color-text":"#123"}}));
        let result = run(
            &mut state,
            role,
            Intent::InstallPlugin {
                plugin: Box::new(theme.clone()),
            },
            None,
        );
        if matches!(role, Role::Gm | Role::CoGm) {
            assert!(result.is_ok());
            assert_eq!(state.plugins["palette"].theme, theme.theme);
            assert!(state.system_plugin.is_none());
        } else {
            assert!(result.is_err());
            assert!(!state.plugins.contains_key("palette"));
        }
    }
}
