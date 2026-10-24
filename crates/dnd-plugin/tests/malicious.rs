//! Packages that must be refused (CLAUDE.md §9), each with its stable error code.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use std::io::{Cursor, Write};

use common::*;
use dnd_plugin::*;
use serde_json::{Value, json};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// One edit of a JSON document, for tables of bad cases.
type EditFn = Box<dyn FnOnce(&mut Value)>;

fn with_rules(edit: impl FnOnce(&mut Value)) -> Package {
    let mut rules = system_rules();
    edit(&mut rules);
    system_package().json("system.json", &rules)
}

fn with_schema(edit: impl FnOnce(&mut Value)) -> Package {
    let mut schema = sheet_schema();
    edit(&mut schema);
    system_package().json("sheet/character.schema.json", &schema)
}

fn with_layout(layout: Value) -> Package {
    system_package().json("sheet/character.layout.json", &layout)
}

fn with_manifest(edit: impl FnOnce(&mut Value)) -> Package {
    let mut m = manifest("test-system", "system");
    edit(&mut m);
    system_package().json("manifest.json", &m)
}

fn field(path: &str) -> Value {
    json!({ "type": "field", "path": path, "label": "x" })
}

#[test]
fn not_a_zip() {
    let err = read_package(b"definitely not a zip", PackageLimits::DEFAULT).unwrap_err();
    assert_eq!(err.code(), "invalid_archive");
}

#[test]
fn path_escapes_and_unknown_names_are_refused() {
    for name in [
        "../manifest.json",
        "../../evil.json",
        "/etc/passwd",
        "C:/Windows/evil.json",
        "sheet/../../evil.json",
        "sheet\\..\\evil.json",
        "sheet/nested/x.json",
        "sheet/.hidden.json",
        "compendia/UPPER.json",
        "compendia/x.txt",
        "README.md",
        "script.js",
        "assets/logo.png",
        "assets/0000000000000000000000000000000000000000000000000000000000000000.svg",
        "theme.css",
        "other/",
    ] {
        let package = system_package().file(name, b"{}".to_vec());
        assert_eq!(package.code(), "unexpected_file", "{name}");
    }
}

#[test]
fn unreferenced_sheet_files_and_system_json_in_content_are_refused() {
    let extra = system_package().json("sheet/extra.json", &json!({ "type": "object" }));
    assert_eq!(extra.code(), "unexpected_file");
    let sheet_in_content = content_package().json("sheet/x.json", &json!({ "type": "object" }));
    assert_eq!(sheet_in_content.code(), "unexpected_file");
    let rules_in_content = content_package().json("system.json", &system_rules());
    assert_eq!(rules_in_content.code(), "unexpected_file");
}

#[test]
fn duplicate_entries_are_refused() {
    // The zip writer refuses duplicate names, so write a look-alike and patch the name bytes
    // (names are not covered by the CRC).
    let mut bytes = content_package()
        .file("compendia/monsterz.json", b"{}".to_vec())
        .build();
    let (from, to) = (b"compendia/monsterz.json", b"compendia/monsters.json");
    let mut i = 0;
    while i + from.len() <= bytes.len() {
        if &bytes[i..i + from.len()] == from {
            bytes[i..i + from.len()].copy_from_slice(to);
        }
        i += 1;
    }
    let err = read_package(&bytes, PackageLimits::DEFAULT).unwrap_err();
    assert_eq!(err.code(), "invalid_archive");
    assert!(err.to_string().contains("duplicate"), "{err}");
}

#[test]
fn symlinks_are_refused() {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    zip.start_file("manifest.json", options).unwrap();
    zip.write_all(&serde_json::to_vec(&manifest("x", "content")).unwrap())
        .unwrap();
    zip.add_symlink("compendia/a.json", "/etc/passwd", options)
        .unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    let err = read_package(&bytes, PackageLimits::DEFAULT).unwrap_err();
    assert_eq!(err.code(), "unsafe_entry");
}

