#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use common::*;
use dnd_game_core::{GameState, check_plugin, default_sheet, snapshot_for_user};
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
            if matches!(
                cost.resource.as_str(),
                "action" | "bonus_action" | "reaction"
            ) {
                cost.feature = None;
            }
        }
        if a.maintain.is_none() && a.interrupt.is_none() {
            a.damage_dice = "3".into();
        }
        a.critical_if = None;
        a.critical_dice = None;
        if a.attack_dice.is_some() {
            a.attack_dice = Some("20".into());
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
fn answer(s: &mut GameState, accept: bool) -> Result<Vec<Event>, RejectReason> {
    let m = s.plugin_runtime.movements[0].clone();
    run(
        s,
        Role::Player,
        Intent::ResolvePluginReaction {
            movement: m.id,
            source: tid(0),
            accept,
        },
        None,
    )
}
fn hp(s: &GameState, n: u128) -> i64 {
    serde_json::from_str::<serde_json::Value>(&s.characters[&cid(n)].sheet_json).unwrap()["hp"]["current"].as_i64().unwrap()
}

#[test]
fn movement_pauses_and_controller_accepts_before_resume() {
    let mut s = world();
    leave(&mut s);
    assert_eq!(s.tokens[&tid(1)].x, 2240);
    assert_eq!(s.plugin_runtime.movements.len(), 1);
    answer(&mut s, true).unwrap();
    assert_eq!(hp(&s, 1), 97);
    assert_eq!(s.tokens[&tid(1)].x, 10_000);
    assert!(s.plugin_runtime.movements.is_empty());
    let values = dnd_game_core::sheet_values(&s, &s.characters[&cid(0)].sheet_json).unwrap();
    assert!(
        values
            .resources
            .iter()
            .any(|(id, v, _)| id == "reaction" && *v == 0)
    );
}
#[test]
fn decline_resumes_without_damage_or_spending() {
    let mut s = world();
    leave(&mut s);
    answer(&mut s, false).unwrap();
    assert_eq!(hp(&s, 1), 100);
    assert_eq!(s.tokens[&tid(1)].x, 10_000);
}
#[test]
fn disabled_feature_and_nonvoluntary_movement_do_not_provoke() {
    for mode in [MovementMode::Forced, MovementMode::Teleport] {
        let mut s = world();
        run(
            &mut s,
            Role::Gm,
            Intent::MoveTokenWithMode {
                id: tid(1),
                x: 10_000,
                y: 1120,
                mode,
            },
            None,
        )
        .unwrap();
        assert!(s.plugin_runtime.movements.is_empty());
    }
    let mut s = world();
    run(
        &mut s,
        Role::CoGm,
        Intent::ConfigurePluginFeature {
            id: "srd5".into(),
            feature: "opportunity_attacks".into(),
            enabled: false,
        },
        None,
    )
    .unwrap();
    leave(&mut s);
    assert!(s.plugin_runtime.movements.is_empty());
}
#[test]
fn player_cannot_change_campaign_rules_or_force_movement() {
    let mut s = world();
    assert_eq!(
        run(
            &mut s,
            Role::Player,
            Intent::ConfigurePluginFeature {
                id: "srd5".into(),
                feature: "opportunity_attacks".into(),
                enabled: false
            },
            None
        ),
        Err(RejectReason::Forbidden)
    );
    assert_eq!(
        run(
            &mut s,
            Role::Player,
            Intent::MoveTokenWithMode {
                id: tid(0),
                x: 5,
                y: 5,
                mode: MovementMode::Teleport
            },
            None
        ),
        Err(RejectReason::Forbidden)
    );
}
#[test]
fn offers_are_private_and_survive_snapshot_roundtrip() {
    let mut s = world();
    leave(&mut s);
    let mine = snapshot_for_user(Role::Player, PLAYER, &s, Seq(0), []);
    assert_eq!(mine.plugin_runtime.movements.len(), 1);
    let other = snapshot_for_user(Role::Player, OTHER_PLAYER, &s, Seq(0), []);
    assert!(
        other
            .plugin_runtime
            .movements
            .iter()
            .all(|m| m.offers.is_empty())
    );
    assert!(
        snapshot_for_user(Role::Spectator, SPECTATOR, &s, Seq(0), [])
            .plugin_runtime
            .movements
            .is_empty()
    );
    let restored = GameState::from_snapshot(s.to_snapshot(Seq(0)));
    assert_eq!(s.plugin_runtime, restored.plugin_runtime);
}
#[test]
fn area_uses_same_action_and_affects_each_character_once() {
    let mut s = world();
    run_at(
        &mut s,
        Role::Player,
        Intent::ExecutePluginAction {
            source: tid(0),
            action: "fireball".into(),
            target: None,
            x: 2240,
            y: 1120,
        },
        None,
        1000,
    )
    .unwrap();
    assert!(hp(&s, 0) < 100);
    assert!(hp(&s, 1) < 100);
    assert_eq!(s.plugin_runtime.visuals.len(), 1);
}
#[test]
fn reaction_cannot_be_replayed_or_called_as_manual_action() {
    let mut s = world();
    assert!(
        run(
            &mut s,
            Role::Player,
            Intent::ExecutePluginAction {
                source: tid(0),
                action: "opportunity_strike".into(),
                target: Some(tid(1)),
                x: 2240,
                y: 1120
            },
            None
        )
        .is_err()
    );
    leave(&mut s);
    let id = s.plugin_runtime.movements[0].id;
    answer(&mut s, true).unwrap();
    assert!(
        run(
            &mut s,
            Role::Player,
            Intent::ResolvePluginReaction {
                movement: id,
                source: tid(0),
                accept: true
            },
            None
        )
        .is_err()
    );
}
#[test]
fn malformed_hook_cannot_install() {
    let mut s = world();
    let p = s.plugins.get_mut("srd5").unwrap();
    p.system.as_mut().unwrap().mechanics.movement_rules[0].action = "missing".into();
    assert!(check_plugin(p).is_err());
}

#[test]
fn old_sheets_use_new_defaults_without_resaving() {
    let mut s = world();
    for character in s.characters.values_mut() {
        let mut sheet: serde_json::Value = serde_json::from_str(&character.sheet_json).unwrap();
        sheet.as_object_mut().unwrap().remove("automation");
        character.sheet_json = sheet.to_string();
    }
    leave(&mut s);
    assert_eq!(s.plugin_runtime.movements.len(), 1);
    answer(&mut s, true).unwrap();
    assert_eq!(hp(&s, 1), 97);
}

#[test]
fn reinstall_preserves_campaign_switches_and_rejects_unknown_feature() {
    let mut s = world();
    run(
        &mut s,
        Role::CoGm,
        Intent::ConfigurePluginFeature {
            id: "srd5".into(),
            feature: "opportunity_attacks".into(),
            enabled: false,
        },
        None,
    )
    .unwrap();
    let mut update = s.plugins["srd5"].clone();
    update
        .feature_settings
        .insert("opportunity_attacks".into(), true);
    run(
        &mut s,
        Role::Gm,
        Intent::InstallPlugin {
            plugin: Box::new(update),
        },
        None,
    )
    .unwrap();
    assert!(!s.plugins["srd5"].feature_settings["opportunity_attacks"]);
    assert!(
        run(
            &mut s,
            Role::Gm,
            Intent::ConfigurePluginFeature {
                id: "srd5".into(),
                feature: "typo".into(),
                enabled: false
            },
            None
        )
        .is_err()
    );
}

#[test]
fn plugin_selects_which_resource_refreshes_at_turn_start() {
    let mut s = world();
    s.plugins
        .get_mut("srd5")
        .unwrap()
        .system
        .as_mut()
        .unwrap()
        .mechanics
        .turn_resources = vec!["spell_slot_3".into()];
    let mut sheet: serde_json::Value =
        serde_json::from_str(&s.characters[&cid(0)].sheet_json).unwrap();
    sheet["automation"]["reaction"] = 0.into();
    sheet["automation"]["spell_slots_3"] = 0.into();
    s.characters.get_mut(&cid(0)).unwrap().sheet_json = sheet.to_string();
    let combat_id = CombatId(v7(0x1234));
    run(
        &mut s,
        Role::Gm,
        Intent::AddCombatant {
            id: CombatantId(v7(0x9911)),
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
    let values = dnd_game_core::sheet_values(&s, &s.characters[&cid(0)].sheet_json).unwrap();
    assert!(
        values
            .resources
            .iter()
            .any(|(id, v, _)| id == "spell_slot_3" && *v == 2)
    );
    assert!(
        values
            .resources
            .iter()
            .any(|(id, v, _)| id == "reaction" && *v == 0)
    );
}

#[test]
fn multiple_costs_are_atomic_and_duplicate_entries_are_summed() {
    let mut s = world();
    let plugin_id = s.system_plugin.clone().unwrap();
    let rules = s
        .plugins
        .get_mut(&plugin_id)
        .unwrap()
        .system
        .as_mut()
        .unwrap();
    let action = rules
        .mechanics
        .actions
        .iter_mut()
        .find(|a| a.id == "fireball")
        .unwrap();
    action.costs.push(ActionCost {
        scope: CostScope::Always,
        feature: None,
        on_resolution: false,
        resource: "spell_slot_3".into(),
        amount: "1".into(),
    });
    let cast = Intent::ExecutePluginAction {
        source: tid(0),
        action: "fireball".into(),
        target: None,
        x: 2240,
        y: 1120,
    };
    run(&mut s, Role::Player, cast.clone(), None).unwrap();
    let sheet: serde_json::Value = serde_json::from_str(&s.characters[&cid(0)].sheet_json).unwrap();
    assert_eq!(sheet["automation"]["actions"], 0);
    assert_eq!(sheet["automation"]["spell_slots_3"], 0);
    let before = s.to_snapshot(Seq(0));
    assert!(run(&mut s, Role::Player, cast, None).is_err());
    assert_eq!(s.to_snapshot(Seq(0)), before);
}

#[test]
fn insufficient_secondary_cost_does_not_spend_first_cost_or_damage() {
    let mut s = world();
    let c = s.characters.get_mut(&cid(0)).unwrap();
    let mut sheet: serde_json::Value = serde_json::from_str(&c.sheet_json).unwrap();
    sheet["automation"]["spell_slots_3"] = 0.into();
    c.sheet_json = sheet.to_string();
    let before = s.to_snapshot(Seq(0));
    assert!(
        run(
            &mut s,
            Role::Player,
            Intent::ExecutePluginAction {
                source: tid(0),
                action: "fireball".into(),
                target: None,
                x: 2240,
                y: 1120,
            },
            None
        )
        .is_err()
    );
    assert_eq!(s.to_snapshot(Seq(0)), before);
}

#[test]
fn invalid_cost_definitions_cannot_install() {
    let s = world();
    let plugin = &s.plugins[s.system_plugin.as_ref().unwrap()];
    for (resource, amount, legacy) in [
        ("unknown", "1", false),
        ("action", "0", false),
        ("action", "-1", false),
        ("action", "1000001", false),
        ("action", "@missing", false),
        ("action", "1", true),
    ] {
        let mut plugin = plugin.clone();
        let action = plugin
            .system
            .as_mut()
            .unwrap()
            .mechanics
            .actions
            .iter_mut()
            .find(|a| a.id == "fireball")
            .unwrap();
        action.cost = legacy.then(|| "action".into());
        action.costs = vec![ActionCost {
            scope: CostScope::Always,
            feature: None,
            on_resolution: false,
            resource: resource.into(),
            amount: amount.into(),
        }];
        assert!(
            check_plugin(&plugin).is_err(),
            "{resource} {amount} {legacy}"
        );
    }
}

#[test]
fn gm_panel_references_are_checked_on_install() {
    let s = world();
    let original = &s.plugins[s.system_plugin.as_ref().unwrap()];
    for kind in [
        GmPanelKind::Actions,
        GmPanelKind::Rules,
        GmPanelKind::Overview,
    ] {
        let mut plugin = original.clone();
        plugin.system.as_mut().unwrap().mechanics.gm_panel = vec![GmPanelSection {
            id: "bad".into(),
            label: Label::Text("Bad".into()),
            description: Label::Text(String::new()),
            kind,
            items: vec!["unknown".into()],
        }];
        assert!(check_plugin(&plugin).is_err());
    }
}

fn prepare_damage(s: &mut GameState, save_success: bool) {
    let plugin = s
        .plugins
        .get_mut(s.system_plugin.as_ref().unwrap())
        .unwrap();
    let action = plugin
        .system
        .as_mut()
        .unwrap()
        .mechanics
        .actions
        .iter_mut()
        .find(|a| a.id == "fireball")
        .unwrap();
    action.damage_dice = "11".into();
    action.save_dice = Some(if save_success { "20" } else { "1" }.into());
    action.save_dc = Some("10".into());
    action.hit = Some("@total >= @difficulty".into());
}
fn edit_sheet(s: &mut GameState, n: u128, edit: impl FnOnce(&mut serde_json::Value)) {
    let c = s.characters.get_mut(&cid(n)).unwrap();
    let mut sheet = serde_json::from_str(&c.sheet_json).unwrap();
    edit(&mut sheet);
    c.sheet_json = sheet.to_string();
}
fn cast_fire(s: &mut GameState) -> Result<Vec<Event>, RejectReason> {
    run(
        s,
        Role::Player,
        Intent::ExecutePluginAction {
            source: tid(0),
            action: "fireball".into(),
            target: None,
            x: 2240,
            y: 1120,
        },
        None,
    )
}
fn temporary_hp(s: &GameState, n: u128) -> i64 {
    serde_json::from_str::<serde_json::Value>(&s.characters[&cid(n)].sheet_json).unwrap()["hp"]["temporary"].as_i64().unwrap()
}

#[test]
fn damage_order_rounding_and_temporary_buffer_are_per_target() {
    let mut s = world();
    prepare_damage(&mut s, true);
    edit_sheet(&mut s, 0, |v| {
        v["defenses"]["immunity"]["fire"] = true.into();
        v["hp"]["temporary"] = 5.into();
    });
    edit_sheet(&mut s, 1, |v| {
        v["defenses"]["resistance"]["fire"] = true.into();
        v["defenses"]["resistance"]["all"] = true.into(); // Same resistance must not stack.
        v["defenses"]["vulnerability"]["fire"] = true.into();
        v["hp"]["temporary"] = 3.into();
    });
    cast_fire(&mut s).unwrap();
    assert_eq!(hp(&s, 0), 100);
    assert_eq!(temporary_hp(&s, 0), 5);
    // 11 -> save 5 -> resistance 2 -> vulnerability 4 -> buffer 3 -> HP 1.
    assert_eq!(hp(&s, 1), 99);
    assert_eq!(temporary_hp(&s, 1), 0);
}

#[test]
fn flat_adjustment_precedes_resistance_and_vulnerability() {
    let mut s = world();
    prepare_damage(&mut s, false);
    edit_sheet(&mut s, 1, |v| {
        v["defenses"]["reduction"] = 4.into();
        v["defenses"]["resistance"]["fire"] = true.into();
        v["defenses"]["vulnerability"]["fire"] = true.into();
    });
    cast_fire(&mut s).unwrap();
    assert_eq!(hp(&s, 1), 94); // (11 - 4) / 2 rounded down, then doubled.
    assert_eq!(hp(&s, 0), 89); // Same base roll, different target defenses.
}

#[test]
fn damage_type_filters_and_feature_switch_preserve_legacy_behavior() {
    for disabled in [false, true] {
        let mut s = world();
        prepare_damage(&mut s, false);
        edit_sheet(&mut s, 1, |v| {
            v["defenses"]["resistance"]["cold"] = true.into();
            v["hp"]["temporary"] = 5.into();
        });
        if disabled {
            s.plugins
                .get_mut(s.system_plugin.as_ref().unwrap())
                .unwrap()
                .feature_settings
                .insert("damage_rules".into(), false);
        }
        cast_fire(&mut s).unwrap();
        assert_eq!(hp(&s, 1), if disabled { 89 } else { 94 });
        assert_eq!(temporary_hp(&s, 1), if disabled { 5 } else { 0 });
    }
}

#[test]
fn buffer_absorbs_damage_without_changing_regular_hp() {
    let mut s = world();
    prepare_damage(&mut s, false);
    edit_sheet(&mut s, 1, |v| {
        v["hp"]["temporary"] = 20.into();
    });
    cast_fire(&mut s).unwrap();
    assert_eq!(hp(&s, 1), 100);
    assert_eq!(temporary_hp(&s, 1), 9);
}

#[test]
fn bad_damage_profile_is_rejected_before_installation() {
    let s = world();
    let original = &s.plugins[s.system_plugin.as_ref().unwrap()];
    for bad in 0..5 {
        let mut plugin = original.clone();
        let rules = plugin.system.as_mut().unwrap();
        let profile = &mut rules.mechanics.damage_profiles[0];
        match bad {
            0 => profile.absorb.push("missing".into()),
            1 => profile.absorb.push("temporary_hp".into()),
            2 => profile.modifiers[0].amount = "@missing".into(),
            3 => profile.modifiers[0].amount = "-1".into(),
            _ => profile.absorb.push("hp".into()),
        }
        assert!(check_plugin(&plugin).is_err(), "bad profile {bad}");
    }
}

#[test]
fn runtime_damage_error_rolls_back_all_costs_and_previous_targets() {
    let mut s = world();
    prepare_damage(&mut s, false);
    let plugin = s
        .plugins
        .get_mut(s.system_plugin.as_ref().unwrap())
        .unwrap();
    plugin.system.as_mut().unwrap().mechanics.damage_profiles[0].modifiers[0].amount =
        "@damage / @defenses.reduction".into();
    edit_sheet(&mut s, 0, |v| {
        v["defenses"]["reduction"] = 1.into();
    });
    edit_sheet(&mut s, 1, |v| {
        v["defenses"]["reduction"] = 2.into();
    });
    // An integer result is required; 11 / 2 is invalid, after actor costs were tentatively applied.
    let before = s.to_snapshot(Seq(0));
    assert!(cast_fire(&mut s).is_err());
    assert_eq!(s.to_snapshot(Seq(0)), before);
}

#[test]
fn actions_without_damage_profiles_keep_direct_resource_damage() {
    let mut s = world();
    prepare_damage(&mut s, false);
    edit_sheet(&mut s, 1, |v| {
        v["hp"]["temporary"] = 5.into();
        v["defenses"]["immunity"]["fire"] = true.into();
    });
    let plugin = s
        .plugins
        .get_mut(s.system_plugin.as_ref().unwrap())
        .unwrap();
    let action = plugin
        .system
        .as_mut()
        .unwrap()
        .mechanics
        .actions
        .iter_mut()
        .find(|a| a.id == "fireball")
        .unwrap();
    action.damage_profile = None;
    cast_fire(&mut s).unwrap();
    assert_eq!(hp(&s, 1), 89);
    assert_eq!(temporary_hp(&s, 1), 5);
}

#[test]
fn buffer_changes_for_hidden_targets_are_private() {
    let mut s = world();
    prepare_damage(&mut s, false);
    s.tokens.get_mut(&tid(1)).unwrap().visible = false;
    edit_sheet(&mut s, 1, |v| {
        v["hp"]["temporary"] = 5.into();
    });
    let events = cast_fire(&mut s).unwrap();
    // Resource announcements for hidden entities must never become public chat.
    for event in events {
        if let Event::ChatMessagePosted { message } = event {
            let text = serde_json::to_string(&message).unwrap();
            if text.contains("temporary_hp") {
                assert_eq!(message.visibility, ChatVisibility::Gm);
            }
        }
    }
    assert_eq!(hp(&s, 1), 94);
}

fn maintained_world() -> GameState {
    let mut s = world();
    edit_sheet(&mut s, 0, |v| {
        v["automation"]["spell_slots_1"] = 4.into();
        v["automation"]["spell_slots_1_max"] = 4.into();
    });
    s
}
fn shield(s: &mut GameState, target: u128, seed: u64) -> Result<Vec<Event>, RejectReason> {
    let token = &s.tokens[&tid(target)];
    let envelope = envelope(
        Intent::ExecutePluginAction {
            source: tid(0),
            action: "shield_of_faith".into(),
            target: Some(token.id),
            x: token.x,
            y: token.y,
        },
        None,
    );
    let events = dnd_game_core::apply(
        s,
        &envelope,
        &actor(Role::Player),
        &mut dnd_game_core::SplitMix64::new(seed),
    )?;
    dnd_game_core::evolve_all(s, &events);
    Ok(events)
}
fn ac(s: &GameState, n: u128) -> i64 {
    use dnd_dice::FieldLookup;
    dnd_game_core::effective_fields(s, &s.characters[&cid(n)])
        .unwrap()
        .lookup("ac")
        .unwrap()
}
fn maintenance_dice(s: &mut GameState, dice: &str) {
    s.plugins
        .get_mut(s.system_plugin.as_ref().unwrap())
        .unwrap()
        .system
        .as_mut()
        .unwrap()
        .mechanics
        .maintained_rules[0]
        .damage_check
        .dice = dice.into();
}
#[test]
fn maintained_effect_modifies_target_and_can_be_ended_by_controller_or_gm() {
    for role in [Role::Player, Role::Gm, Role::CoGm] {
        let mut s = maintained_world();
        let events = shield(&mut s, 1, 1).unwrap();
        assert!(!events.iter().any(
            |e| matches!(e, Event::ChatMessagePosted { message } if message.kind == ChatKind::Roll)
        ));
        assert_eq!(s.plugin_runtime.maintained.len(), 1);
        assert_eq!(ac(&s, 1), 12);
        assert_eq!(ac(&s, 0), 10);
        let raw: serde_json::Value =
            serde_json::from_str(&s.characters[&cid(1)].sheet_json).unwrap();
        assert_eq!(raw["ac"], 10);
        let id = s.plugin_runtime.maintained[0].id;
        run(&mut s, role, Intent::EndMaintainedEffect { id }, None).unwrap();
        assert!(s.plugin_runtime.maintained.is_empty());
        assert_eq!(ac(&s, 1), 10);
        assert!(run(&mut s, role, Intent::EndMaintainedEffect { id }, None).is_err());
    }
}
#[test]
fn other_players_and_spectators_cannot_end_anothers_maintenance() {
    let mut s = maintained_world();
    shield(&mut s, 1, 1).unwrap();
    let id = s.plugin_runtime.maintained[0].id;
    let before = s.to_snapshot(Seq(0));
    let stranger = dnd_game_core::Actor {
        role: Role::Player,
        user_id: OTHER_PLAYER,
    };
    assert!(
        dnd_game_core::apply(
            &s,
            &envelope(Intent::EndMaintainedEffect { id }, None),
            &stranger,
            &mut dnd_game_core::SplitMix64::new(0)
        )
        .is_err()
    );
    assert!(
        run(
            &mut s,
            Role::Spectator,
            Intent::EndMaintainedEffect { id },
            None
        )
        .is_err()
    );
    assert_eq!(s.to_snapshot(Seq(0)), before);
}
#[test]
fn exclusive_group_replaces_prior_effect_without_stacking() {
    let mut s = maintained_world();
    shield(&mut s, 1, 1).unwrap();
    let old = s.plugin_runtime.maintained[0].id;
    edit_sheet(&mut s, 0, |v| {
        v["automation"]["bonus_actions"] = 1.into();
    });
    shield(&mut s, 0, 2).unwrap();
    assert_eq!(s.plugin_runtime.maintained.len(), 1);
    assert_ne!(old, s.plugin_runtime.maintained[0].id);
    assert_eq!(ac(&s, 1), 10);
    assert_eq!(ac(&s, 0), 12);
    assert!(
        run(
            &mut s,
            Role::Player,
            Intent::EndMaintainedEffect { id: old },
            None
        )
        .is_err()
    );
}
#[test]
fn damage_absorbed_by_temporary_hp_still_checks_maintenance() {
    for (dice, survives) in [("20", true), ("1", false)] {
        let mut s = maintained_world();
        prepare_damage(&mut s, false);
        maintenance_dice(&mut s, dice);
        shield(&mut s, 1, 1).unwrap();
        edit_sheet(&mut s, 0, |v| {
            v["hp"]["temporary"] = 100.into();
        });
        let events = cast_fire(&mut s).unwrap();
        assert_eq!(hp(&s, 0), 100);
        assert_eq!(temporary_hp(&s, 0), 89);
        assert_eq!(!s.plugin_runtime.maintained.is_empty(), survives);
        assert_eq!(ac(&s, 1), if survives { 12 } else { 10 });
        assert_eq!(events.iter().filter(|e| matches!(e, Event::ChatMessagePosted { message } if message.body == "Concentration save")).count(), 1);
    }
}
#[test]
fn immunity_prevents_a_maintenance_check() {
    let mut s = maintained_world();
    prepare_damage(&mut s, false);
    maintenance_dice(&mut s, "1");
    shield(&mut s, 1, 1).unwrap();
    edit_sheet(&mut s, 0, |v| {
        v["defenses"]["immunity"]["fire"] = true.into();
    });
    let events = cast_fire(&mut s).unwrap();
    assert_eq!(s.plugin_runtime.maintained.len(), 1);
    assert!(!events.iter().any(|e| matches!(e, Event::ChatMessagePosted { message } if message.body == "Concentration save")));
}
#[test]
fn concentration_difficulty_rounds_down_and_is_capped_by_plugin() {
    for (damage, survives) in [(59, true), (60, false), (100, false)] {
        let mut s = maintained_world();
        prepare_damage(&mut s, false);
        maintenance_dice(&mut s, "29");
        s.plugins
            .get_mut(s.system_plugin.as_ref().unwrap())
            .unwrap()
            .system
            .as_mut()
            .unwrap()
            .mechanics
            .actions
            .iter_mut()
            .find(|a| a.id == "fireball")
            .unwrap()
            .damage_dice = damage.to_string();
        shield(&mut s, 1, 1).unwrap();
        edit_sheet(&mut s, 0, |v| {
            v["hp"]["temporary"] = 1000.into();
        });
        cast_fire(&mut s).unwrap();
        assert_eq!(
            !s.plugin_runtime.maintained.is_empty(),
            survives,
            "damage {damage}"
        );
    }
    let mut s = maintained_world();
    prepare_damage(&mut s, false);
    maintenance_dice(&mut s, "30");
    s.plugins
        .get_mut(s.system_plugin.as_ref().unwrap())
        .unwrap()
        .system
        .as_mut()
        .unwrap()
        .mechanics
        .actions
        .iter_mut()
        .find(|a| a.id == "fireball")
        .unwrap()
        .damage_dice = "100".into();
    shield(&mut s, 1, 1).unwrap();
    edit_sheet(&mut s, 0, |v| {
        v["hp"]["temporary"] = 1000.into();
    });
    cast_fire(&mut s).unwrap();
    assert_eq!(s.plugin_runtime.maintained.len(), 1);
}
#[test]
fn incapacity_feature_disable_and_expiry_remove_linked_bonuses() {
    for mode in 0..3 {
        let mut s = maintained_world();
        if mode == 2 {
            s.plugins
                .get_mut(s.system_plugin.as_ref().unwrap())
                .unwrap()
                .system
                .as_mut()
                .unwrap()
                .mechanics
                .actions
                .iter_mut()
                .find(|a| a.id == "shield_of_faith")
                .unwrap()
                .maintain
                .as_mut()
                .unwrap()
                .duration = EffectDurationDef::RealTime { minutes: 1 };
        }
        shield(&mut s, 1, 1).unwrap();
        match mode {
            0 => {
                let version = s.characters[&cid(0)].version;
                run(
                    &mut s,
                    Role::Gm,
                    Intent::SetConditions {
                        id: cid(0),
                        conditions: vec!["incapacitated".into()],
                    },
                    Some(version),
                )
                .unwrap();
            }
            1 => {
                let id = s.system_plugin.clone().unwrap();
                run(
                    &mut s,
                    Role::CoGm,
                    Intent::ConfigurePluginFeature {
                        id,
                        feature: "concentration".into(),
                        enabled: false,
                    },
                    None,
                )
                .unwrap();
            }
            _ => {
                run_at(
                    &mut s,
                    Role::Gm,
                    Intent::MoveToken {
                        id: tid(0),
                        x: 1120,
                        y: 1120,
                    },
                    None,
                    60001,
                )
                .unwrap();
            }
        }
        assert!(s.plugin_runtime.maintained.is_empty());
        assert_eq!(ac(&s, 1), 10);
    }
}
#[test]
fn removing_the_source_or_last_child_cleans_maintenance() {
    for source in [true, false] {
        let mut s = maintained_world();
        shield(&mut s, 1, 1).unwrap();
        if source {
            let version = s.tokens[&tid(0)].version;
            run(
                &mut s,
                Role::Gm,
                Intent::DeleteToken { id: tid(0) },
                Some(version),
            )
            .unwrap();
        } else {
            let id = s.plugin_runtime.maintained[0].effects[0];
            let version = s.effects[&id].version;
            run(
                &mut s,
                Role::Gm,
                Intent::RemoveTokenEffect { id },
                Some(version),
            )
            .unwrap();
        }
        assert!(s.plugin_runtime.maintained.is_empty());
        assert_eq!(ac(&s, 1), 10);
    }
}
#[test]
fn maintenance_snapshot_is_owned_private_and_roundtrips() {
    let mut s = maintained_world();
    shield(&mut s, 1, 1).unwrap();
    assert_eq!(
        GameState::from_snapshot(s.to_snapshot(Seq(0))).plugin_runtime,
        s.plugin_runtime
    );
    assert_eq!(
        snapshot_for_user(Role::Player, PLAYER, &s, Seq(0), [])
            .plugin_runtime
            .maintained
            .len(),
        1
    );
    assert!(
        snapshot_for_user(Role::Player, OTHER_PLAYER, &s, Seq(0), [])
            .plugin_runtime
            .maintained
            .is_empty()
    );
    assert!(
        snapshot_for_user(Role::Spectator, SPECTATOR, &s, Seq(0), [])
            .plugin_runtime
            .maintained
            .is_empty()
    );
}
#[test]
fn unknown_maintenance_rules_and_formulas_cannot_install() {
    let s = maintained_world();
    for mode in 0..4 {
        let mut plugin = s.plugins[s.system_plugin.as_ref().unwrap()].clone();
        let rules = plugin.system.as_mut().unwrap();
        match mode {
            0 => rules.mechanics.maintained_rules[0].damage_check.difficulty = "@unknown".into(),
            1 => rules.mechanics.maintained_rules[0].keep_condition = "@condition.missing".into(),
            2 => {
                rules
                    .mechanics
                    .actions
                    .iter_mut()
                    .find(|a| a.id == "shield_of_faith")
                    .unwrap()
                    .maintain
                    .as_mut()
                    .unwrap()
                    .rule = "missing".into()
            }
            _ => rules.mechanics.maintained_rules[0].damage_check.resource = "missing".into(),
        }
        assert!(check_plugin(&plugin).is_err());
    }
}

#[test]
fn failed_replacement_keeps_the_old_effect_and_all_resources() {
    let mut s = maintained_world();
    shield(&mut s, 1, 1).unwrap();
    // No bonus action remains; validation must not end the existing effect.
    let before = s.to_snapshot(Seq(0));
    assert!(shield(&mut s, 0, 2).is_err());
    assert_eq!(s.to_snapshot(Seq(0)), before);
    assert_eq!(ac(&s, 1), 12);
}
#[test]
fn damage_checks_on_a_hidden_maintainer_stay_private() {
    let mut s = maintained_world();
    prepare_damage(&mut s, false);
    maintenance_dice(&mut s, "20");
    shield(&mut s, 1, 1).unwrap();
    s.tokens.get_mut(&tid(0)).unwrap().visible = false;
    let events = run(
        &mut s,
        Role::Gm,
        Intent::ExecutePluginAction {
            source: tid(1),
            action: "fireball".into(),
            target: None,
            x: 1120,
            y: 1120,
        },
        None,
    )
    .unwrap();
    let checks: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::ChatMessagePosted { message } if message.body == "Concentration save" => {
                Some(message)
            }
            _ => None,
        })
        .collect();
    assert_eq!(checks.len(), 1);
    assert_eq!(checks[0].visibility, ChatVisibility::Gm);
}

#[test]
fn independent_casters_share_a_condition_without_stacking_its_bonus() {
    let mut s = maintained_world();
    shield(&mut s, 1, 1).unwrap();
    let first = s.plugin_runtime.maintained[0].id;
    edit_sheet(&mut s, 1, |v| {
        v["automation"]["spell_slots_1"] = 1.into();
        v["automation"]["spell_slots_1_max"] = 1.into();
    });
    run(
        &mut s,
        Role::Gm,
        Intent::ExecutePluginAction {
            source: tid(1),
            action: "shield_of_faith".into(),
            target: Some(tid(1)),
            x: 2240,
            y: 1120,
        },
        None,
    )
    .unwrap();
    assert_eq!(s.plugin_runtime.maintained.len(), 2);
    assert_eq!(ac(&s, 1), 12);
    run(
        &mut s,
        Role::Player,
        Intent::EndMaintainedEffect { id: first },
        None,
    )
    .unwrap();
    assert_eq!(s.plugin_runtime.maintained.len(), 1);
    assert_eq!(ac(&s, 1), 12);
}
#[test]
fn a_different_primary_resource_does_not_count_as_health_damage() {
    let mut s = maintained_world();
    prepare_damage(&mut s, false);
    maintenance_dice(&mut s, "1");
    shield(&mut s, 1, 1).unwrap();
    let action = s
        .plugins
        .get_mut(s.system_plugin.as_ref().unwrap())
        .unwrap()
        .system
        .as_mut()
        .unwrap()
        .mechanics
        .actions
        .iter_mut()
        .find(|a| a.id == "fireball")
        .unwrap();
    action.resource = "reaction".into();
    action.damage_profile = None;
    let events = cast_fire(&mut s).unwrap();
    assert_eq!(s.plugin_runtime.maintained.len(), 1);
    assert_eq!(hp(&s, 0), 100);
    assert!(!events.iter().any(|e| matches!(e, Event::ChatMessagePosted { message } if message.body == "Concentration save")));
}

fn interruption_world(save: &str) -> GameState {
    let mut s = world();
    edit_sheet(&mut s, 1, |v| {
        v["automation"]["can_counterspell"] = true.into();
    });
    let plugin = s
        .plugins
        .get_mut(s.system_plugin.as_ref().unwrap())
        .unwrap();
    plugin
        .system
        .as_mut()
        .unwrap()
        .mechanics
        .actions
        .iter_mut()
        .find(|a| a.id == "counterspell")
        .unwrap()
        .interrupt
        .as_mut()
        .unwrap()
        .save_dice = save.into();
    s
}
fn slots3(s: &GameState, n: u128) -> i64 {
    serde_json::from_str::<serde_json::Value>(&s.characters[&cid(n)].sheet_json).unwrap()["automation"]["spell_slots_3"].as_i64().unwrap()
}
fn answer_cast(s: &mut GameState, accept: bool, seed: u64) -> Result<Vec<Event>, RejectReason> {
    let stack = &s.plugin_runtime.action_stacks[0];
    let frame = stack.frames.last().unwrap();
    let intent = Intent::ResolveActionReaction {
        stack: stack.id,
        frame: frame.id,
        source: frame.offers[0].source,
        accept,
    };
    let events = dnd_game_core::apply(
        s,
        &envelope(intent, None),
        &actor(Role::Gm),
        &mut dnd_game_core::SplitMix64::new(seed),
    )?;
    dnd_game_core::evolve_all(s, &events);
    Ok(events)
}
#[test]
fn declaring_a_spell_spends_action_and_reserves_slot_without_damage() {
    let mut s = interruption_world("1");
    let events = cast_fire(&mut s).unwrap();
    assert_eq!(hp(&s, 0), 100);
    assert_eq!(hp(&s, 1), 100);
    assert_eq!(slots3(&s, 0), 2);
    let sheet: serde_json::Value = serde_json::from_str(&s.characters[&cid(0)].sheet_json).unwrap();
    assert_eq!(sheet["automation"]["actions"], 0);
    let frame = &s.plugin_runtime.action_stacks[0].frames[0];
    assert_eq!(
        frame.deferred,
        vec![DeferredCost {
            resource: "spell_slot_3".into(),
            amount: 1
        }]
    );
    assert_eq!(frame.offers.len(), 1);
    assert_eq!(frame.offers[0].source, tid(1));
    assert!(frame.awaiting_reactions);
    assert!(!events.iter().any(
        |e| matches!(e, Event::ChatMessagePosted { message } if message.kind == ChatKind::Roll)
    ));
}
#[test]
fn successful_counterspell_keeps_targets_slot_but_spends_both_activations() {
    let mut s = interruption_world("1");
    cast_fire(&mut s).unwrap();
    let events = answer_cast(&mut s, true, 100).unwrap();
    assert!(s.plugin_runtime.action_stacks.is_empty());
    assert_eq!(slots3(&s, 0), 2);
    assert_eq!(slots3(&s, 1), 1);
    assert_eq!(hp(&s, 0), 100);
    assert_eq!(hp(&s, 1), 100);
    let sheet: serde_json::Value = serde_json::from_str(&s.characters[&cid(1)].sheet_json).unwrap();
    assert_eq!(sheet["automation"]["reaction"], 0);
    assert!(events.iter().any(|e| matches!(e, Event::ChatMessagePosted { message } if matches!(&message.system, Some(SystemMessage::PluginActionResolved { outcome: ActionResolution::Countered, .. })))));
}
#[test]
fn declined_or_failed_counterspell_resumes_the_original_action_once() {
    for accept in [true, false] {
        let mut s = interruption_world("20");
        cast_fire(&mut s).unwrap();
        let stack = s.plugin_runtime.action_stacks[0].clone();
        answer_cast(&mut s, accept, 100).unwrap();
        assert!(s.plugin_runtime.action_stacks.is_empty());
        assert_eq!(slots3(&s, 0), 1);
        assert_eq!(slots3(&s, 1), if accept { 1 } else { 2 });
        assert!(hp(&s, 0) < 100);
        assert!(hp(&s, 1) < 100);
        let before = s.to_snapshot(Seq(0));
        assert!(
            run(
                &mut s,
                Role::Gm,
                Intent::ResolveActionReaction {
                    stack: stack.id,
                    frame: stack.frames[0].id,
                    source: tid(1),
                    accept
                },
                None
            )
            .is_err()
        );
        assert_eq!(s.to_snapshot(Seq(0)), before);
    }
}
#[test]
fn counterspell_itself_can_be_countered_lifo() {
    let mut s = interruption_world("1");
    edit_sheet(&mut s, 0, |v| {
        v["automation"]["can_counterspell"] = true.into();
    });
    cast_fire(&mut s).unwrap();
    answer_cast(&mut s, true, 100).unwrap();
    assert_eq!(s.plugin_runtime.action_stacks[0].frames.len(), 2);
    assert!(!s.plugin_runtime.action_stacks[0].frames[0].ready);
    assert_eq!(
        s.plugin_runtime.action_stacks[0].frames[1].offers[0].source,
        tid(0)
    );
    let events = answer_cast(&mut s, true, 200).unwrap();
    assert!(s.plugin_runtime.action_stacks.is_empty());
    assert!(hp(&s, 1) < 100);
    assert_eq!(slots3(&s, 0), 0); // Original spell + counter-counterspell.
    assert_eq!(slots3(&s, 1), 2); // Countered reaction retained its slot.
    assert_eq!(events.iter().filter(|e| matches!(e, Event::ChatMessagePosted { message } if message.body == "Counterspell")).count(), 1);
}
#[test]
fn reservations_prevent_spending_the_same_slot_on_a_nested_reaction() {
    let mut s = interruption_world("1");
    edit_sheet(&mut s, 0, |v| {
        v["automation"]["can_counterspell"] = true.into();
        v["automation"]["spell_slots_3"] = 1.into();
    });
    cast_fire(&mut s).unwrap();
    answer_cast(&mut s, true, 100).unwrap();
    assert!(s.plugin_runtime.action_stacks.is_empty());
    assert_eq!(hp(&s, 1), 100);
    assert_eq!(slots3(&s, 0), 1);
}
#[test]
fn pending_windows_lock_movement_and_turns_and_require_the_offered_controller() {
    let mut s = interruption_world("1");
    cast_fire(&mut s).unwrap();
    let stack = s.plugin_runtime.action_stacks[0].clone();
    let before = s.to_snapshot(Seq(0));
    for intent in [
        Intent::MoveToken {
            id: tid(0),
            x: 2000,
            y: 1120,
        },
        Intent::NextTurn {
            combat_id: *s.combats.keys().next().unwrap(),
        },
        Intent::ContinuePluginAction {
            stack: stack.id,
            frame: stack.frames[0].id,
        },
        Intent::ResolveActionReaction {
            stack: stack.id,
            frame: stack.frames[0].id,
            source: tid(1),
            accept: true,
        },
    ] {
        assert!(run(&mut s, Role::Player, intent, None).is_err());
    }
    assert_eq!(s.to_snapshot(Seq(0)), before);
}
#[test]
fn action_window_snapshots_do_not_leak_targets_or_other_owners_reservations() {
    let mut s = interruption_world("1");
    cast_fire(&mut s).unwrap();
    let own = snapshot_for_user(Role::Player, PLAYER, &s, Seq(0), []);
    let caster = &own.plugin_runtime.action_stacks[0].frames[0];
    assert!(caster.offers.is_empty());
    assert!(caster.awaiting_reactions);
    assert_eq!(caster.deferred.len(), 1);
    let reactor = snapshot_for_user(Role::Player, OTHER_PLAYER, &s, Seq(0), []);
    let frame = &reactor.plugin_runtime.action_stacks[0].frames[0];
    assert_eq!(frame.offers.len(), 1);
    assert!(frame.deferred.is_empty());
    assert_eq!((frame.x, frame.y), (0, 0));
    assert_eq!(frame.target, None);
    assert!(
        snapshot_for_user(Role::Spectator, SPECTATOR, &s, Seq(0), [])
            .plugin_runtime
            .action_stacks
            .is_empty()
    );
    assert_eq!(
        GameState::from_snapshot(s.to_snapshot(Seq(0))).plugin_runtime,
        s.plugin_runtime
    );
}
#[test]
fn gm_cancellation_releases_reservations_and_ineligible_offers_can_be_skipped() {
    let mut s = interruption_world("1");
    cast_fire(&mut s).unwrap();
    let stack = s.plugin_runtime.action_stacks[0].clone();
    run(
        &mut s,
        Role::CoGm,
        Intent::CancelActionStack { stack: stack.id },
        None,
    )
    .unwrap();
    assert!(s.plugin_runtime.action_stacks.is_empty());
    assert_eq!(slots3(&s, 0), 2);
    let mut s = interruption_world("1");
    cast_fire(&mut s).unwrap();
    let version = s.characters[&cid(1)].version;
    run(
        &mut s,
        Role::Gm,
        Intent::SetConditions {
            id: cid(1),
            conditions: vec!["incapacitated".into()],
        },
        Some(version),
    )
    .unwrap();
    let stack = s.plugin_runtime.action_stacks[0].clone();
    assert!(stack.frames[0].offers.is_empty());
    assert!(!stack.frames[0].awaiting_reactions);
    run(
        &mut s,
        Role::Player,
        Intent::ContinuePluginAction {
            stack: stack.id,
            frame: stack.frames[0].id,
        },
        None,
    )
    .unwrap();
    assert!(s.plugin_runtime.action_stacks.is_empty());
    assert!(hp(&s, 1) < 100);
}

#[test]
fn interruption_eligibility_respects_components_range_availability_and_visibility() {
    for mode in 0..6 {
        let mut s = interruption_world("1");
        match mode {
            0 => {
                s.plugins
                    .get_mut(s.system_plugin.as_ref().unwrap())
                    .unwrap()
                    .system
                    .as_mut()
                    .unwrap()
                    .mechanics
                    .actions
                    .iter_mut()
                    .find(|a| a.id == "fireball")
                    .unwrap()
                    .tags = vec!["spell".into()];
            }
            1 => {
                s.tokens.get_mut(&tid(1)).unwrap().x = 60000;
            }
            2 => edit_sheet(&mut s, 1, |v| {
                v["automation"]["reaction"] = 0.into();
            }),
            3 => edit_sheet(&mut s, 1, |v| {
                v["automation"]["can_see"] = false.into();
            }),
            4 => {
                s.plugins
                    .get_mut(s.system_plugin.as_ref().unwrap())
                    .unwrap()
                    .feature_settings
                    .insert("counterspell".into(), false);
            }
            _ => {
                s.characters.get_mut(&cid(1)).unwrap().conditions = vec!["incapacitated".into()];
            }
        }
        cast_fire(&mut s).unwrap();
        assert!(s.plugin_runtime.action_stacks.is_empty(), "mode {mode}");
        assert_eq!(slots3(&s, 0), 1);
    }
}
#[test]
fn starting_a_new_maintained_cast_ends_the_old_one_even_if_countered() {
    let mut s = maintained_world();
    shield(&mut s, 1, 1).unwrap();
    edit_sheet(&mut s, 0, |v| {
        v["automation"]["bonus_actions"] = 1.into();
    });
    edit_sheet(&mut s, 1, |v| {
        v["automation"]["can_counterspell"] = true.into();
    });
    s.plugins
        .get_mut(s.system_plugin.as_ref().unwrap())
        .unwrap()
        .system
        .as_mut()
        .unwrap()
        .mechanics
        .actions
        .iter_mut()
        .find(|a| a.id == "counterspell")
        .unwrap()
        .interrupt
        .as_mut()
        .unwrap()
        .save_dice = "1".into();
    shield(&mut s, 0, 2).unwrap();
    assert!(s.plugin_runtime.maintained.is_empty());
    assert_eq!(ac(&s, 1), 10);
    let raw: serde_json::Value = serde_json::from_str(&s.characters[&cid(0)].sheet_json).unwrap();
    assert_eq!(raw["automation"]["spell_slots_1"], 3);
    let mut restored = GameState::from_snapshot(s.to_snapshot(Seq(0)));
    answer_cast(&mut restored, true, 100).unwrap();
    assert!(restored.plugin_runtime.action_stacks.is_empty());
    assert!(restored.plugin_runtime.maintained.is_empty());
    let raw: serde_json::Value =
        serde_json::from_str(&restored.characters[&cid(0)].sheet_json).unwrap();
    assert_eq!(raw["automation"]["spell_slots_1"], 3);
    assert_eq!(ac(&restored, 0), 10);
}
#[test]
fn changing_campaign_rules_cancels_the_pending_chain_without_charging_slots() {
    let mut s = interruption_world("1");
    cast_fire(&mut s).unwrap();
    let plugin = s.system_plugin.clone().unwrap();
    run(
        &mut s,
        Role::Gm,
        Intent::ConfigurePluginFeature {
            id: plugin,
            feature: "counterspell".into(),
            enabled: false,
        },
        None,
    )
    .unwrap();
    assert!(s.plugin_runtime.action_stacks.is_empty());
    assert_eq!(slots3(&s, 0), 2);
    assert_eq!(hp(&s, 1), 100);
}
#[test]
fn invalid_interruption_references_and_difficulty_cannot_install() {
    let s = interruption_world("1");
    for mode in 0..4 {
        let mut p = s.plugins[s.system_plugin.as_ref().unwrap()].clone();
        let r = p.system.as_mut().unwrap();
        match mode {
            0 => r.mechanics.interruption_rules[0].action = "missing".into(),
            1 => r.mechanics.interruption_rules[0].range_cells = "@missing".into(),
            2 => {
                r.mechanics
                    .actions
                    .iter_mut()
                    .find(|a| a.id == "counterspell")
                    .unwrap()
                    .interrupt
                    .as_mut()
                    .unwrap()
                    .save_dc = "-1".into()
            }
            _ => {
                r.mechanics
                    .actions
                    .iter_mut()
                    .find(|a| a.id == "counterspell")
                    .unwrap()
                    .reaction_only = false
            }
        }
        assert!(check_plugin(&p).is_err(), "mode {mode}");
    }
}

fn economy_world() -> GameState {
    let mut s = world();
    s.plugins
        .get_mut(s.system_plugin.as_ref().unwrap())
        .unwrap()
        .feature_settings
        .insert("turn_economy".into(), true);
    for action in &mut s
        .plugins
        .get_mut(s.system_plugin.as_ref().unwrap())
        .unwrap()
        .system
        .as_mut()
        .unwrap()
        .mechanics
        .actions
    {
        for cost in &mut action.costs {
            if matches!(
                cost.resource.as_str(),
                "action" | "bonus_action" | "reaction"
            ) {
                cost.feature = Some("turn_economy".into());
            }
        }
    }
    let combat = *s.combats.keys().next().unwrap();
    for n in 0..2 {
        let id = CombatantId(v7(0xE000 + n));
        s.combatants.insert(
            id,
            CombatantDto {
                id,
                combat_id: combat,
                token_id: Some(tid(n)),
                character_id: Some(cid(n)),
                name: format!("Actor {n}"),
                initiative: Some(20 - i64::try_from(n).unwrap()),
                sort_order: i64::try_from(n).unwrap(),
                version: Version::INITIAL,
            },
        );
    }
    s.combats.get_mut(&combat).unwrap().turn = Some(CombatantId(v7(0xE000)));
    s
}
fn auto_value(s: &GameState, n: u128, key: &str) -> i64 {
    serde_json::from_str::<serde_json::Value>(&s.characters[&cid(n)].sheet_json).unwrap()["automation"][key].as_i64().unwrap()
}
fn next_economy_turn(s: &mut GameState) {
    let combat_id = *s.combats.keys().next().unwrap();
    let version = s.combats[&combat_id].version;
    run(s, Role::Gm, Intent::NextTurn { combat_id }, Some(version)).unwrap();
}
#[test]
fn timing_and_slot_expenditure_budget_are_declared_by_the_plugin() {
    let mut s = economy_world();
    edit_sheet(&mut s, 0, |v| {
        v["automation"]["spell_slots_1"] = 2.into();
        v["automation"]["spell_slots_1_max"] = 2.into();
    });
    let before = s.to_snapshot(Seq(0));
    assert!(
        run(
            &mut s,
            Role::Gm,
            Intent::ExecutePluginAction {
                source: tid(1),
                action: "fireball".into(),
                target: None,
                x: 2240,
                y: 1120
            },
            None
        )
        .is_err()
    );
    assert_eq!(s.to_snapshot(Seq(0)), before);
    cast_fire(&mut s).unwrap();
    assert_eq!(auto_value(&s, 0, "slot_casts_remaining"), 0);
    assert_eq!(auto_value(&s, 0, "bonus_actions"), 1);
    assert!(shield(&mut s, 0, 4).is_err()); // A bonus action exists, but no second slot expenditure.
    assert_eq!(auto_value(&s, 0, "spell_slots_1"), 2);
    next_economy_turn(&mut s);
    assert_eq!(auto_value(&s, 0, "slot_casts_remaining"), 1);
    assert_eq!(auto_value(&s, 0, "actions"), 0); // Its own action has not refreshed.
}
#[test]
fn refresh_preserves_all_resources_and_distinguishes_own_from_every_turn() {
    let mut s = economy_world();
    edit_sheet(&mut s, 0, |v| {
        v["automation"]["actions_per_turn"] = 2.into();
        for key in [
            "actions",
            "bonus_actions",
            "reaction",
            "slot_casts_remaining",
        ] {
            v["automation"][key] = 0.into();
        }
    });
    next_economy_turn(&mut s);
    assert_eq!(auto_value(&s, 0, "slot_casts_remaining"), 1);
    for key in ["actions", "bonus_actions", "reaction"] {
        assert_eq!(auto_value(&s, 0, key), 0);
    }
    next_economy_turn(&mut s);
    assert_eq!(auto_value(&s, 0, "actions"), 2);
    assert_eq!(auto_value(&s, 0, "bonus_actions"), 1);
    assert_eq!(auto_value(&s, 0, "reaction"), 1);
}
#[test]
fn budget_policies_work_on_a_single_combatants_round_wrap() {
    let mut s = economy_world();
    s.combatants.retain(|_, c| c.character_id == Some(cid(0)));
    edit_sheet(&mut s, 0, |v| {
        v["automation"]["slot_casts_remaining"] = 0.into();
        v["automation"]["reaction"] = 0.into();
    });
    next_economy_turn(&mut s);
    assert_eq!(s.combats.values().next().unwrap().round, 2);
    assert_eq!(auto_value(&s, 0, "slot_casts_remaining"), 1);
    assert_eq!(auto_value(&s, 0, "reaction"), 1);
}
#[test]
fn resources_allow_extra_actions_but_never_an_extra_slot_expenditure() {
    let mut s = economy_world();
    edit_sheet(&mut s, 0, |v| {
        v["automation"]["actions_per_turn"] = 2.into();
        v["automation"]["actions"] = 2.into();
    });
    let id = s.system_plugin.clone().unwrap();
    let mut free = s.plugins[&id]
        .system
        .as_ref()
        .unwrap()
        .mechanics
        .actions
        .iter()
        .find(|a| a.id == "fireball")
        .unwrap()
        .clone();
    free.id = "free_spell".into();
    free.costs.retain(|c| c.resource == "action");
    s.plugins
        .get_mut(&id)
        .unwrap()
        .system
        .as_mut()
        .unwrap()
        .mechanics
        .actions
        .push(free);
    cast_fire(&mut s).unwrap();
    run(
        &mut s,
        Role::Player,
        Intent::ExecutePluginAction {
            source: tid(0),
            action: "free_spell".into(),
            target: None,
            x: 2240,
            y: 1120,
        },
        None,
    )
    .unwrap();
    assert_eq!(slots3(&s, 0), 1);
    assert_eq!(auto_value(&s, 0, "actions"), 0);
    assert_eq!(auto_value(&s, 0, "slot_casts_remaining"), 0);
}
#[test]
fn combat_scoped_costs_do_not_block_outside_combat_actions() {
    let mut s = economy_world();
    s.combats.clear();
    s.combatants.clear();
    cast_fire(&mut s).unwrap();
    cast_fire(&mut s).unwrap();
    assert_eq!(slots3(&s, 0), 0);
    assert_eq!(auto_value(&s, 0, "actions"), 1);
    assert_eq!(auto_value(&s, 0, "slot_casts_remaining"), 1);
}
#[test]
fn reserved_slot_expenditure_prevents_self_countering_on_the_same_turn() {
    let mut s = economy_world();
    for n in 0..2 {
        edit_sheet(&mut s, n, |v| {
            v["automation"]["can_counterspell"] = true.into();
        });
    }
    s.plugins
        .get_mut(s.system_plugin.as_ref().unwrap())
        .unwrap()
        .system
        .as_mut()
        .unwrap()
        .mechanics
        .actions
        .iter_mut()
        .find(|a| a.id == "counterspell")
        .unwrap()
        .interrupt
        .as_mut()
        .unwrap()
        .save_dice = "1".into();
    cast_fire(&mut s).unwrap();
    let view = dnd_game_core::turn_economy_view(&s, &s.characters[&cid(0)]).unwrap();
    let budget = view
        .budgets
        .iter()
        .find(|b| b.resource == "spell_cast")
        .unwrap();
    assert_eq!((budget.remaining, budget.reserved), (0, 1));
    answer_cast(&mut s, true, 100).unwrap();
    assert!(s.plugin_runtime.action_stacks.is_empty());
    assert_eq!(slots3(&s, 0), 2);
    assert_eq!(auto_value(&s, 0, "slot_casts_remaining"), 1); // Countered spell did not expend a slot.
    assert_eq!(auto_value(&s, 1, "slot_casts_remaining"), 0);
}
#[test]
fn invalid_budget_targets_and_amounts_cannot_install() {
    let s = economy_world();
    for mode in 0..4 {
        let mut p = s.plugins[s.system_plugin.as_ref().unwrap()].clone();
        let b = &mut p.system.as_mut().unwrap().mechanics.turn_budgets[0];
        match mode {
            0 => b.resource = "missing".into(),
            1 => b.amount = Some("-1".into()),
            2 => b.amount = Some("1000".into()),
            _ => b.feature = Some("missing".into()),
        }
        assert!(check_plugin(&p).is_err());
    }
}
fn script_trigger(s: &mut GameState, once: bool) -> TriggerId {
    let id = TriggerId(v7(0xF001));
    let action = TriggerAction::PluginAction {
        action: "fireball".into(),
        source: Some(tid(0)),
        target: None,
        x: 2240,
        y: 1120,
    };
    let definition = TriggerDef {
        name: "Spell and outcome".into(),
        enabled: true,
        once,
        map_ids: vec![mid(0)],
        event: TriggerEvent::Manual,
        tokens: Vec::new(),
        condition: None,
        steps: vec![
            TriggerStep { when: None, action },
            TriggerStep {
                when: Some("@action.completed == 1".into()),
                action: TriggerAction::Chat {
                    text: "after completed".into(),
                    gm_only: false,
                },
            },
            TriggerStep {
                when: Some("@action.countered == 1".into()),
                action: TriggerAction::Chat {
                    text: "after countered".into(),
                    gm_only: false,
                },
            },
        ],
    };
    run(s, Role::Gm, Intent::CreateTrigger { id, definition }, None).unwrap();
    id
}
fn narration_count(events: &[Event], text: &str) -> usize {
    events.iter().filter(|e| matches!(e, Event::ChatMessagePosted { message } if matches!(&message.system, Some(SystemMessage::Narration { text: actual }) if actual == text))).count()
}
#[test]
fn trigger_waits_and_resumes_from_a_persisted_position_once() {
    for accept in [false, true] {
        let mut s = interruption_world("1");
        let id = script_trigger(&mut s, true);
        let events = run(
            &mut s,
            Role::Gm,
            Intent::FireTrigger {
                id,
                token: Some(tid(0)),
            },
            None,
        )
        .unwrap();
        assert_eq!(narration_count(&events, "after completed"), 0);
        assert_eq!(narration_count(&events, "after countered"), 0);
        assert_eq!(s.plugin_runtime.trigger_runs.len(), 1);
        assert!(s.triggers[&id].fired);
        assert!(
            snapshot_for_user(Role::Player, PLAYER, &s, Seq(0), [])
                .plugin_runtime
                .trigger_runs
                .is_empty()
        );
        let mut s = GameState::from_snapshot(s.to_snapshot(Seq(0)));
        let events = answer_cast(&mut s, accept, 100).unwrap();
        assert!(s.plugin_runtime.trigger_runs.is_empty());
        assert_eq!(
            narration_count(
                &events,
                if accept {
                    "after countered"
                } else {
                    "after completed"
                }
            ),
            1
        );
        assert_eq!(
            narration_count(
                &events,
                if accept {
                    "after completed"
                } else {
                    "after countered"
                }
            ),
            0
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::TriggerFired { .. }))
        );
    }
}
#[test]
fn gm_cancel_and_editing_a_trigger_abort_the_remaining_script() {
    for edit in [false, true] {
        let mut s = interruption_world("1");
        let id = script_trigger(&mut s, false);
        run(
            &mut s,
            Role::Gm,
            Intent::FireTrigger {
                id,
                token: Some(tid(0)),
            },
            None,
        )
        .unwrap();
        if edit {
            let mut def = s.triggers[&id].definition.clone();
            def.name = "Changed".into();
            let version = s.triggers[&id].version;
            run(
                &mut s,
                Role::Gm,
                Intent::UpdateTrigger {
                    id,
                    definition: def,
                },
                Some(version),
            )
            .unwrap();
            assert!(s.plugin_runtime.trigger_runs.is_empty());
            let events = answer_cast(&mut s, false, 100).unwrap();
            assert_eq!(narration_count(&events, "after completed"), 0);
        } else {
            let stack = s.plugin_runtime.action_stacks[0].id;
            let events = run(&mut s, Role::Gm, Intent::CancelActionStack { stack }, None).unwrap();
            assert!(s.plugin_runtime.trigger_runs.is_empty());
            assert_eq!(narration_count(&events, "after completed"), 0);
            assert_eq!(narration_count(&events, "after countered"), 0);
        }
    }
}

