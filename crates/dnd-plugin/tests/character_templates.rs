#![allow(clippy::unwrap_used, clippy::panic)] // test helpers
mod common;
use common::*;
use serde_json::json;

#[test]
fn creature_layout_uses_the_system_schema_and_references() {
    let mut rules = system_rules();
    rules["creature_sheet_layout"] = json!("sheet/creature.layout.json");
    let package = system_package().json("system.json", &rules).json(
        "sheet/creature.layout.json",
        &json!({"type":"section","label":"Creature","children":[
            {"type":"field","path":"str","label":"Strength"},
            {"type":"computed","name":"str_mod","label":"Modifier"},
            {"type":"resource","resource":"hp"},
            {"type":"roll","roll":"str_check"}
        ]}),
    );
    let loaded = package.read().unwrap();
    assert_eq!(
        loaded
            .to_dto()
            .system
            .unwrap()
            .creature_sheet_layout
            .unwrap()["label"],
        "Creature"
    );
    for node in [
        json!({"type":"field","path":"missing","label":"Bad"}),
        json!({"type":"roll","roll":"missing"}),
        json!({"type":"resource","resource":"missing"}),
        json!({"type":"computed","name":"missing","label":"Bad"}),
        json!({"type":"script","code":"alert(1)"}),
    ] {
        assert!(
            package
                .clone()
                .json("sheet/creature.layout.json", &node)
                .read()
                .is_err()
        );
    }
}

#[test]
fn creature_layout_requires_a_safe_existing_distinct_file() {
    for path in [
        "../creature.json",
        "https://example.org/form.json",
        "sheet/missing.json",
        "sheet/character.schema.json",
    ] {
        let mut rules = system_rules();
        rules["creature_sheet_layout"] = json!(path);
        assert!(system_package().json("system.json", &rules).read().is_err());
    }
}

#[test]
fn character_templates_are_validated() {
    for template in [
        json!(null),
        json!({"kind":"monster","sheet_data":{}}),
        json!({"kind":"npc","sheet_data":[]}),
        json!({"kind":"npc","sheet_data":{"str":99}}),
        json!({"kind":"npc","sheet_data":{"str":2.5}}),
    ] {
        let mut c = compendium();
        c["entries"][0]["data"]["character_template"] = template;
        assert!(
            system_package()
                .json("compendia/spells.json", &c)
                .read()
                .is_err()
        );
    }
}

#[test]
fn character_template_preserves_data_and_allows_schema_defaults() {
    let mut c = compendium();
    c["entries"][0]["data"]["character_template"] =
        json!({"name":{"en":"Scout"},"kind":"npc","sheet_data":{}});
    let p = system_package()
        .json("compendia/spells.json", &c)
        .read()
        .unwrap();
    assert_eq!(
        p.compendia[0].entries[0].data["character_template"]["kind"],
        "npc"
    );
}
