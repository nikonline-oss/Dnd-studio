//! What the GM lets players read (ADR-0063): sheet field overrides, the name and the token picture,
//! and the rule that a hidden field never gets a fabricated default.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use common::*;
use dnd_game_core::{
    GameState, check_plugin, default_sheet, public_sheet_values, snapshot_for, snapshot_for_user,
};
use dnd_protocol::arbitrary::{pooled_character_id, pooled_token_id};
use dnd_protocol::*;

fn world() -> GameState {
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
    let mut sheet = default_sheet(rules);
    sheet["hp"]["current"] = 17.into();
    sheet["hp"]["max"] = 24.into();
    sheet["ac"] = 15.into();
    for c in s.characters.values_mut() {
        c.sheet_json = sheet.to_string();
    }
    s.tokens
        .retain(|id, _| *id == pooled_token_id(0) || *id == pooled_token_id(1));
    s.system_plugin = Some(p.id.clone());
    s.plugins.insert(p.id.clone(), p);
    s
}

fn set_policy(state: &mut GameState, policy: VisibilityPolicy) {
    run(
        state,
        Role::Gm,
        Intent::UpdateCampaignMeta {
            name: None,
            description: None,
            token_inspector: None,
            item_states: None,
            visibility: Some(policy),
        },
        Some(state.campaign.version),
    )
    .unwrap();
}

fn public_sheet(state: &GameState) -> serde_json::Value {
    let snapshot = snapshot_for(Role::Player, state, Seq(1), []);
    let hero = snapshot
        .characters
        .iter()
        .find(|c| c.id == pooled_character_id(0))
        .expect("the player knows about the active PC");
    serde_json::from_str(&hero.sheet_json).unwrap()
}

#[test]
fn hidden_fields_get_no_defaults_and_no_resource() {
    let state = world();
    let hero = state.characters[&pooled_character_id(0)].clone();
    // The public sheet carries no hit points at all…
    let public = public_sheet(&state);
    assert!(public.get("hp").is_none(), "hp is hidden by the SRD schema");
    // …and the derived resources are not invented from defaults either: the bar would have shown
    // 10/10 for a character at 17/24.
    let values = public_sheet_values(&state, &hero.sheet_json).unwrap();
    assert!(!values.resources.iter().any(|(id, _, _)| id == "hp"));

    // The owner's own sheet still has everything.
    let mine = snapshot_for_user(Role::Player, PLAYER, &state, Seq(1), []);
    let own = mine
        .characters
        .iter()
        .find(|c| c.id == pooled_character_id(0))
        .unwrap();
    let sheet: serde_json::Value = serde_json::from_str(&own.sheet_json).unwrap();
    assert_eq!(sheet["hp"]["current"], 17);
}

#[test]
fn the_gm_can_open_a_hidden_field() {
    let mut state = world();
    set_policy(
        &mut state,
        VisibilityPolicy {
            sheet: vec![FieldVisibility {
                path: "hp".into(),
                visible: true,
            }],
            ..VisibilityPolicy::default()
        },
    );
    let public = public_sheet(&state);
    assert_eq!(public["hp"]["current"], 17);
    assert_eq!(public["hp"]["max"], 24);
    // Opening one field does not open the others: AC stays as the schema says (public in SRD).
    assert_eq!(public["ac"], 15);
}

#[test]
fn the_gm_can_close_a_public_field_deeper_in_the_sheet() {
    let mut state = world();
    set_policy(
        &mut state,
        VisibilityPolicy {
            sheet: vec![FieldVisibility {
                path: "hp.current".into(),
                visible: false,
            }],
            ..VisibilityPolicy::default()
        },
    );
    // hp itself stays hidden by the schema; the rule is about the deeper path in a sheet where the
    // object is open.
    set_policy(
        &mut state,
        VisibilityPolicy {
            sheet: vec![
                FieldVisibility {
                    path: "hp".into(),
                    visible: true,
                },
                FieldVisibility {
                    path: "hp.current".into(),
                    visible: false,
                },
            ],
            ..VisibilityPolicy::default()
        },
    );
    let public = public_sheet(&state);
    assert!(
        public["hp"].get("current").is_none(),
        "the deeper rule wins"
    );
    assert_eq!(public["hp"]["max"], 24);
}

#[test]
fn a_hidden_name_travels_empty() {
    let mut state = world();
    let name = state.characters[&pooled_character_id(0)].name.clone();
    assert!(public_sheet(&state).is_object());
    let before = snapshot_for(Role::Player, &state, Seq(1), []);
    assert!(before.characters.iter().any(|c| c.name == name));

    set_policy(
        &mut state,
        VisibilityPolicy {
            name: Some(false),
            ..VisibilityPolicy::default()
        },
    );
    let after = snapshot_for(Role::Player, &state, Seq(1), []);
    let hero = after
        .characters
        .iter()
        .find(|c| c.id == pooled_character_id(0))
        .unwrap();
    assert_eq!(hero.name, "");
    // The GM and the owner still read the real name.
    let gm = snapshot_for(Role::Gm, &state, Seq(1), []);
    assert!(gm.characters.iter().any(|c| c.name == name));
    let owner = snapshot_for_user(Role::Player, PLAYER, &state, Seq(1), []);
    assert!(owner.characters.iter().any(|c| c.name == name));
}

#[test]
fn a_hidden_icon_clears_the_token_picture() {
    let mut state = world();
    let token = pooled_token_id(0);
    state.tokens.get_mut(&token).unwrap().asset_id = Some(AssetId::new_v7());
    let before = snapshot_for(Role::Player, &state, Seq(1), []);
    assert!(
        before
            .tokens
            .iter()
            .find(|t| t.id == token)
            .and_then(|t| t.asset_id)
            .is_some()
    );

    set_policy(
        &mut state,
        VisibilityPolicy {
            icon: Some(false),
            ..VisibilityPolicy::default()
        },
    );
    let after = snapshot_for(Role::Player, &state, Seq(1), []);
    assert_eq!(
        after
            .tokens
            .iter()
            .find(|t| t.id == token)
            .unwrap()
            .asset_id,
        None
    );
    let gm = snapshot_for(Role::Gm, &state, Seq(1), []);
    assert!(
        gm.tokens
            .iter()
            .find(|t| t.id == token)
            .and_then(|t| t.asset_id)
            .is_some()
    );
}

#[test]
fn visibility_rules_are_plain_and_unique() {
    let mut state = world();
    let duplicated = VisibilityPolicy {
        sheet: vec![
            FieldVisibility {
                path: "ac".into(),
                visible: true,
            },
            FieldVisibility {
                path: "ac".into(),
                visible: false,
            },
        ],
        ..VisibilityPolicy::default()
    };
    let before = state.campaign.version;
    assert!(
        run(
            &mut state,
            Role::Gm,
            Intent::UpdateCampaignMeta {
                name: None,
                description: None,
                token_inspector: None,
                item_states: None,
                visibility: Some(duplicated),
            },
            Some(before),
        )
        .is_err()
    );

    let unsafe_path = VisibilityPolicy {
        sheet: vec![FieldVisibility {
            path: "hp/current".into(),
            visible: true,
        }],
        ..VisibilityPolicy::default()
    };
    assert!(
        run(
            &mut state,
            Role::Gm,
            Intent::UpdateCampaignMeta {
                name: None,
                description: None,
                token_inspector: None,
                item_states: None,
                visibility: Some(unsafe_path),
            },
            Some(before),
        )
        .is_err()
    );
}