#[test]
fn turn_budgets_refresh_before_periodic_resource_penalties() {
    let mut s = economy_world();
    let id = EffectId(v7(0xE100));
    s.effects.insert(
        id,
        TokenEffectDto {
            id,
            token: tid(0),
            label: "Action drain".into(),
            condition: None,
            resource: Some("action".into()),
            dice: Some("1".into()),
            heal: false,
            duration: EffectDuration::Turns { remaining: 2 },
            version: Version::INITIAL,
        },
    );
    edit_sheet(&mut s, 0, |v| {
        v["automation"]["actions"] = 0.into();
    });
    next_economy_turn(&mut s);
    next_economy_turn(&mut s);
    assert_eq!(auto_value(&s, 0, "actions"), 0);
    assert_eq!(auto_value(&s, 0, "reaction"), 1);
}
#[test]
fn disabling_turn_economy_releases_timing_and_budget_costs_but_not_slots() {
    let mut s = economy_world();
    s.plugins
        .get_mut(s.system_plugin.as_ref().unwrap())
        .unwrap()
        .feature_settings
        .insert("turn_economy".into(), false);
    run(
        &mut s,
        Role::Gm,
        Intent::ExecutePluginAction {
            source: tid(1),
            action: "fireball".into(),
            target: None,
            x: 2240,
            y: 1120,
        },
        None,
    )
    .unwrap();
    assert_eq!(auto_value(&s, 1, "actions"), 1);
    assert_eq!(slots3(&s, 1), 1);
}
#[test]
fn a_trigger_can_wait_twice_without_repeating_earlier_steps() {
    let mut s = interruption_world("1");
    let plugin = s
        .plugins
        .get_mut(s.system_plugin.as_ref().unwrap())
        .unwrap();
    let rules = plugin.system.as_mut().unwrap();
    let mut followup = rules
        .mechanics
        .actions
        .iter()
        .find(|a| a.id == "fireball")
        .unwrap()
        .clone();
    followup.id = "followup".into();
    followup.costs.retain(|c| c.resource == "spell_slot_3");
    rules.mechanics.actions.push(followup);
    let id = script_trigger(&mut s, true);
    let mut definition = s.triggers[&id].definition.clone();
    definition.steps = vec![
        definition.steps[0].clone(),
        TriggerStep {
            when: Some("@action.completed == 1".into()),
            action: TriggerAction::PluginAction {
                action: "followup".into(),
                source: Some(tid(0)),
                target: None,
                x: 2240,
                y: 1120,
            },
        },
        TriggerStep {
            when: Some("@action.completed == 1".into()),
            action: TriggerAction::Chat {
                text: "finished twice".into(),
                gm_only: false,
            },
        },
    ];
    let version = s.triggers[&id].version;
    run(
        &mut s,
        Role::Gm,
        Intent::UpdateTrigger { id, definition },
        Some(version),
    )
    .unwrap();
    run(
        &mut s,
        Role::Gm,
        Intent::FireTrigger {
            id,
            token: Some(tid(0)),
        },
        None,
    )
    .unwrap();
    let first = s.plugin_runtime.action_stacks[0].id;
    let events = answer_cast(&mut s, false, 100).unwrap();
    assert_eq!(narration_count(&events, "finished twice"), 0);
    assert_eq!(slots3(&s, 0), 1);
    assert_eq!(s.plugin_runtime.trigger_runs.len(), 1);
    assert_ne!(s.plugin_runtime.action_stacks[0].id, first);
    let events = answer_cast(&mut s, false, 200).unwrap();
    assert_eq!(slots3(&s, 0), 0);
    assert_eq!(narration_count(&events, "finished twice"), 1);
    assert!(s.plugin_runtime.trigger_runs.is_empty());
}

