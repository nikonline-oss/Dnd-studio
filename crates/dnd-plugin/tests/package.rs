//! Valid packages: what reading returns, the DTO for the core, reading from a file.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use common::*;
use dnd_plugin::*;
use serde_json::json;

#[test]
fn reads_a_system_package() {
    let bytes = system_package().build();
    let package = read_package(&bytes, PackageLimits::DEFAULT).unwrap();

    assert_eq!(package.sha256, hex_sha256(&bytes));
    let m = &package.manifest;
    assert_eq!(m.id, "test-system");
    assert_eq!(m.kind, PluginKind::System);
    assert_eq!(m.version, semver::Version::new(1, 0, 0));
    assert_eq!(m.name.get("ru"), "Тест");
    assert_eq!(m.name.get("de"), "Test");

    let system = package.system.as_ref().unwrap();
    assert_eq!(system.rules.initiative, "1d20 + @str_mod");
    assert_eq!(system.rules.health_thresholds, vec![75, 25, 0]);
    assert_eq!(system.sheet_schema, sheet_schema());
    let LayoutNode::Tabs { tabs } = &system.sheet_layout else {
        panic!("root is tabs");
    };
    assert_eq!(tabs.len(), 2);

    assert_eq!(package.compendia.len(), 1);
    assert_eq!(package.compendia[0].entries[0].id, "fireball");
    assert_eq!(package.assets.len(), 1);
    assert_eq!(package.assets[0].format, AssetFormat::Png);
    assert_eq!(package.assets[0].bytes, png());
}

#[test]
fn reads_a_content_package() {
    let package = content_package().read().unwrap();
    assert_eq!(package.manifest.kind, PluginKind::Content);
    assert!(package.system.is_none());
    assert_eq!(
        package.manifest.dependencies["test-system"].to_string(),
        "^1"
    );
    assert_eq!(package.compendia[0].entries[0].data["hp"], json!(7));
}

#[test]
fn optional_fields_have_defaults() {
    let mut m = manifest("bare", "system");
    m.as_object_mut().unwrap().remove("dependencies");
    let rules = json!({
        "sheet": { "schema": "sheet/s.json", "layout": "sheet/l.json" },
        "initiative": "1d6"
    });
    let package = Package::new()
        .json("manifest.json", &m)
        .json("system.json", &rules)
        .json("sheet/s.json", &json!({ "type": "object" }))
        .json("sheet/l.json", &json!({ "type": "text", "text": "empty" }))
        .read()
        .unwrap();
    assert!(package.manifest.dependencies.is_empty());
    let rules = &package.system.unwrap().rules;
    assert_eq!(rules.health_thresholds, DEFAULT_HEALTH_THRESHOLDS.to_vec());
    assert!(rules.computed.is_empty() && rules.resources.is_empty() && rules.rolls.is_empty());
}

#[test]
fn folder_entries_are_allowed() {
    let package = system_package()
        .file("sheet/", Vec::new())
        .file("assets/", Vec::new());
    // `start_file` with a trailing slash writes a directory entry.
    assert!(package.read().is_ok());
}

#[test]
fn webp_assets_are_accepted() {
    let mut webp = b"RIFF\x10\x00\x00\x00WEBPVP8 ".to_vec();
    webp.extend_from_slice(&[0; 8]);
    let package = content_package()
        .file(&asset_name(&webp, "webp"), webp.clone())
        .read()
        .unwrap();
    assert_eq!(package.assets[0].format, AssetFormat::Webp);
}

#[test]
fn dto_for_the_core() {
    let package = system_package().read().unwrap();
    let dto = package.to_dto();
    assert_eq!(dto.id, "test-system");
    assert_eq!(dto.version, "1.0.0");
    assert_eq!(dto.archive_sha256, package.sha256);
    let system = dto.system.as_ref().unwrap();
    assert_eq!(system.sheet_layout, sheet_layout());
    assert_eq!(system.computed["str_mod"], "floor((@str - 10) / 2)");

    let json = serde_json::to_value(&dto).unwrap();
    assert_eq!(json["kind"], "system");
    assert_eq!(json["name"], json!({ "ru": "Тест", "en": "Test" }));
    let back: InstalledPluginDto = serde_json::from_value(json).unwrap();
    assert_eq!(back, dto);

    let content = content_package().read().unwrap().to_dto();
    let json = serde_json::to_value(&content).unwrap();
    assert!(json.get("system").is_none());
}

#[test]
fn reads_from_a_file_and_returns_its_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.dndplugin");
    let bytes = system_package().build();
    std::fs::write(&path, &bytes).unwrap();

    let (package, read) = read_package_file(&path, PackageLimits::DEFAULT).unwrap();
    assert_eq!(read, bytes);
    assert_eq!(package.sha256, hex_sha256(&bytes));

    let small = PackageLimits {
        max_package_bytes: 100,
        ..PackageLimits::DEFAULT
    };
    let err = read_package_file(&path, small).unwrap_err();
    assert_eq!(err.code(), "too_large");

    let err = read_package_file(&dir.path().join("missing"), PackageLimits::DEFAULT).unwrap_err();
    assert_eq!(err.code(), "io");
}

#[test]
fn check_data_follows_the_schema() {
    let schema = sheet_schema();
    assert!(check_data(&schema, &json!({ "str": 10 })).is_ok());
    assert!(
        check_data(
            &schema,
            &json!({ "str": 30, "alignment": "evil", "items": [{ "title": "rope", "count": 1 }] })
        )
        .is_ok()
    );
    for bad in [
        json!({}),                                  // required
        json!({ "str": 31 }),                       // maximum
        json!({ "str": 1.5 }),                      // integer
        json!({ "str": 10, "alignment": "chaos" }), // enum
        json!({ "str": 10, "extra": 1 }),           // unknown field
        json!({ "str": 10, "name": "x".repeat(101) }),
        json!({ "str": 10, "items": [{ "count": -1 }] }),
        json!({ "str": 10, "inspired": "yes" }),
    ] {
        assert!(check_data(&schema, &bad).is_err(), "{bad}");
    }
}
