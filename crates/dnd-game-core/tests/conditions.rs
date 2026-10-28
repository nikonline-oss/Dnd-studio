//! Conditions, items and system rolls (ADR-0024), on the bundled SRD system.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use std::path::Path;

use common::*;
use dnd_game_core::{GameState, check_plugin, inventory_names};
use dnd_protocol::arbitrary::{pooled_character_id, v7};
use dnd_protocol::*;

fn srd() -> PluginDto {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/system-srd5");
    let bytes = dnd_plugin::pack_directory(&path).unwrap();
    dnd_plugin::read_package(&bytes, dnd_plugin::PackageLimits::default())
        .unwrap()
        .to_dto()
}

/// The table under SRD 5.2: character 0 belongs to `PLAYER`, character 1 to `OTHER_PLAYER`.
fn srd_table() -> GameState {
    let mut state = table_state();
    run(
        &mut state,
        Role::Gm,
        Intent::InstallPlugin {
            plugin: Box::new(srd()),
        },
        None,
    )
    .unwrap();
    run(
        &mut state,
        Role::Gm,
        Intent::SetSystemPlugin {
            id: Some("srd5".into()),
        },
        None,
    )
    .unwrap();
    state
}

fn hero(n: u128) -> CharacterId {
    pooled_character_id(n)
}

fn set_conditions(
    state: &mut GameState,
    role: Role,
    n: u128,
    ids: &[&str],
) -> Result<Vec<Event>, RejectReason> {
    let version = state.characters[&hero(n)].version;
    run(
        state,
        role,
        Intent::SetConditions {
            id: hero(n),
            conditions: ids.iter().map(|s| (*s).to_owned()).collect(),
        },
        Some(version),
    )
}

#[test]
fn conditions_are_the_systems_once_each_in_its_order() {
    let mut state = srd_table();
    set_conditions(&mut state, Role::Gm, 0, &["prone", "poisoned", "prone"]).unwrap();
    assert_eq!(state.characters[&hero(0)].conditions, ["poisoned", "prone"]);
    assert_eq!(
        set_conditions(&mut state, Role::Gm, 0, &["on_fire"]),
        Err(RejectReason::invalid("conditions"))
    );
    // Without a system there are no conditions at all.
    let mut bare = table_state();
    assert_eq!(
        set_conditions(&mut bare, Role::Gm, 0, &["prone"]),
        Err(RejectReason::invalid("conditions"))
    );
}

#[test]
fn owners_and_game_masters_set_conditions() {
    for (role, n, allowed) in [
        (Role::Gm, 1, true),
        (Role::CoGm, 1, true),
        (Role::Player, 0, true),
        (Role::Player, 1, false),
        (Role::Spectator, 0, false),
    ] {
        let mut state = srd_table();
        let result = set_conditions(&mut state, role, n, &["blinded"]);
        assert_eq!(result.is_ok(), allowed, "{role:?} on {n}: {result:?}");
        if !allowed {
            assert_eq!(result, Err(RejectReason::Forbidden));
        }
    }
}

#[test]
fn game_masters_give_items_into_the_inventory_list() {
    let mut state = srd_table();
    for name in ["Rope", "Lantern"] {
        let version = state.characters[&hero(0)].version;
        run(
            &mut state,
            Role::Gm,
            Intent::GiveItem {
                id: hero(0),
                name: format!("  {name} "),
            },
            Some(version),
        )
        .unwrap();
    }
    let rules = state.rules().unwrap().clone();
    let sheet = &state.characters[&hero(0)].sheet_json;
    assert_eq!(inventory_names(&rules, sheet), ["Rope", "Lantern"]);
    let value: serde_json::Value = serde_json::from_str(sheet).unwrap();
    assert_eq!(value["equipment"][0]["quantity"], 1);

    let version = state.characters[&hero(0)].version;
    let give = || Intent::GiveItem {
        id: hero(0),
        name: "Gold".into(),
    };
    assert_eq!(
        run(&mut state.clone(), Role::Player, give(), Some(version)),
        Err(RejectReason::Forbidden)
    );
    let mut bare = table_state();
    let version = bare.characters[&hero(0)].version;
    assert_eq!(
        run(&mut bare, Role::Gm, give(), Some(version)),
        Err(RejectReason::invalid("name"))
    );
}

#[test]
fn system_rolls_use_the_systems_formula() {
    let mut state = srd_table();
    let roll = |state: &mut GameState, expression: &str, id: &str| {
        run(
            state,
            Role::Player,
            Intent::RollDice {
                id: ChatMessageId(v7(0x950)),
                expression: expression.into(),
                hidden: false,
                character_id: Some(hero(0)),
                roll: Some(id.into()),
            },
            None,
        )
    };
    assert!(roll(&mut state.clone(), "1d20 + @perception", "perception").is_ok());
    // A forged formula under a system roll's name is refused.
    assert_eq!(
        roll(&mut state.clone(), "1d20 + 100", "perception"),
        Err(RejectReason::invalid("expression"))
    );
    assert_eq!(
        roll(&mut state, "1d20", "flying"),
        Err(RejectReason::invalid("roll"))
    );
}

#[test]
fn rules_check_conditions_and_inventory() {
    let plugin = srd();
    assert_eq!(check_plugin(&plugin), Ok(()));
    let with = |edit: &dyn Fn(&mut SystemRulesDto)| {
        let mut plugin = plugin.clone();
        edit(plugin.system.as_mut().unwrap());
        check_plugin(&plugin)
    };
    assert_eq!(
        with(&|r| r.conditions.push(r.conditions[0].clone())),
        Err(RejectReason::invalid("conditions"))
    );
    assert_eq!(
        with(&|r| r.conditions[0].id = "Not An Id".into()),
        Err(RejectReason::invalid("conditions"))
    );
    assert_eq!(
        with(&|r| r.inventory.as_mut().unwrap().path = "notes".into()),
        Err(RejectReason::invalid("inventory"))
    );
    assert_eq!(
        with(&|r| r.inventory.as_mut().unwrap().name_field = "quantity".into()),
        Err(RejectReason::invalid("inventory"))
    );
}