#[test]
fn zip_bomb_is_stopped_by_bytes_actually_read() {
    // 8 MiB of zeros compresses to a few KiB; the header's size is not what is checked.
    let zeros = vec![0_u8; 8 * 1024 * 1024];
    let bytes = content_package().file("compendia/bomb.json", zeros).build();
    assert!(bytes.len() < 64 * 1024);
    let limits = PackageLimits {
        max_json_bytes: 1024 * 1024,
        ..PackageLimits::DEFAULT
    };
    let err = read_package(&bytes, limits).unwrap_err();
    assert_eq!(err.code(), "too_large");
    assert!(err.to_string().contains("compendia/bomb.json"), "{err}");

    // Many medium files hit the total limit.
    let limits = PackageLimits {
        max_total_bytes: 1024,
        ..PackageLimits::DEFAULT
    };
    let mut package = content_package();
    for i in 0..4 {
        package = package.file(&format!("compendia/c{i}.json"), vec![b' '; 400]);
    }
    let err = read_package(&package.build(), limits).unwrap_err();
    assert_eq!(err.code(), "too_large");
}

#[test]
fn too_many_entries_are_refused() {
    let limits = PackageLimits {
        max_entries: 3,
        ..PackageLimits::DEFAULT
    };
    let err = read_package(&system_package().build(), limits).unwrap_err();
    assert_eq!(err.code(), "too_large");
}

#[test]
fn oversized_package_is_refused_before_unzipping() {
    let limits = PackageLimits {
        max_package_bytes: 10,
        ..PackageLimits::DEFAULT
    };
    let err = read_package(&system_package().build(), limits).unwrap_err();
    assert_eq!(err.code(), "too_large");
}

#[test]
fn assets_must_match_their_hash_and_format() {
    let png = png();
    let wrong_hash = content_package().file(&asset_name(b"something else", "png"), png.clone());
    assert_eq!(wrong_hash.code(), "hash_mismatch");

    let not_png = b"<svg onload=alert(1)>".to_vec();
    let fake = content_package().file(&asset_name(&not_png, "png"), not_png);
    assert_eq!(fake.code(), "invalid_asset");

    let png_as_webp = content_package().file(&asset_name(&png, "webp"), png);
    assert_eq!(png_as_webp.code(), "invalid_asset");
}

#[test]
fn missing_files() {
    assert_eq!(
        system_package().without("manifest.json").code(),
        "missing_file"
    );
    assert_eq!(
        system_package().without("system.json").code(),
        "missing_file"
    );
    assert_eq!(
        system_package()
            .without("sheet/character.layout.json")
            .code(),
        "missing_file"
    );
}

#[test]
fn bad_manifests() {
    let cases: Vec<(&str, EditFn)> = vec![
        ("not json", Box::new(|_| {})),
        ("bad id", Box::new(|m| m["id"] = json!("Bad_Id"))),
        ("id path", Box::new(|m| m["id"] = json!("../x"))),
        ("long id", Box::new(|m| m["id"] = json!("a".repeat(65)))),
        ("bad version", Box::new(|m| m["version"] = json!("1.0"))),
        ("bad kind", Box::new(|m| m["kind"] = json!("script"))),
        ("no authors", Box::new(|m| m["authors"] = json!([]))),
        (
            "bad range",
            Box::new(|m| m["app_compat"] = json!("not a range")),
        ),
        ("unknown key", Box::new(|m| m["main"] = json!("index.js"))),
        (
            "no license",
            Box::new(|m| {
                m.as_object_mut().unwrap().remove("license");
            }),
        ),
        (
            "bad dependency id",
            Box::new(|m| m["dependencies"] = json!({ "Bad": "^1" })),
        ),
        (
            "bad dependency range",
            Box::new(|m| m["dependencies"] = json!({ "x": "^^1" })),
        ),
        (
            "self dependency",
            Box::new(|m| m["dependencies"] = json!({ "test-system": "^1" })),
        ),
        ("empty name", Box::new(|m| m["name"] = json!({}))),
        (
            "name in unknown language",
            Box::new(|m| m["name"] = json!({ "de": "Test" })),
        ),
    ];
    for (what, edit) in cases {
        let package = if what == "not json" {
            system_package().file("manifest.json", b"{ nope".to_vec())
        } else {
            with_manifest(edit)
        };
        assert_eq!(package.code(), "invalid_manifest", "{what}");
    }
}

#[test]
fn newer_format_is_reported_as_such() {
    let err = with_manifest(|m| m["format"] = json!(2))
        .read()
        .unwrap_err();
    assert_eq!(err.code(), "unsupported_format");
}

