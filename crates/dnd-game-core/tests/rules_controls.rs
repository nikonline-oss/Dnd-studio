#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use common::*;
use dnd_game_core::{check_plugin, default_sheet, snapshot_for_user};
use dnd_protocol::arbitrary::{pooled_character_id as cid, pooled_token_id as tid, v7};
use dnd_protocol::*;
fn world() -> dnd_game_core::GameState {
    let mut s = table_state();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/system-srd5");
    let mut p = dnd_plugin::read_package(
        &dnd_plugin::pack_directory(&dir).unwrap(),
        Default::default(),
    )
    .unwrap()
    .to_dto();
    check_plugin(&p).unwrap();
    let rules = p.system.as_mut().unwrap();
    let a = rules
        .mechanics
        .actions
        .iter_mut()
        .find(|a| a.id == "fireball")
        .unwrap();
    a.damage_dice = "10".into();
    a.save_dice = Some("20".into());
    a.costs.clear();
    let mut sheet = default_sheet(rules);
    sheet["hp"]["current"] = 100.into();
    sheet["hp"]["max"] = 100.into();
    for c in s.characters.values_mut() {
        c.sheet_json = sheet.to_string();
    }
    s.tokens.retain(|id, _| *id == tid(0) || *id == tid(1));
    s.system_plugin = Some(p.id.clone());
    s.plugins.insert(p.id.clone(), p);
    s
}
fn fire(s: &mut dnd_game_core::GameState) {
    run(
        s,
        Role::Gm,
        Intent::ExecutePluginAction {
            source: tid(0),
            action: "fireball".into(),
            target: None,
            x: 0,
            y: 0,
        },
        None,
    )
    .unwrap();
}
fn hp(s: &dnd_game_core::GameState, n: u128) -> i64 {
    serde_json::from_str::<serde_json::Value>(&s.characters[&cid(n)].sheet_json).unwrap()["hp"]["current"].as_i64().unwrap()
}
#[test]
fn each_target_owner_rolls_once_and_caster_cannot_roll_for_another_player() {
    let mut s = world();
    fire(&mut s);
    assert_eq!(hp(&s, 0), 100);
    assert_eq!(hp(&s, 1), 100);
    let id = s.plugin_runtime.saves[0].id;
    assert!(
        run(
            &mut s,
            Role::Player,
            Intent::ResolveActionSave {
                group: id,
                target: tid(1)
            },
            None
        )
        .is_err()
    );
    run(
        &mut s,
        Role::Player,
        Intent::ResolveActionSave {
            group: id,
            target: tid(0),
        },
        None,
    )
    .unwrap();
    assert_eq!(hp(&s, 0), 95);
    assert_eq!(hp(&s, 1), 100);
    assert!(
        run(
            &mut s,
            Role::Player,
            Intent::ResolveActionSave {
                group: id,
                target: tid(0)
            },
            None
        )
        .is_err()
    );
    let view = snapshot_for_user(Role::Player, PLAYER, &s, Seq(0), []);
    assert!(
        view.plugin_runtime.saves[0]
            .targets
            .iter()
            .all(|t| t.character == cid(0))
    );
    run(
        &mut s,
        Role::Gm,
        Intent::ResolveActionSave {
            group: id,
            target: tid(1),
        },
        None,
    )
    .unwrap();
    assert_eq!(hp(&s, 1), 95);
    assert!(s.plugin_runtime.saves.is_empty());
}
#[test]
fn unconscious_fails_automatically_but_incapacitated_still_gets_a_save() {
    let mut s = world();
    s.characters.get_mut(&cid(0)).unwrap().conditions = vec!["unconscious".into()];
    s.characters.get_mut(&cid(1)).unwrap().conditions = vec!["incapacitated".into()];
    // Actor must be able to cast; use an independent caster character on the same map.
    let mut caster = s.characters[&cid(0)].clone();
    caster.id = cid(2);
    caster.conditions.clear();
    s.characters.insert(caster.id, caster);
    s.tokens.get_mut(&tid(0)).unwrap().character_id = Some(cid(2));
    let mut victim = s.tokens[&tid(1)].clone();
    victim.id = tid(2);
    victim.character_id = Some(cid(0));
    s.tokens.insert(victim.id, victim);
    fire(&mut s);
    assert_eq!(hp(&s, 0), 90);
    assert_eq!(hp(&s, 1), 100);
    assert!(
        s.plugin_runtime.saves[0]
            .targets
            .iter()
            .any(|t| t.character == cid(1) && t.status == SaveStatus::Pending)
    );
}
#[test]
fn item_instances_have_independent_state_and_owner_permissions() {
    let mut s = world();
    let template = ItemId(v7(901));
    let a = ItemId(v7(902));
    let b = ItemId(v7(903));
    run(
        &mut s,
        Role::Gm,
        Intent::CreateItem {
            id: template,
            item_kind: ItemKind::Item,
            name: "Wand".into(),
            type_id: Some("equipment".into()),
            description: String::new(),
            sheet_json: Some(r#"{"charges":3}"#.into()),
            visible_to_players: false,
        },
        None,
    )
    .unwrap();
    for id in [a, b] {
        run(
            &mut s,
            Role::Gm,
            Intent::InstantiateItem {
                id,
                template,
                owner: Some(cid(0)),
            },
            None,
        )
        .unwrap();
    }
    run(
        &mut s,
        Role::Player,
        Intent::UpdateItem {
            id: a,
            name: None,
            description: None,
            sheet_json: Some(r#"{"charges":2,"state":"used"}"#.into()),
            visible_to_players: None,
            state: None,
            effects: None,
            grants: None,
            values: None,
        },
        Some(Version(1)),
    )
    .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&s.items[&b].sheet_json).unwrap()["charges"],
        3
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&s.items[&template].sheet_json).unwrap()["charges"],
        3
    );
    run(
        &mut s,
        Role::Gm,
        Intent::TransferItem {
            id: a,
            owner: Some(cid(1)),
        },
        Some(Version(2)),
    )
    .unwrap();
    assert!(
        run(
            &mut s,
            Role::Player,
            Intent::UpdateItem {
                id: a,
                name: Some("Other".into()),
                description: None,
                sheet_json: None,
                visible_to_players: None,
                state: None,
                effects: None,
                grants: None,
                values: None,
            },
            Some(Version(3))
        )
        .is_err()
    );
    let view = snapshot_for_user(Role::Player, PLAYER, &s, Seq(0), []);
    assert!(!view.items.iter().any(|i| i.id == a));
    assert!(view.items.iter().any(|i| i.id == b));
}
#[test]
fn earth_calendar_handles_leap_years_and_month_clamping() {
    let mut s = world();
    for (amount, unit) in [
        (3, CalendarUnit::Year),
        (1, CalendarUnit::Month),
        (28, CalendarUnit::Day),
    ] {
        let v = s.campaign.game_clock.version;
        run(
            &mut s,
            Role::Gm,
            Intent::AdvanceCalendarTime { amount, unit },
            Some(v),
        )
        .unwrap();
    }
    let view = s.to_snapshot(Seq(0)).rules_view.calendar.unwrap();
    assert_eq!((view.year, view.month_index, view.day), (4, 1, 29));
    let v = s.campaign.game_clock.version;
    run(
        &mut s,
        Role::CoGm,
        Intent::AdvanceCalendarTime {
            amount: 1,
            unit: CalendarUnit::Year,
        },
        Some(v),
    )
    .unwrap();
    let view = s.to_snapshot(Seq(0)).rules_view.calendar.unwrap();
    assert_eq!((view.year, view.month_index, view.day), (5, 1, 28));
}

