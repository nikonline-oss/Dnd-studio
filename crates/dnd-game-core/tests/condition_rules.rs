#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use common::*;
use dnd_game_core::{GameState, check_plugin, default_sheet};
use dnd_protocol::arbitrary::{
    pooled_character_id as cid, pooled_map_id as mid, pooled_token_id as tid, v7,
};
use dnd_protocol::*;

fn world() -> GameState {
    let mut state = table_state();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/system-srd5");
    let bytes = dnd_plugin::pack_directory(&dir).unwrap();
    let mut plugin = dnd_plugin::read_package(&bytes, Default::default())
        .unwrap()
        .to_dto();
    check_plugin(&plugin).unwrap();
    let rules = plugin.system.as_mut().unwrap();
    for a in &mut rules.mechanics.actions {
        for cost in &mut a.costs {
            if a.id != "melee_attack"
                && matches!(
                    cost.resource.as_str(),
                    "action" | "bonus_action" | "reaction"
                )
            {
                cost.feature = None;
            }
        }
        if a.turn_condition.is_none() && a.maintain.is_none() && a.interrupt.is_none() {
            a.damage_dice = "3".into();
        }
        a.critical_if = None;
        a.critical_dice = None;
        if a.attack_dice.is_some() {
            a.attack_dice = Some("1d20".into());
            a.hit = Some("true".into());
        }
    }
    let mut sheet = default_sheet(rules);
    sheet["hp"]["current"] = 100.into();
    sheet["hp"]["max"] = 100.into();
    sheet["automation"]["spell_slots_3"] = 2.into();
    sheet["automation"]["spell_slots_3_max"] = 2.into();
    for c in state.characters.values_mut() {
        c.sheet_json = sheet.to_string();
    }
    plugin.feature_settings.insert("turn_economy".into(), false);
    state.system_plugin = Some(plugin.id.clone());
    state.plugins.insert(plugin.id.clone(), plugin);
    state.tokens.retain(|id, _| *id == tid(0) || *id == tid(1));
    for (n, x, side) in [
        (0, 1120, TokenDisposition::Ally),
        (1, 2240, TokenDisposition::Enemy),
    ] {
        let t = state.tokens.get_mut(&tid(n)).unwrap();
        t.x = x;
        t.y = 1120;
        t.disposition = side;
        t.visible = true;
    }
    let id = CombatId(v7(0x1234));
    state.combats.insert(
        id,
        CombatDto {
            id,
            map_id: mid(0),
            round: 1,
            turn: None,
            version: Version::INITIAL,
        },
    );
    state
}
fn leave(s: &mut GameState) {
    run(
        s,
        Role::Gm,
        Intent::MoveToken {
            id: tid(1),
            x: 10_000,
            y: 1120,
        },
        None,
    )
    .unwrap();
}
fn set_condition(s: &mut GameState, n: u128, id: &str) {
    s.characters.get_mut(&cid(n)).unwrap().conditions = vec![id.into()];
}
fn action(s: &mut GameState, id: &str) -> Result<Vec<Event>, RejectReason> {
    run(
        s,
        Role::Gm,
        Intent::ExecutePluginAction {
            source: tid(0),
            action: id.into(),
            target: Some(tid(1)),
            x: 2240,
            y: 1120,
        },
        None,
    )
}
fn first_d20(events: &[Event]) -> String {
    events
        .iter()
        .find_map(|e| match e {
            Event::ChatMessagePosted { message } => message
                .roll
                .as_ref()
                .and_then(|r| r.terms.iter().find(|t| t.notation.contains("d20")))
                .map(|t| t.notation.clone()),
            _ => None,
        })
        .unwrap()
}