#[test]
fn combat_initialization_resets_all_budgets_without_public_resource_announcements() {
    let mut s = economy_world();
    s.combats.clear();
    s.combatants.clear();
    s.tokens.get_mut(&tid(1)).unwrap().visible = false;
    for n in 0..2 {
        edit_sheet(&mut s, n, |v| {
            for key in [
                "actions",
                "bonus_actions",
                "reaction",
                "slot_casts_remaining",
            ] {
                v["automation"][key] = 0.into();
            }
        });
    }
    let events = run(
        &mut s,
        Role::Gm,
        Intent::StartCombat {
            id: CombatId(v7(0xE200)),
            map_id: mid(0),
        },
        None,
    )
    .unwrap();
    for n in 0..2 {
        for key in [
            "actions",
            "bonus_actions",
            "reaction",
            "slot_casts_remaining",
        ] {
            assert_eq!(auto_value(&s, n, key), 1);
        }
    }
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::ChatMessagePosted { .. }))
    );
}
#[test]
fn another_scene_cannot_advance_the_same_characters_budgets_during_a_window() {
    let mut s = interruption_world("1");
    cast_fire(&mut s).unwrap();
    let id = CombatId(v7(0xE300));
    let participant = CombatantId(v7(0xE301));
    s.combats.insert(
        id,
        CombatDto {
            id,
            map_id: mid(1),
            round: 1,
            turn: Some(participant),
            version: Version::INITIAL,
        },
    );
    s.combatants.insert(
        participant,
        CombatantDto {
            id: participant,
            combat_id: id,
            token_id: None,
            character_id: Some(cid(0)),
            name: "Duplicated character".into(),
            initiative: Some(20),
            sort_order: 0,
            version: Version::INITIAL,
        },
    );
    let before = s.to_snapshot(Seq(0));
    assert!(
        run(
            &mut s,
            Role::Gm,
            Intent::NextTurn { combat_id: id },
            Some(Version::INITIAL)
        )
        .is_err()
    );
    assert_eq!(s.to_snapshot(Seq(0)), before);
}