#[test]
fn plugin_calendar_replaces_earth_without_changing_the_clock() {
    let mut s = world();
    let p = s
        .plugins
        .get_mut(s.system_plugin.as_ref().unwrap())
        .unwrap();
    p.system.as_mut().unwrap().mechanics.calendar = Some(CalendarDef {
        epoch_year: 100,
        day_seconds: 1000,
        round_seconds: 5,
        months: vec![
            CalendarMonth {
                label: Label::Text("First".into()),
                days: 10,
            },
            CalendarMonth {
                label: Label::Text("Second".into()),
                days: 10,
            },
        ],
        weekdays: vec![Label::Text("One".into()), Label::Text("Two".into())],
        periods: vec![
            DayPeriod {
                label: Label::Text("Dark".into()),
                start_second: 0,
            },
            DayPeriod {
                label: Label::Text("Light".into()),
                start_second: 500,
            },
        ],
        leap_interval: 0,
        leap_except: 0,
        leap_include: 0,
        leap_month: 0,
    });
    let v = s.campaign.game_clock.version;
    run(
        &mut s,
        Role::Gm,
        Intent::AdvanceCalendarTime {
            amount: 1,
            unit: CalendarUnit::Month,
        },
        Some(v),
    )
    .unwrap();
    assert_eq!(s.campaign.game_clock.seconds, 10000);
    let date = s.to_snapshot(Seq(0)).rules_view.calendar.unwrap();
    assert_eq!((date.year, date.month_index, date.day), (100, 1, 1));
}
#[test]
fn outside_combat_ignores_action_cost_but_still_requires_spell_slots() {
    let mut s = world();
    let p = s
        .plugins
        .get_mut(s.system_plugin.as_ref().unwrap())
        .unwrap();
    let action = p
        .system
        .as_mut()
        .unwrap()
        .mechanics
        .actions
        .iter_mut()
        .find(|a| a.id == "fireball")
        .unwrap();
    action.costs = vec![
        ActionCost {
            resource: "action".into(),
            amount: "1".into(),
            on_resolution: false,
            scope: CostScope::Combat,
            feature: None,
        },
        ActionCost {
            resource: "spell_slot_3".into(),
            amount: "1".into(),
            on_resolution: false,
            scope: CostScope::Always,
            feature: None,
        },
    ];
    let mut data: serde_json::Value =
        serde_json::from_str(&s.characters[&cid(0)].sheet_json).unwrap();
    data["automation"]["actions"] = 0.into();
    data["automation"]["spell_slots_3"] = 0.into();
    s.characters.get_mut(&cid(0)).unwrap().sheet_json = data.to_string();
    let enabled = |s: &dnd_game_core::GameState| {
        dnd_game_core::rules_view(s)
            .tokens
            .iter()
            .find(|t| t.token == tid(0))
            .unwrap()
            .actions
            .iter()
            .find(|a| a.id == "fireball")
            .unwrap()
            .enabled
    };
    assert!(!enabled(&s));
    data["automation"]["spell_slots_3"] = 1.into();
    s.characters.get_mut(&cid(0)).unwrap().sheet_json = data.to_string();
    assert!(enabled(&s));
}
#[test]
fn changing_pending_target_to_unconscious_finishes_its_save_without_a_roll() {
    let mut s = world();
    fire(&mut s);
    let v = s.characters[&cid(1)].version;
    run(
        &mut s,
        Role::Gm,
        Intent::SetConditions {
            id: cid(1),
            conditions: vec!["unconscious".into()],
        },
        Some(v),
    )
    .unwrap();
    assert_eq!(hp(&s, 1), 90);
    assert!(
        s.plugin_runtime.saves[0]
            .targets
            .iter()
            .any(|t| t.character == cid(1) && t.status == SaveStatus::AutomaticFailure)
    );
}