#[test]
fn incapacity_blocks_actions_and_reactions_without_blocking_saves() {
    let mut s = world();
    set_condition(&mut s, 0, "incapacitated");
    let before = s.clone();
    assert!(action(&mut s, "melee_attack").is_err());
    assert_eq!(s, before);
    leave(&mut s);
    assert!(s.plugin_runtime.movements.is_empty());
    let c = s.characters[&cid(0)].clone();
    let def = s
        .rules()
        .unwrap()
        .rolls
        .iter()
        .find(|r| r.id == "dex_save")
        .unwrap();
    let expression = def.dice.clone();
    run(
        &mut s,
        Role::Player,
        Intent::RollDice {
            id: ChatMessageId(v7(0x9810)),
            expression,
            hidden: false,
            character_id: Some(c.id),
            roll: Some("dex_save".into()),
        },
        None,
    )
    .unwrap();
}
#[test]
fn actor_prone_and_target_prone_sources_cancel() {
    let mut s = world();
    set_condition(&mut s, 0, "prone");
    assert!(first_d20(&action(&mut s, "melee_attack").unwrap()).contains("dis"));
    let mut s = world();
    set_condition(&mut s, 1, "prone");
    assert!(first_d20(&action(&mut s, "melee_attack").unwrap()).contains("adv"));
    let mut s = world();
    set_condition(&mut s, 0, "prone");
    set_condition(&mut s, 1, "prone");
    let notation = first_d20(&action(&mut s, "melee_attack").unwrap());
    assert_eq!(notation, "d20");
}
#[test]
fn prone_target_at_distance_gives_disadvantage() {
    let mut s = world();
    set_condition(&mut s, 1, "prone");
    s.tokens.get_mut(&tid(1)).unwrap().x = 4480;
    s.plugins
        .get_mut("srd5")
        .unwrap()
        .system
        .as_mut()
        .unwrap()
        .mechanics
        .actions
        .iter_mut()
        .find(|a| a.id == "melee_attack")
        .unwrap()
        .range_cells = "10".into();
    assert!(first_d20(&action(&mut s, "melee_attack").unwrap()).contains("dis"));
}
#[test]
fn disabling_condition_feature_restores_plain_roll() {
    let mut s = world();
    set_condition(&mut s, 0, "prone");
    s.plugins
        .get_mut("srd5")
        .unwrap()
        .feature_settings
        .insert("condition_rules".into(), false);
    assert_eq!(first_d20(&action(&mut s, "melee_attack").unwrap()), "d20");
}
#[test]
fn temporary_and_permanent_conditions_are_unioned_without_mutation() {
    let mut s = world();
    let effect = TokenEffectDto {
        id: EffectId(v7(0x9820)),
        token: tid(0),
        label: "Test".into(),
        condition: Some("prone".into()),
        resource: None,
        dice: None,
        heal: false,
        reveal: Vec::new(),
        duration: EffectDuration::RealTime { until_ts: 1000 },
        version: Version::INITIAL,
    };
    s.effects.insert(effect.id, effect);
    let raw = s.characters[&cid(0)].conditions.clone();
    assert!(first_d20(&action(&mut s, "melee_attack").unwrap()).contains("dis"));
    assert_eq!(s.characters[&cid(0)].conditions, raw);
}
#[test]
fn disengage_is_a_timed_state_and_expires_at_the_turn_end() {
    let mut s = world();
    let combat_id = CombatId(v7(0x1234));
    let own = CombatantId(v7(0x9821));
    let other = CombatantId(v7(0x9822));
    for (id, token, c) in [(own, tid(0), cid(0)), (other, tid(1), cid(1))] {
        run(
            &mut s,
            Role::Gm,
            Intent::AddCombatant {
                id,
                combat_id,
                token_id: Some(token),
                character_id: Some(c),
                name: "Actor".into(),
            },
            None,
        )
        .unwrap();
    }
    let version = s.combats[&combat_id].version;
    run(
        &mut s,
        Role::Gm,
        Intent::NextTurn { combat_id },
        Some(version),
    )
    .unwrap();
    assert_eq!(s.combats[&combat_id].turn, Some(own));
    action(&mut s, "disengage").unwrap();
    assert!(
        s.effects
            .values()
            .any(|e| e.condition.as_deref() == Some("disengaged"))
    );
    run(
        &mut s,
        Role::Gm,
        Intent::MoveToken {
            id: tid(0),
            x: 9000,
            y: 1120,
        },
        None,
    )
    .unwrap();
    assert!(s.plugin_runtime.movements.is_empty());
    let version = s.combats[&combat_id].version;
    run(
        &mut s,
        Role::Gm,
        Intent::NextTurn { combat_id },
        Some(version),
    )
    .unwrap();
    assert!(
        s.effects
            .values()
            .all(|e| e.condition.as_deref() != Some("disengaged"))
    );
}
#[test]
fn self_effect_outside_own_turn_is_rejected_without_spending() {
    let mut s = world();
    let before = s.clone();
    assert!(action(&mut s, "disengage").is_err());
    assert_eq!(s, before);
}