fn obstacle(s: &mut GameState, shape: ZoneShape, sight: bool, effect: bool) -> ZoneId {
    let id = ZoneId(v7(0xabba));
    run(
        s,
        Role::Gm,
        Intent::CreateZone {
            id,
            map_id: mid(0),
            name: "Obstacle".into(),
            shape,
        },
        None,
    )
    .unwrap();
    run(
        s,
        Role::CoGm,
        Intent::SetZoneOcclusion {
            id,
            occlusion: ZoneOcclusion { sight, effect },
        },
        Some(Version::INITIAL),
    )
    .unwrap();
    id
}
fn between() -> ZoneShape {
    ZoneShape::Rect {
        x: 1600,
        y: 800,
        width: 100,
        height: 600,
    }
}
#[test]
fn physical_effect_blocker_rejects_cast_before_any_cost_even_for_gm() {
    for role in [Role::Player, Role::Gm, Role::CoGm] {
        let mut s = world();
        obstacle(&mut s, between(), false, true);
        let before = s.clone();
        let result = run(
            &mut s,
            role,
            Intent::ExecutePluginAction {
                source: tid(0),
                action: "fireball".into(),
                target: None,
                x: 2240,
                y: 1120,
            },
            None,
        );
        assert!(result.is_err());
        assert_eq!(s.to_snapshot(Seq(0)), before.to_snapshot(Seq(0)));
    }
}
#[test]
fn opaque_smoke_blocks_counterspell_but_not_fireballs_effect() {
    let mut s = interruption_world("1");
    obstacle(&mut s, between(), true, false);
    cast_fire(&mut s).unwrap();
    assert!(s.plugin_runtime.action_stacks.is_empty());
    assert_eq!(slots3(&s, 0), 1);
    assert_eq!(slots3(&s, 1), 2);
    assert!(hp(&s, 1) < 100);
}
#[test]
fn area_effect_uses_origin_to_target_ray_and_leaves_targets_behind_wall_unharmed() {
    let mut s = world();
    obstacle(
        &mut s,
        ZoneShape::Rect {
            x: 2000,
            y: 0,
            width: 100,
            height: 3000,
        },
        false,
        true,
    );
    run(
        &mut s,
        Role::Player,
        Intent::ExecutePluginAction {
            source: tid(0),
            action: "fireball".into(),
            target: None,
            x: 1800,
            y: 1120,
        },
        None,
    )
    .unwrap();
    assert!(hp(&s, 0) < 100);
    assert_eq!(hp(&s, 1), 100);
    assert_eq!(slots3(&s, 0), 1);
}
#[test]
fn opportunity_attack_requires_unblocked_sight_at_reach_exit() {
    let mut s = world();
    obstacle(&mut s, between(), true, false);
    leave(&mut s);
    assert!(s.plugin_runtime.movements.is_empty());
    assert_eq!(s.tokens[&tid(1)].x, 10_000);
    assert_eq!(hp(&s, 1), 100);
}
#[test]
fn opportunity_sight_is_checked_at_boundary_not_at_movement_start() {
    let mut s = world();
    s.tokens.get_mut(&tid(1)).unwrap().x = 2000;
    // Start is visible along y=1120; the diagonal exit is behind this narrow obstacle.
    obstacle(
        &mut s,
        ZoneShape::Rect {
            x: 1500,
            y: 1200,
            width: 100,
            height: 500,
        },
        true,
        false,
    );
    run(
        &mut s,
        Role::Gm,
        Intent::MoveToken {
            id: tid(1),
            x: 10_000,
            y: 10_000,
        },
        None,
    )
    .unwrap();
    assert!(s.plugin_runtime.movements.is_empty());
}
#[test]
fn editing_obstacle_revalidates_and_closes_counterspell_window_without_spending_reaction() {
    let mut s = interruption_world("1");
    let id = obstacle(&mut s, between(), false, false);
    cast_fire(&mut s).unwrap();
    assert_eq!(s.plugin_runtime.action_stacks.len(), 1);
    let events = run(
        &mut s,
        Role::Gm,
        Intent::SetZoneOcclusion {
            id,
            occlusion: ZoneOcclusion {
                sight: true,
                effect: false,
            },
        },
        Some(Version(2)),
    )
    .unwrap();
    let stack = s.plugin_runtime.action_stacks[0].clone();
    assert!(stack.frames[0].offers.is_empty());
    assert!(!stack.frames[0].awaiting_reactions);
    assert_eq!(slots3(&s, 0), 2);
    run(
        &mut s,
        Role::Player,
        Intent::ContinuePluginAction {
            stack: stack.id,
            frame: stack.frames[0].id,
        },
        None,
    )
    .unwrap();
    assert!(s.plugin_runtime.action_stacks.is_empty());
    assert_eq!(slots3(&s, 0), 1);
    assert_eq!(slots3(&s, 1), 2);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::PluginRuntimeUpdated { .. }))
    );
    assert!(
        snapshot_for_user(Role::Player, PLAYER, &s, Seq(0), [])
            .zones
            .is_empty()
    );
}
#[test]
fn spatial_feature_can_be_disabled_without_changing_obstacles() {
    let mut s = world();
    obstacle(&mut s, between(), true, true);
    let p = s
        .plugins
        .get_mut(s.system_plugin.as_ref().unwrap())
        .unwrap();
    p.feature_settings.insert("spatial_rules".into(), false);
    cast_fire(&mut s).unwrap();
    assert!(hp(&s, 1) < 100);
}
#[test]
fn invalid_spatial_feature_and_area_on_single_target_are_rejected() {
    let mut s = world();
    let mut p = s.plugins.remove(s.system_plugin.as_ref().unwrap()).unwrap();
    let original = p.clone();
    p.system.as_mut().unwrap().mechanics.actions[0]
        .spatial
        .feature = Some("missing".into());
    assert!(check_plugin(&p).is_err());
    p = original;
    p.system.as_mut().unwrap().mechanics.actions[0]
        .spatial
        .area_line_of_effect = true;
    assert!(check_plugin(&p).is_err());
}