#[test]
fn trigger_waits_for_reaction_then_for_all_target_saves() {
    let mut s = world();
    let mut sheet: serde_json::Value =
        serde_json::from_str(&s.characters[&cid(1)].sheet_json).unwrap();
    sheet["automation"]["can_counterspell"] = true.into();
    sheet["automation"]["spell_slots_3"] = 1.into();
    s.characters.get_mut(&cid(1)).unwrap().sheet_json = sheet.to_string();
    let trigger = TriggerId(v7(9991));
    run(
        &mut s,
        Role::Gm,
        Intent::CreateTrigger {
            id: trigger,
            definition: TriggerDef {
                name: "Spell sequence".into(),
                enabled: true,
                once: true,
                event: TriggerEvent::Manual,
                map_ids: vec![],
                tokens: vec![],
                condition: None,
                steps: vec![
                    TriggerStep {
                        when: None,
                        action: TriggerAction::PluginAction {
                            action: "fireball".into(),
                            source: Some(tid(0)),
                            target: None,
                            x: 0,
                            y: 0,
                        },
                    },
                    TriggerStep {
                        when: Some("@action.completed == 1".into()),
                        action: TriggerAction::Chat {
                            text: "All targets finished".into(),
                            gm_only: false,
                        },
                    },
                ],
            },
        },
        None,
    )
    .unwrap();
    let events = run(
        &mut s,
        Role::Gm,
        Intent::FireTrigger {
            id: trigger,
            token: Some(tid(0)),
        },
        None,
    )
    .unwrap();
    let finished = |events: &[Event]| {
        events.iter().filter(|e|matches!(e,Event::ChatMessagePosted{message} if matches!(&message.system, Some(SystemMessage::Narration{text}) if text == "All targets finished"))).count()
    };
    assert_eq!(finished(&events), 0);
    assert_eq!(hp(&s, 0), 100);
    assert!(s.plugin_runtime.saves.is_empty());
    assert_eq!(
        s.plugin_runtime.trigger_runs.len(),
        1,
        "run after declaration"
    );
    let stack = s.plugin_runtime.action_stacks[0].clone();
    let frame = stack.frames.last().unwrap();
    let events = run(
        &mut s,
        Role::Gm,
        Intent::ResolveActionReaction {
            stack: stack.id,
            frame: frame.id,
            source: tid(1),
            accept: false,
        },
        None,
    )
    .unwrap();
    assert_eq!(finished(&events), 0);
    assert!(s.plugin_runtime.action_stacks.is_empty());
    assert_eq!(s.plugin_runtime.saves.len(), 1);
    assert_eq!(hp(&s, 0), 100);
    assert_eq!(s.plugin_runtime.trigger_runs.len(), 1, "run after reaction");
    let group = s.plugin_runtime.saves[0].clone();
    let mut count = 0;
    for target in group.targets {
        if target.status == SaveStatus::Pending {
            let events = run(
                &mut s,
                Role::Gm,
                Intent::ResolveActionSave {
                    group: group.id,
                    target: target.token,
                },
                None,
            )
            .unwrap();
            count += finished(&events);
        }
    }
    assert_eq!(count, 1);
    assert!(s.plugin_runtime.saves.is_empty());
    assert!(s.plugin_runtime.trigger_runs.is_empty());
}

