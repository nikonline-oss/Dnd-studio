#![allow(clippy::unwrap_used, clippy::panic)] // test helpers
mod common;
use common::*;
use serde_json::json;

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