#[test]
fn newly_blocked_movement_reaction_cannot_be_accepted_but_may_be_declined() {
    let mut s = world();
    let id = obstacle(&mut s, between(), false, false);
    leave(&mut s);
    assert_eq!(s.plugin_runtime.movements.len(), 1);
    run(
        &mut s,
        Role::Gm,
        Intent::SetZoneOcclusion {
            id,
            occlusion: ZoneOcclusion {
                sight: true,
                effect: false,
            },
        },
        Some(Version(2)),
    )
    .unwrap();
    let before = s.to_snapshot(Seq(0));
    assert!(answer(&mut s, true).is_err());
    assert_eq!(s.to_snapshot(Seq(0)), before);
    answer(&mut s, false).unwrap();
    assert!(s.plugin_runtime.movements.is_empty());
    assert_eq!(s.tokens[&tid(1)].x, 10_000);
    assert_eq!(hp(&s, 1), 100);
}

#[test]
fn blocker_added_during_cast_preserves_declaration_cost_but_releases_deferred_costs() {
    let mut s = interruption_world("1");
    let id = obstacle(&mut s, between(), false, false);
    cast_fire(&mut s).unwrap();
    run(
        &mut s,
        Role::Gm,
        Intent::SetZoneOcclusion {
            id,
            occlusion: ZoneOcclusion {
                sight: true,
                effect: true,
            },
        },
        Some(Version(2)),
    )
    .unwrap();
    let stack = s.plugin_runtime.action_stacks[0].clone();
    run(
        &mut s,
        Role::Player,
        Intent::ContinuePluginAction {
            stack: stack.id,
            frame: stack.frames[0].id,
        },
        None,
    )
    .unwrap();
    assert!(s.plugin_runtime.action_stacks.is_empty());
    assert_eq!(slots3(&s, 0), 2);
    assert_eq!(hp(&s, 1), 100);
    let sheet: serde_json::Value = serde_json::from_str(&s.characters[&cid(0)].sheet_json).unwrap();
    assert_eq!(sheet["automation"]["actions"], 0);
}