#[test]
fn dead_participants_are_skipped_without_removing_hero_entries() {
    let mut s = world();
    let combat = CombatId(v7(9201));
    run(
        &mut s,
        Role::Gm,
        Intent::StartCombat {
            id: combat,
            map_id: dnd_protocol::arbitrary::pooled_map_id(0),
        },
        None,
    )
    .unwrap();
    for n in [0, 1] {
        run(
            &mut s,
            Role::Gm,
            Intent::AddCombatant {
                id: CombatantId(v7(9210 + n)),
                combat_id: combat,
                token_id: Some(tid(n)),
                character_id: Some(cid(n)),
                name: format!("Fighter {n}"),
            },
            None,
        )
        .unwrap();
    }
    s.characters.get_mut(&cid(0)).unwrap().conditions = vec!["dead".into()];
    let v = s.combats[&combat].version;
    run(
        &mut s,
        Role::Gm,
        Intent::NextTurn { combat_id: combat },
        Some(v),
    )
    .unwrap();
    assert_eq!(s.combats[&combat].turn, Some(CombatantId(v7(9211))));
    assert_eq!(s.combatants.len(), 2);
    assert_eq!(
        dnd_game_core::rules_view(&s)
            .characters
            .iter()
            .find(|c| c.character == cid(0))
            .unwrap()
            .life,
        LifeStatus::Dead
    );
}

#[test]
fn deleting_a_character_detaches_owned_items_atomically() {
    let mut s = world();
    let id = ItemId(v7(9900));
    s.items.insert(
        id,
        ItemDto {
            id,
            kind: ItemKind::Item,
            name: "Owned wand".into(),
            type_id: Some("equipment".into()),
            is_template: false,
            template: None,
            owner: Some(cid(0)),
            visible_to_players: true,
            description: String::new(),
            state: None,
            effects: Vec::new(),
            grants: Vec::new(),
            values: Vec::new(),
            sheet_json: "{}".into(),
            version: Version(i64::MAX),
        },
    );
    let version = s.characters[&cid(0)].version;
    let before = s.clone();
    assert!(
        run(
            &mut s,
            Role::Gm,
            Intent::DeleteCharacter { id: cid(0) },
            Some(version)
        )
        .is_err()
    );
    assert_eq!(s, before);
    s.items.get_mut(&id).unwrap().version = Version(1);
    run(
        &mut s,
        Role::Gm,
        Intent::DeleteCharacter { id: cid(0) },
        Some(version),
    )
    .unwrap();
    assert_eq!(s.items[&id].owner, None);
    assert!(!s.items[&id].visible_to_players);
    assert_eq!(s.items[&id].version, Version(2));
}

