#![allow(clippy::unwrap_used, clippy::panic)]
mod common;
use common::*;
use serde_json::json;

#[test]
fn theme_package_requires_palette_and_preserves_both_modes() {
    let package = Package::new().json("manifest.json", &manifest("palette", "theme"));
    assert!(package.read().is_err());
    let theme = json!({"light":{"--color-bg":"#fff"},"dark":{"--color-accent":"#AABBCC80"}});
    let loaded = package.json("theme.json", &theme).read().unwrap();
    assert_eq!(
        serde_json::to_value(loaded.to_dto()).unwrap()["theme"],
        theme
    );
}

#[test]
fn theme_files_reject_unsafe_data_and_wrong_kinds() {
    for theme in [
        json!({}),
        json!({"light":{},"dark":{}}),
        json!({"light":{"--font-ui":"#fff"}}),
        json!({"dark":{"--color-bg":"url(https://example.org)"}}),
        json!({"light":{"--color-bg":"#ffff"}}),
        json!({"css":"body{}"}),
        json!({"light":null}),
    ] {
        assert!(
            Package::new()
                .json("manifest.json", &manifest("palette", "theme"))
                .json("theme.json", &theme)
                .read()
                .is_err()
        );
    }
    for kind in ["content", "ui"] {
        assert!(
            Package::new()
                .json("manifest.json", &manifest("palette", kind))
                .json("theme.json", &json!({"light":{"--color-bg":"#fff"}}))
                .read()
                .is_err()
        );
    }
    assert!(
        system_package()
            .json("theme.json", &json!({"light":{"--color-bg":"#fff"}}))
            .read()
            .is_err()
    );
}

#[test]
fn schema_and_shared_validator_accept_every_whitelisted_token() {
    let palette: serde_json::Map<String, serde_json::Value> = dnd_protocol::THEME_COLOR_VARIABLES
        .into_iter()
        .map(|key| (key.to_owned(), json!("#AaBbCc80")))
        .collect();
    let package = Package::new().json("manifest.json", &manifest("palette", "theme"));
    let loaded = package
        .json("theme.json", &json!({"dark":palette}))
        .read()
        .unwrap();
    let theme = loaded.to_dto().theme.unwrap();
    assert!(theme.light.is_empty());
    assert_eq!(theme.dark.len(), 16);
}