#[test]
fn numeric_modifiers_recompute_dependants_and_preserve_base_data() {
    let mut s = world();
    let rules = s.plugins.get_mut("srd5").unwrap().system.as_mut().unwrap();
    rules.mechanics.condition_rules.push(ConditionRule {
        id: "stat_test".into(),
        label: Label::Text("Strength +2".into()),
        condition: "prone".into(),
        subject: RuleSubject::Actor,
        feature: None,
        when: String::new(),
        effects: vec![ConditionModifier::Parameter {
            path: "abilities.str".into(),
            add: 2,
            minimum: None,
            maximum: None,
        }],
    });
    set_condition(&mut s, 0, "prone");
    let c = &s.characters[&cid(0)];
    let raw = c.sheet_json.clone();
    let baseline = dnd_game_core::CharacterFields::new(&s, Some(&raw));
    let effective = dnd_game_core::effective_fields(&s, c).unwrap();
    use dnd_dice::FieldLookup;
    assert_eq!(
        effective.lookup("str_mod").unwrap(),
        baseline.lookup("str_mod").unwrap() + 1
    );
    assert_eq!(s.characters[&cid(0)].sheet_json, raw);
}
#[test]
fn inherited_states_are_bounded_and_do_not_duplicate_modifiers() {
    let mut s = world();
    let rules = s.plugins.get_mut("srd5").unwrap().system.as_mut().unwrap();
    rules.mechanics.condition_rules.push(ConditionRule {
        id: "implied".into(),
        label: Label::Text("Implies incapacity".into()),
        condition: "prone".into(),
        subject: RuleSubject::Actor,
        feature: None,
        when: String::new(),
        effects: vec![ConditionModifier::Imply {
            condition: "incapacitated".into(),
        }],
    });
    set_condition(&mut s, 0, "prone");
    assert!(action(&mut s, "melee_attack").is_err());
}
#[test]
fn invalid_references_and_numeric_resource_mutations_are_refused() {
    let mut s = world();
    let plugin = s.plugins.get_mut("srd5").unwrap();
    plugin.system.as_mut().unwrap().mechanics.condition_rules[0].when =
        "@actor.condition.missing".into();
    assert!(check_plugin(plugin).is_err());
    plugin.system.as_mut().unwrap().mechanics.condition_rules[0].when = String::new();
    plugin.system.as_mut().unwrap().mechanics.condition_rules[0].effects =
        vec![ConditionModifier::Parameter {
            path: "hp.current".into(),
            add: 2,
            minimum: None,
            maximum: None,
        }];
    assert!(check_plugin(plugin).is_err());
}