fn row_action(s: &dnd_game_core::GameState, collection: &str) -> ActionAvailability {
    dnd_game_core::rules_view(s)
        .tokens
        .into_iter()
        .find(|t| t.token == tid(0))
        .unwrap()
        .actions
        .into_iter()
        .find(|a| {
            a.sheet_action
                .as_ref()
                .is_some_and(|r| r.collection == collection)
        })
        .unwrap()
}
fn edit_sheet(
    s: &mut dnd_game_core::GameState,
    n: u128,
    edit: impl FnOnce(&mut serde_json::Value),
) {
    let c = &s.characters[&cid(n)];
    let mut data: serde_json::Value = serde_json::from_str(&c.sheet_json).unwrap();
    edit(&mut data);
    let version = c.version;
    run(
        s,
        Role::Gm,
        Intent::UpdateCharacter {
            id: cid(n),
            name: None,
            sheet_json: Some(data.to_string()),
        },
        Some(version),
    )
    .unwrap();
}
#[test]
fn sheet_attack_uses_its_own_damage_range_and_bonus_with_authoritative_permissions() {
    let mut s = world();
    edit_sheet(
        &mut s,
        0,
        |v| v["attacks"] = serde_json::json!([{"name":"Ice lance","action":"ranged_attack","bonus":17,"damage":"1d8+3","range":12,"damage_type":"cold"}]),
    );
    let available = row_action(&s, "attacks");
    let row = available.sheet_action.unwrap();
    assert!(available.enabled);
    assert_eq!(available.range_cells, 12);
    assert_eq!(row.action.label.get("en"), "Ice lance");
    assert_eq!(row.action.damage_dice, "d8+3");
    assert_eq!(row.action.critical_dice.as_deref(), Some("0+d8"));
    assert_eq!(row.action.attack_dice.as_deref(), Some("0+d20+17"));
    assert_eq!(row.action.tags, ["weapon", "ranged"]);
    let cast = Intent::ExecutePluginAction {
        source: tid(0),
        action: available.id,
        target: Some(tid(1)),
        x: 0,
        y: 0,
    };
    let before = s.clone();
    assert!(run(&mut s, Role::Spectator, cast.clone(), None).is_err());
    assert!(
        dnd_game_core::apply(
            &s,
            &envelope(cast.clone(), None),
            &dnd_game_core::Actor {
                role: Role::Player,
                user_id: OTHER_PLAYER
            },
            &mut dnd_game_core::SplitMix64::new(0)
        )
        .is_err()
    );
    assert_eq!(s, before);
    let events = run(&mut s, Role::Player, cast, None).unwrap();
    assert!(events.iter().any(|e| matches!(e, Event::ChatMessagePosted { message } if message.body == "Ice lance" && message.roll.as_ref().is_some_and(|r| r.expression.contains("d8")))));
    assert!(hp(&s, 1) < 100);
}
#[test]
fn stale_row_action_cannot_cast_a_different_row_and_invalid_dice_cannot_execute() {
    let mut s = world();
    edit_sheet(
        &mut s,
        0,
        |v| v["attacks"] = serde_json::json!([{"name":"First","bonus":2,"damage":"3"},{"name":"Second","bonus":4,"damage":"9"}]),
    );
    let id = row_action(&s, "attacks").id;
    edit_sheet(&mut s, 0, |v| {
        v["attacks"].as_array_mut().unwrap().remove(0);
    });
    let before = s.clone();
    assert!(
        run(
            &mut s,
            Role::Player,
            Intent::ExecutePluginAction {
                source: tid(0),
                action: id,
                target: Some(tid(1)),
                x: 0,
                y: 0
            },
            None
        )
        .is_err()
    );
    assert_eq!(s, before);
    edit_sheet(&mut s, 0, |v| v["attacks"][0]["damage"] = "not dice".into());
    assert!(
        dnd_game_core::rules_view(&s)
            .tokens
            .iter()
            .find(|t| t.token == tid(0))
            .unwrap()
            .actions
            .iter()
            .all(|a| a.sheet_action.is_none())
    );
}
#[test]
fn a_declared_sheet_spell_survives_list_changes_and_restart_without_changing_its_effect() {
    let mut s = world();
    edit_sheet(
        &mut s,
        0,
        |v| {
            v["spells"] = serde_json::json!([{"name":"My fireball","action":"fireball","notes":""}])
        },
    );
    edit_sheet(&mut s, 1, |v| {
        v["automation"]["can_counterspell"] = true.into();
        v["automation"]["spell_slots_3"] = 1.into();
    });
    let id = row_action(&s, "spells").id;
    run(
        &mut s,
        Role::Player,
        Intent::ExecutePluginAction {
            source: tid(0),
            action: id,
            target: None,
            x: 0,
            y: 0,
        },
        None,
    )
    .unwrap();
    let frame = s.plugin_runtime.action_stacks[0].frames[0].clone();
    assert!(frame.definition.is_some());
    edit_sheet(&mut s, 0, |v| v["spells"] = serde_json::json!([]));
    let json = serde_json::to_string(&s.to_snapshot(Seq(42))).unwrap();
    s = dnd_game_core::GameState::from_snapshot(serde_json::from_str(&json).unwrap());
    let stack = s.plugin_runtime.action_stacks[0].id;
    run(
        &mut s,
        Role::Gm,
        Intent::ResolveActionReaction {
            stack,
            frame: frame.id,
            source: tid(1),
            accept: false,
        },
        None,
    )
    .unwrap();
    assert!(s.plugin_runtime.action_stacks.is_empty());
    let save = &s.plugin_runtime.saves[0];
    assert_eq!(save.action.label.get("en"), "My fireball");
    assert_eq!(save.damage, 10);
    assert_eq!(hp(&s, 1), 100);
}

