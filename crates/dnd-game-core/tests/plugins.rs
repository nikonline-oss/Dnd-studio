//! Plugins and system rules (ADR-0023), with the system plugins shipped in `plugins/`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use std::path::Path;

use common::*;
use dnd_game_core::{GameState, check_plugin, default_sheet};
use dnd_protocol::arbitrary::v7;
use dnd_protocol::*;
use serde_json::{Value, json};

fn bundled(dir: &str) -> PluginDto {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../plugins")
        .join(dir);
    let bytes = dnd_plugin::pack_directory(&path).unwrap();
    dnd_plugin::read_package(&bytes, dnd_plugin::PackageLimits::default())
        .unwrap()
        .to_dto()
}

#[test]
fn creature_form_must_be_an_object_and_legacy_systems_need_no_form() {
    let mut plugin = bundled("system-generic-d6");
    let rules = plugin.system.as_mut().unwrap();
    rules.creature_sheet_layout = None;
    assert!(check_plugin(&plugin).is_ok());
    plugin.system.as_mut().unwrap().creature_sheet_layout = Some(json!("script"));
    assert_eq!(
        check_plugin(&plugin),
        Err(RejectReason::invalid("creature_sheet_layout"))
    );
}

fn install(state: &mut GameState, plugin: PluginDto, system: bool) {
    let id = plugin.id.clone();
    run(
        state,
        Role::Gm,
        Intent::InstallPlugin {
            plugin: Box::new(plugin),
        },
        None,
    )
    .unwrap();
    if system {
        run(
            state,
            Role::Gm,
            Intent::SetSystemPlugin { id: Some(id) },
            None,
        )
        .unwrap();
    }
}

fn create_character(
    state: &mut GameState,
    n: u128,
    sheet: &Value,
) -> Result<CharacterDto, RejectReason> {
    let id = CharacterId(v7(0x500 + n));
    run(
        state,
        Role::Gm,
        Intent::CreateCharacter {
            id,
            character_kind: CharacterKind::Npc,
            name: format!("Hero {n}"),
            owner: None,
            sheet_json: sheet.to_string(),
        },
        None,
    )?;
    Ok(state.characters[&id].clone())
}