#[test]
fn bad_system_rules() {
    let cases: Vec<(&str, EditFn)> = vec![
        (
            "no initiative",
            Box::new(|r| {
                r.as_object_mut().unwrap().remove("initiative");
            }),
        ),
        (
            "sheet outside sheet/",
            Box::new(|r| r["sheet"]["schema"] = json!("compendia/spells.json")),
        ),
        (
            "sheet path escape",
            Box::new(|r| r["sheet"]["layout"] = json!("sheet/../manifest.json")),
        ),
        (
            "ascending thresholds",
            Box::new(|r| r["health_thresholds"] = json!([25, 75, 0])),
        ),
        (
            "threshold > 100",
            Box::new(|r| r["health_thresholds"] = json!([150, 0])),
        ),
        (
            "duplicate roll",
            Box::new(|r| {
                r["rolls"] = json!([
                    { "id": "a", "label": "A", "dice": "d20" }, { "id": "a", "label": "B", "dice": "d6" }
                ])
            }),
        ),
        (
            "long expression",
            Box::new(|r| r["computed"]["x"] = json!("1+".repeat(600))),
        ),
        (
            "bad computed name",
            Box::new(|r| r["computed"] = json!({ "a-b": "1" })),
        ),
        (
            "computed shadows field",
            Box::new(|r| r["computed"]["str"] = json!("1")),
        ),
        (
            "computed shadows resource",
            Box::new(|r| r["computed"]["hp"] = json!("1")),
        ),
        ("unknown key", Box::new(|r| r["script"] = json!("x"))),
        (
            "same file twice",
            Box::new(|r| r["sheet"]["layout"] = r["sheet"]["schema"].clone()),
        ),
    ];
    for (what, edit) in cases {
        assert_eq!(with_rules(edit).code(), "invalid_document", "{what}");
    }
}

#[test]
fn schema_outside_the_subset_is_refused() {
    let cases: Vec<(&str, EditFn)> = vec![
        (
            "root not object",
            Box::new(|s| *s = json!({ "type": "integer" })),
        ),
        (
            "$ref",
            Box::new(|s| s["properties"]["str"] = json!({ "$ref": "http://evil/x" })),
        ),
        (
            "pattern",
            Box::new(|s| s["properties"]["name"]["pattern"] = json!("^(a+)+$")),
        ),
        (
            "number type",
            Box::new(|s| s["properties"]["str"]["type"] = json!("number")),
        ),
        (
            "type list",
            Box::new(|s| s["properties"]["str"]["type"] = json!(["integer", "null"])),
        ),
        ("oneOf", Box::new(|s| s["oneOf"] = json!([]))),
        (
            "additionalProperties",
            Box::new(|s| s["additionalProperties"] = json!(true)),
        ),
        (
            "minimum on string",
            Box::new(|s| s["properties"]["name"]["minimum"] = json!(1)),
        ),
        (
            "maxLength on integer",
            Box::new(|s| s["properties"]["str"]["maxLength"] = json!(1)),
        ),
        (
            "fractional minimum",
            Box::new(|s| s["properties"]["str"]["minimum"] = json!(1.5)),
        ),
        (
            "unsafe integer",
            Box::new(|s| s["properties"]["str"]["maximum"] = json!(9007199254740992_u64)),
        ),
        (
            "min > max",
            Box::new(|s| s["properties"]["str"]["minimum"] = json!(40)),
        ),
        (
            "required unknown",
            Box::new(|s| s["required"] = json!(["str", "ghost"])),
        ),
        (
            "array without items",
            Box::new(|s| {
                s["properties"]["items"]
                    .as_object_mut()
                    .unwrap()
                    .remove("items");
            }),
        ),
        (
            "bad default",
            Box::new(|s| s["properties"]["str"]["default"] = json!(99)),
        ),
        (
            "bad enum value",
            Box::new(|s| s["properties"]["alignment"]["enum"] = json!(["good", 1])),
        ),
        (
            "bad property name",
            Box::new(|s| s["properties"]["bad-name"] = json!({ "type": "string" })),
        ),
        (
            "too deep",
            Box::new(|s| {
                let mut node = json!({ "type": "integer" });
                for _ in 0..20 {
                    node = json!({ "type": "object", "properties": { "a": node } });
                }
                s["properties"]["deep"] = node;
            }),
        ),
    ];
    for (what, edit) in cases {
        assert_eq!(with_schema(edit).code(), "unsupported_schema", "{what}");
    }
}

