//! The system plugins shipped with the app (`plugins/`) pack and load, with their compendia.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::path::Path;

use dnd_plugin::{PackageLimits, PluginKind, pack_directory, read_package};

fn bundled(dir: &str) -> dnd_plugin::PluginPackage {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../plugins")
        .join(dir);
    let bytes = pack_directory(&path).unwrap();
    assert_eq!(
        pack_directory(&path).unwrap(),
        bytes,
        "packing is deterministic"
    );
    read_package(&bytes, PackageLimits::default())
        .unwrap_or_else(|e| panic!("{dir}: {} {e}", e.code()))
}

#[test]
fn srd5_loads_with_its_sheet_rules_and_compendia() {
    let package = bundled("system-srd5");
    assert_eq!(package.manifest.id, "srd5");
    assert_eq!(package.manifest.kind, PluginKind::System);
    assert!(
        package
            .manifest
            .attribution
            .as_deref()
            .is_some_and(|a| a.contains("SRD 5.2"))
    );
    let dto = package.to_dto();
    let rules = dto.system.unwrap();
    assert_eq!(rules.initiative, "1d20 + @dex_mod");
    assert!(rules.creature_sheet_layout.is_some());
    assert!(rules.computed.contains_key("str_mod"));
    assert!(rules.rolls.iter().any(|r| r.id == "stealth"));
    assert_eq!(package.compendia.len(), 2);
}

#[test]
fn generic_d6_loads() {
    let package = bundled("system-generic-d6");
    assert_eq!(package.manifest.id, "generic-d6");
    assert_eq!(
        package.compendia[0].entries[0].data["character_template"]["sheet_data"]["attributes"]["agility"],
        2
    );
    let rules = package.to_dto().system.unwrap();
    assert_eq!(rules.initiative, "2d6 + @attributes.agility");
    assert!(rules.creature_sheet_layout.is_some());
    assert_eq!(rules.health_thresholds, [66, 33, 0]);
}