fn roll(state: &mut GameState, expression: &str, character: CharacterId) -> DiceRollDto {
    let events = run(
        state,
        Role::Gm,
        Intent::RollDice {
            id: ChatMessageId(v7(0x900)),
            expression: expression.into(),
            hidden: false,
            character_id: Some(character),
            roll: None,
        },
        None,
    )
    .unwrap();
    match &events[..] {
        [Event::ChatMessagePosted { message }] => message.roll.clone().unwrap(),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn bundled_systems_pass_the_cores_own_checks() {
    for dir in ["system-srd5", "system-generic-d6"] {
        let plugin = bundled(dir);
        assert_eq!(check_plugin(&plugin), Ok(()), "{dir}");
        let defaults = default_sheet(plugin.system.as_ref().unwrap());
        assert!(defaults.is_object());
    }
}

#[test]
fn only_game_masters_manage_plugins() {
    let plugin = bundled("system-srd5");
    let intents = [
        Intent::InstallPlugin {
            plugin: Box::new(plugin.clone()),
        },
        Intent::SetSystemPlugin {
            id: Some("srd5".into()),
        },
        Intent::UninstallPlugin { id: "srd5".into() },
    ];
    for role in [Role::Player, Role::Spectator] {
        let mut state = base_state();
        install(&mut state, plugin.clone(), false);
        for intent in &intents {
            assert_eq!(
                run(&mut state, role, intent.clone(), None),
                Err(RejectReason::Forbidden),
                "{role:?} {}",
                intent.kind()
            );
        }
    }
    // Co-GM: install, make it the system, leave it again, uninstall.
    let mut state = base_state();
    let [install_srd5, set_srd5, uninstall_srd5] = intents;
    for intent in [
        install_srd5,
        set_srd5,
        Intent::SetSystemPlugin { id: None },
        uninstall_srd5,
    ] {
        run(&mut state, Role::CoGm, intent, None).unwrap();
    }
    assert!(state.plugins.is_empty());
    assert_eq!(state.system_plugin, None);
}

#[test]
fn the_system_fills_and_checks_sheets() {
    let mut state = base_state();
    install(&mut state, bundled("system-srd5"), true);
    let hero = create_character(&mut state, 1, &json!({})).unwrap();
    let sheet: Value = serde_json::from_str(&hero.sheet_json).unwrap();
    assert_eq!(sheet["level"], 1);
    assert_eq!(sheet["abilities"]["str"], 10);
    assert_eq!(sheet["hp"]["max"], 10);
    assert_eq!(sheet["skills"]["stealth"], false);

    // Out of range, unknown fields, wrong types.
    for bad in [
        json!({ "abilities": { "str": 99 } }),
        json!({ "wings": 2 }),
        json!({ "level": "one" }),
    ] {
        assert_eq!(
            create_character(&mut state, 2, &bad),
            Err(RejectReason::invalid("sheet_json")),
            "{bad}"
        );
    }

    // Updates are normalized the same way.
    let update = json!({ "level": 5, "abilities": { "str": 16 } }).to_string();
    run(
        &mut state,
        Role::Gm,
        Intent::UpdateCharacter {
            id: hero.id,
            name: None,
            sheet_json: Some(update),
        },
        Some(hero.version),
    )
    .unwrap();
    let sheet: Value = serde_json::from_str(&state.characters[&hero.id].sheet_json).unwrap();
    assert_eq!(
        (sheet["level"].clone(), sheet["abilities"]["dex"].clone()),
        (json!(5), json!(10))
    );
}

#[test]
fn rolls_see_computed_fields() {
    let mut state = base_state();
    install(&mut state, bundled("system-srd5"), true);
    let hero = create_character(
        &mut state,
        1,
        &json!({ "level": 5, "abilities": { "str": 16, "dex": 14 }, "skills": { "stealth": true } }),
    )
    .unwrap();
    // str_mod = 3; stealth = dex_mod 2 + proficiency 3 (level 5).
    for (expression, bonus) in [
        ("1d20 + @str_mod", 3),
        ("1d20 + @stealth", 5),
        ("1d20 + @abilities.dex", 14),
    ] {
        let result = roll(&mut state, expression, hero.id);
        assert!(
            (1 + bonus..=20 + bonus).contains(&result.total),
            "{expression}: {}",
            result.total
        );
    }
}

#[test]
fn initiative_follows_the_system() {
    for (dir, sheet, low, high) in [
        ("system-srd5", json!({ "abilities": { "dex": 18 } }), 5, 24),
        (
            "system-generic-d6",
            json!({ "attributes": { "agility": 3 } }),
            5,
            15,
        ),
    ] {
        let mut state = table_state();
        install(&mut state, bundled(dir), true);
        let hero = create_character(&mut state, 1, &sheet).unwrap();
        let combat = CombatId(v7(0x600));
        let combatant = CombatantId(v7(0x601));
        run(
            &mut state,
            Role::Gm,
            Intent::StartCombat {
                id: combat,
                map_id: dnd_protocol::arbitrary::pooled_map_id(0),
            },
            None,
        )
        .unwrap();
        run(
            &mut state,
            Role::Gm,
            Intent::AddCombatant {
                id: combatant,
                combat_id: combat,
                token_id: None,
                character_id: Some(hero.id),
                name: "Hero".into(),
            },
            None,
        )
        .unwrap();
        let version = state.combatants[&combatant].version;
        run(
            &mut state,
            Role::Gm,
            Intent::RollInitiative { id: combatant },
            Some(version),
        )
        .unwrap();
        let initiative = state.combatants[&combatant].initiative.unwrap();
        assert!((low..=high).contains(&initiative), "{dir}: {initiative}");
    }
}

#[test]
fn the_active_system_stays_until_replaced() {
    let mut state = base_state();
    install(&mut state, bundled("system-srd5"), true);
    install(&mut state, bundled("system-generic-d6"), false);
    let uninstall = |state: &mut GameState, id: &str| {
        run(
            state,
            Role::Gm,
            Intent::UninstallPlugin { id: id.into() },
            None,
        )
    };
    assert_eq!(
        uninstall(&mut state, "srd5"),
        Err(RejectReason::invalid("id"))
    );
    assert_eq!(
        uninstall(&mut state, "nothing"),
        Err(RejectReason::NotFound)
    );
    let set = |state: &mut GameState, id: Option<&str>| {
        run(
            state,
            Role::Gm,
            Intent::SetSystemPlugin {
                id: id.map(str::to_owned),
            },
            None,
        )
    };
    assert_eq!(
        set(&mut state, Some("nothing")),
        Err(RejectReason::NotFound)
    );
    assert_eq!(
        set(&mut state, Some("srd5")),
        Err(RejectReason::invalid("id")),
        "no change"
    );
    set(&mut state, Some("generic-d6")).unwrap();
    uninstall(&mut state, "srd5").unwrap();
    assert_eq!(
        state.rules().unwrap().initiative,
        "2d6 + @attributes.agility"
    );
    set(&mut state, None).unwrap();
    assert!(state.rules().is_none());
}

#[test]
fn broken_rules_are_refused() {
    let good = bundled("system-srd5");
    let broken = |f: &dyn Fn(&mut SystemRulesDto)| {
        let mut plugin = good.clone();
        f(plugin.system.as_mut().unwrap());
        check_plugin(&plugin)
    };
    #[allow(clippy::type_complexity)]
    let cases: Vec<(&str, Box<dyn Fn(&mut SystemRulesDto)>)> = vec![
        (
            "computed",
            Box::new(|r| {
                r.computed.insert("a".into(), "@b + 1".into());
                r.computed.insert("b".into(), "@a + 1".into());
            }),
        ),
        (
            "computed",
            Box::new(|r| {
                r.computed.insert("x".into(), "@nowhere".into());
            }),
        ),
        (
            "computed",
            Box::new(|r| {
                r.computed.insert("half".into(), "@level / 2".into());
            }),
        ),
        (
            "sheet_schema",
            Box::new(|r| {
                r.sheet_schema["properties"]["notes"]["pattern"] = json!(".*");
            }),
        ),
        ("sheet_layout", Box::new(|r| r.sheet_layout = json!([]))),
        (
            "health_thresholds",
            Box::new(|r| r.health_thresholds = vec![0, 25, 75]),
        ),
        (
            "dice",
            Box::new(|r| r.initiative = "1d20 + @missing".into()),
        ),
        ("dice", Box::new(|r| r.rolls[0].dice = "roll a d20".into())),
        (
            "resources",
            Box::new(|r| r.resources[0].max = "@hp.nothing".into()),
        ),
        (
            "ids",
            Box::new(|r| {
                let first = r.rolls[0].clone();
                r.rolls.push(first);
            }),
        ),
    ];
    for (field, change) in cases {
        assert_eq!(
            broken(&*change),
            Err(RejectReason::invalid(field)),
            "{field}"
        );
    }
    let mut content = good.clone();
    content.kind = PluginKind::Content;
    assert_eq!(check_plugin(&content), Err(RejectReason::invalid("system")));
    let mut hashless = good;
    hashless.archive_sha256 = "x".into();
    assert_eq!(
        check_plugin(&hashless),
        Err(RejectReason::invalid("archive_sha256"))
    );
}