#[test]
fn dangling_layout_references_are_refused() {
    let cases = [
        ("unknown field", field("ghost")),
        ("unknown nested field", field("str.value")),
        ("object field", field("items")),
        (
            "unknown computed",
            json!({ "type": "computed", "name": "ghost", "label": "x" }),
        ),
        ("unknown roll", json!({ "type": "roll", "roll": "ghost" })),
        (
            "unknown resource",
            json!({ "type": "resource", "resource": "mana" }),
        ),
        (
            "list of a scalar",
            json!({ "type": "list", "path": "str", "label": "x", "item": [] }),
        ),
        (
            "list item field outside item",
            json!({ "type": "list", "path": "items", "label": "x", "item": [field("str")] }),
        ),
        (
            "widget mismatch",
            json!({ "type": "field", "path": "str", "label": "x", "widget": "checkbox" }),
        ),
        (
            "select without enum",
            json!({ "type": "field", "path": "name", "label": "x", "widget": "select" }),
        ),
        ("unknown node type", json!({ "type": "script", "src": "x" })),
        (
            "unknown key",
            json!({ "type": "row", "children": [], "onclick": "x" }),
        ),
        ("bad path", field("../str")),
        (
            "grid columns",
            json!({ "type": "grid", "columns": 13, "children": [] }),
        ),
        ("empty tabs", json!({ "type": "tabs", "tabs": [] })),
    ];
    for (what, node) in cases {
        let layout = json!({ "type": "section", "children": [node] });
        assert_eq!(with_layout(layout).code(), "invalid_layout", "{what}");
    }

    let mut deep = json!({ "type": "text", "text": "x" });
    for _ in 0..40 {
        deep = json!({ "type": "row", "children": [deep] });
    }
    assert_eq!(with_layout(deep).code(), "invalid_layout", "too deep");
}

#[test]
fn bad_compendia_are_refused() {
    let dup_entry = content_package().json(
        "compendia/c.json",
        &json!({ "id": "c", "name": "C", "entry_type": "x", "entries": [
            { "id": "a", "name": "A", "data": {} }, { "id": "a", "name": "B", "data": {} }
        ]}),
    );
    assert_eq!(dup_entry.code(), "invalid_document");

    let dup_id = content_package().json(
        "compendia/z.json",
        &json!({ "id": "monsters", "name": "Again", "entry_type": "x", "entries": [] }),
    );
    assert_eq!(dup_id.code(), "invalid_document");

    let data_not_object = content_package().json(
        "compendia/c.json",
        &json!({ "id": "c", "name": "C", "entry_type": "x", "entries": [
            { "id": "a", "name": "A", "data": [1] }
        ]}),
    );
    assert_eq!(data_not_object.code(), "invalid_document");
}

#[test]
fn stored_and_deflated_entries_both_work() {
    // The builder deflates; make sure a stored entry is fine too (no codec surprises).
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file("manifest.json", stored).unwrap();
    zip.write_all(&serde_json::to_vec(&manifest("x", "ui")).unwrap())
        .unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    assert!(read_package(&bytes, PackageLimits::DEFAULT).is_ok());
}

#[test]
fn messages_point_at_the_problem() {
    let err = with_manifest(|m| m["id"] = json!("Bad_Id"))
        .read()
        .unwrap_err();
    assert!(err.to_string().contains("/id"), "{err}");

    let err = with_schema(|s| s["properties"]["str"]["pattern"] = json!("x"))
        .read()
        .unwrap_err();
    assert!(err.to_string().contains("/properties/str"), "{err}");

    let err = with_layout(json!({ "type": "section", "children": [field("ghost")] }))
        .read()
        .unwrap_err();
    assert!(err.to_string().contains("ghost"), "{err}");

    let err = with_layout(json!({ "type": "field", "path": "str", "label": { "fr": "x" } }))
        .read()
        .unwrap_err();
    assert!(err.to_string().contains("/label"), "{err}");
}