#[test]
fn declared_counterspell_rechecks_sight_after_its_nested_window() {
    let mut s = interruption_world("1");
    edit_sheet(&mut s, 0, |v| {
        v["automation"]["can_counterspell"] = true.into();
    });
    let id = obstacle(&mut s, between(), false, false);
    cast_fire(&mut s).unwrap();
    answer_cast(&mut s, true, 100).unwrap();
    assert_eq!(s.plugin_runtime.action_stacks[0].frames.len(), 2);
    run(
        &mut s,
        Role::Gm,
        Intent::SetZoneOcclusion {
            id,
            occlusion: ZoneOcclusion {
                sight: true,
                effect: false,
            },
        },
        Some(Version(2)),
    )
    .unwrap();
    let stack = s.plugin_runtime.action_stacks[0].clone();
    assert!(stack.frames[1].offers.is_empty());
    run(
        &mut s,
        Role::Gm,
        Intent::ContinuePluginAction {
            stack: stack.id,
            frame: stack.frames[1].id,
        },
        None,
    )
    .unwrap();
    assert!(s.plugin_runtime.action_stacks.is_empty());
    assert_eq!(slots3(&s, 0), 1);
    assert_eq!(slots3(&s, 1), 2);
    assert!(hp(&s, 1) < 100);
    let sheet: serde_json::Value = serde_json::from_str(&s.characters[&cid(1)].sheet_json).unwrap();
    assert_eq!(sheet["automation"]["reaction"], 0);
}