#[test]
fn end_of_turn_expires_on_a_single_combatants_round_wrap_and_preserves_permanent_state() {
    let mut s = world();
    let combat_id = CombatId(v7(0x1234));
    run(
        &mut s,
        Role::Gm,
        Intent::AddCombatant {
            id: CombatantId(v7(0x9890)),
            combat_id,
            token_id: Some(tid(0)),
            character_id: Some(cid(0)),
            name: "Actor".into(),
        },
        None,
    )
    .unwrap();
    let version = s.combats[&combat_id].version;
    run(
        &mut s,
        Role::Gm,
        Intent::NextTurn { combat_id },
        Some(version),
    )
    .unwrap();
    action(&mut s, "disengage").unwrap();
    set_condition(&mut s, 0, "disengaged");
    let bytes = serde_json::to_vec(&s.to_snapshot(Seq(0))).unwrap();
    let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
    s = GameState::from_snapshot(snapshot);
    let version = s.combats[&combat_id].version;
    run(
        &mut s,
        Role::Gm,
        Intent::NextTurn { combat_id },
        Some(version),
    )
    .unwrap();
    assert_eq!(s.combats[&combat_id].round, 2);
    assert!(s.effects.is_empty());
    assert_eq!(
        s.characters[&cid(0)].conditions,
        vec!["disengaged".to_owned()]
    );
}

#[test]
fn target_distance_uses_the_actions_metric() {
    let mut s = world();
    set_condition(&mut s, 1, "prone");
    s.tokens.get_mut(&tid(1)).unwrap().y = 2240;
    let action_def = s
        .plugins
        .get_mut("srd5")
        .unwrap()
        .system
        .as_mut()
        .unwrap()
        .mechanics
        .actions
        .iter_mut()
        .find(|a| a.id == "melee_attack")
        .unwrap();
    action_def.range_cells = "10".into();
    action_def.distance_metric = DistanceMetric::Euclidean;
    assert!(first_d20(&action(&mut s, "melee_attack").unwrap()).contains("dis"));
}

#[test]
fn initiative_is_modified_without_requiring_an_action() {
    let mut s = world();
    set_condition(&mut s, 0, "incapacitated");
    let id = CombatantId(v7(0x9891));
    let combat_id = CombatId(v7(0x1234));
    run(
        &mut s,
        Role::Gm,
        Intent::AddCombatant {
            id,
            combat_id,
            token_id: Some(tid(0)),
            character_id: Some(cid(0)),
            name: "Actor".into(),
        },
        None,
    )
    .unwrap();
    let version = s.combatants[&id].version;
    let events = run(
        &mut s,
        Role::Gm,
        Intent::RollInitiative { id },
        Some(version),
    )
    .unwrap();
    assert!(first_d20(&events).contains("dis"));
}

#[test]
fn target_numeric_modifier_changes_the_hit_result_and_removal_restores_it() {
    let mut s = world();
    let rules = s.plugins.get_mut("srd5").unwrap().system.as_mut().unwrap();
    let attack = rules
        .mechanics
        .actions
        .iter_mut()
        .find(|a| a.id == "melee_attack")
        .unwrap();
    attack.attack_dice = Some("17".into());
    attack.hit = Some("@total >= @ac".into());
    rules.mechanics.condition_rules.push(ConditionRule {
        id: "defense_test".into(),
        label: Label::Text("Defense +8".into()),
        condition: "poisoned".into(),
        subject: RuleSubject::Actor,
        feature: None,
        when: String::new(),
        effects: vec![ConditionModifier::Parameter {
            path: "ac".into(),
            add: 8,
            minimum: None,
            maximum: None,
        }],
    });
    set_condition(&mut s, 1, "poisoned");
    action(&mut s, "melee_attack").unwrap();
    let sheet: serde_json::Value = serde_json::from_str(&s.characters[&cid(1)].sheet_json).unwrap();
    assert_eq!(sheet["hp"]["current"], 100);
    assert_eq!(sheet["ac"], 10);
    s.characters.get_mut(&cid(1)).unwrap().conditions.clear();
    action(&mut s, "melee_attack").unwrap();
    let sheet: serde_json::Value = serde_json::from_str(&s.characters[&cid(1)].sheet_json).unwrap();
    assert_eq!(sheet["hp"]["current"], 97);
    assert_eq!(sheet["ac"], 10);
}