#[test]
fn sheet_action_collections_reject_unknown_rules_invalid_fields_and_unbounded_lists() {
    let s = world();
    let plugin = &s.plugins[s.system_plugin.as_ref().unwrap()];
    for case in 0..5 {
        let mut p = plugin.clone();
        let rules = p.system.as_mut().unwrap();
        match case {
            0 => rules.mechanics.sheet_actions[0].actions = vec!["missing".into()],
            1 => rules.mechanics.sheet_actions[0].attack_dice = Some("1d20 + @row.name".into()),
            2 => {
                rules.mechanics.sheet_actions[0]
                    .bindings
                    .insert(SheetActionField::RangeCells, "damage".into());
            }
            3 => rules.sheet_schema["properties"]["attacks"]["maxItems"] = 101.into(),
            _ => rules.mechanics.sheet_actions[0].actions = vec!["counterspell".into()],
        }
        assert!(check_plugin(&p).is_err(), "invalid collection case {case}");
    }
}

// --- A3.3: pending saves across deletion and restart (ADR-0052) ---

#[test]
fn deleting_a_target_cancels_the_save_group_without_damaging_anyone() {
    let mut s = world();
    fire(&mut s);
    assert_eq!(s.plugin_runtime.saves.len(), 1);
    let version = s.characters[&cid(1)].version;
    run(
        &mut s,
        Role::Gm,
        Intent::DeleteCharacter { id: cid(1) },
        Some(version),
    )
    .unwrap();
    assert!(s.plugin_runtime.saves.is_empty());
    assert_eq!(
        hp(&s, 0),
        100,
        "nobody took damage from the cancelled spell"
    );
}

#[test]
fn a_save_group_survives_a_restart_and_applies_damage_once() {
    let mut s = world();
    fire(&mut s);
    let json = serde_json::to_string(&s.to_snapshot(Seq(7))).unwrap();
    let mut restored: dnd_game_core::GameState = serde_json::from_str(&json)
        .map(dnd_game_core::GameState::from_snapshot)
        .unwrap();
    assert_eq!(restored.plugin_runtime.saves.len(), 1);
    let id = restored.plugin_runtime.saves[0].id;
    for (role, target) in [(Role::Player, tid(0)), (Role::Gm, tid(1))] {
        run(
            &mut restored,
            role,
            Intent::ResolveActionSave { group: id, target },
            None,
        )
        .unwrap();
    }
    assert!(restored.plugin_runtime.saves.is_empty());
    assert_eq!(hp(&restored, 0), 95);
    assert_eq!(hp(&restored, 1), 95);
    // A third answer, after the group is gone, changes nothing.
    assert!(
        run(
            &mut restored,
            Role::Player,
            Intent::ResolveActionSave {
                group: id,
                target: tid(0),
            },
            None
        )
        .is_err()
    );
    assert_eq!(hp(&restored, 0), 95);
}
