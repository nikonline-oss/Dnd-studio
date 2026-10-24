//! Building `.dndplugin` packages in tests.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)] // test code

use std::io::{Cursor, Write};

use dnd_plugin::{PackageLimits, PluginError, PluginPackage, hex_sha256, read_package};
use serde_json::{Value, json};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// A tiny "PNG": the signature is all the crate checks.
pub fn png() -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend_from_slice(b"not really an image");
    bytes
}

pub fn asset_name(bytes: &[u8], ext: &str) -> String {
    format!("assets/{}.{ext}", hex_sha256(bytes))
}

#[derive(Clone, Default)]
pub struct Package {
    files: Vec<(String, Vec<u8>)>,
}

impl Package {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn file(mut self, name: &str, bytes: impl Into<Vec<u8>>) -> Self {
        self.files.retain(|(n, _)| n != name);
        self.files.push((name.to_owned(), bytes.into()));
        self
    }

    pub fn json(self, name: &str, value: &Value) -> Self {
        self.file(name, serde_json::to_vec_pretty(value).unwrap())
    }

    pub fn without(mut self, name: &str) -> Self {
        self.files.retain(|(n, _)| n != name);
        self
    }

    pub fn build(&self) -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, bytes) in &self.files {
            zip.start_file(name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    pub fn read(&self) -> Result<PluginPackage, PluginError> {
        read_package(&self.build(), PackageLimits::DEFAULT)
    }

    /// The error code of reading, panicking if the package is accepted.
    pub fn code(&self) -> &'static str {
        match self.read() {
            Ok(_) => panic!("the package was accepted"),
            Err(e) => e.code(),
        }
    }
}

pub fn manifest(id: &str, kind: &str) -> Value {
    json!({
        "format": 1,
        "id": id,
        "name": { "ru": "Тест", "en": "Test" },
        "version": "1.0.0",
        "kind": kind,
        "authors": ["Tester"],
        "license": "CC-BY-4.0",
        "app_compat": "^0.1",
        "dependencies": {}
    })
}

pub fn system_rules() -> Value {
    json!({
        "sheet": { "schema": "sheet/character.schema.json", "layout": "sheet/character.layout.json" },
        "computed": { "str_mod": "floor((@str - 10) / 2)" },
        "initiative": "1d20 + @str_mod",
        "resources": [{ "id": "hp", "label": { "ru": "Хиты", "en": "HP" }, "value": "@hp_current", "max": "@hp_max" }],
        "health_thresholds": [75, 25, 0],
        "rolls": [{ "id": "str_check", "label": "STR", "dice": "1d20 + @str_mod" }]
    })
}

pub fn sheet_schema() -> Value {
    json!({
        "type": "object",
        "required": ["str"],
        "properties": {
            "name": { "type": "string", "maxLength": 100, "title": { "ru": "Имя", "en": "Name" } },
            "str": { "type": "integer", "minimum": 1, "maximum": 30, "default": 10 },
            "hp_current": { "type": "integer", "minimum": 0, "default": 0 },
            "hp_max": { "type": "integer", "minimum": 0, "default": 0 },
            "inspired": { "type": "boolean", "default": false },
            "alignment": { "type": "string", "enum": ["good", "neutral", "evil"] },
            "notes": { "type": "string" },
            "items": {
                "type": "array",
                "maxItems": 50,
                "items": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string" },
                        "count": { "type": "integer", "minimum": 0 }
                    }
                }
            }
        }
    })
}

pub fn sheet_layout() -> Value {
    json!({
        "type": "tabs",
        "tabs": [
            {
                "label": { "ru": "Основное", "en": "Main" },
                "children": [
                    { "type": "section", "label": "Hero", "children": [
                        { "type": "field", "path": "name", "label": "Name" },
                        { "type": "row", "children": [
                            { "type": "field", "path": "str", "label": "STR", "widget": "number" },
                            { "type": "computed", "name": "str_mod", "label": "mod" },
                            { "type": "roll", "roll": "str_check" }
                        ]},
                        { "type": "field", "path": "alignment", "label": "Alignment", "widget": "select" },
                        { "type": "field", "path": "inspired", "label": "Inspired" },
                        { "type": "resource", "resource": "hp" }
                    ]}
                ]
            },
            {
                "label": "Inventory",
                "children": [
                    { "type": "grid", "columns": 2, "children": [
                        { "type": "list", "path": "items", "label": "Items", "item": [
                            { "type": "field", "path": "title", "label": "Item" },
                            { "type": "field", "path": "count", "label": "#" }
                        ]},
                        { "type": "field", "path": "notes", "label": "Notes", "widget": "textarea" }
                    ]},
                    { "type": "text", "text": { "ru": "Конец", "en": "The end" } }
                ]
            }
        ]
    })
}

pub fn compendium() -> Value {
    json!({
        "id": "spells",
        "name": { "ru": "Заклинания", "en": "Spells" },
        "entry_type": "spell",
        "entries": [
            { "id": "fireball", "name": "Fireball", "data": { "level": 3 } },
            { "id": "light", "name": "Light", "data": {} }
        ]
    })
}

/// A complete valid system package.
pub fn system_package() -> Package {
    let png = png();
    Package::new()
        .json("manifest.json", &manifest("test-system", "system"))
        .json("system.json", &system_rules())
        .json("sheet/character.schema.json", &sheet_schema())
        .json("sheet/character.layout.json", &sheet_layout())
        .json("compendia/spells.json", &compendium())
        .file(&asset_name(&png, "png"), png)
}

/// A minimal valid content package.
pub fn content_package() -> Package {
    let mut m = manifest("test-content", "content");
    m["dependencies"] = json!({ "test-system": "^1" });
    Package::new().json("manifest.json", &m).json(
        "compendia/monsters.json",
        &json!({
            "id": "monsters", "name": "Monsters", "entry_type": "monster",
            "entries": [{ "id": "goblin", "name": "Goblin", "data": { "hp": 7 } }]
        }),
    )
}
