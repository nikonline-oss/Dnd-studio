//! Items as inventory objects (ADR-0062): free-form items, campaign states that gate what an item
//! does, numeric effects and granted conditions, and duplication.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use common::*;
use dnd_game_core::{
    GameState, active_conditions, active_modifier_labels, check_plugin, default_sheet,
    effective_fields,
};
use dnd_protocol::arbitrary::{pooled_character_id as cid, pooled_token_id as tid, v7};
use dnd_protocol::*;

/// The real SRD plugin: its sheet has `ac`, and its conditions include `poisoned`.
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
    sheet["hp"]["current"] = 100.into();
    sheet["hp"]["max"] = 100.into();
    sheet["ac"] = 10.into();
    for c in s.characters.values_mut() {
        c.sheet_json = sheet.to_string();
    }
    s.tokens.retain(|id, _| *id == tid(0) || *id == tid(1));
    s.system_plugin = Some(p.id.clone());
    s.plugins.insert(p.id.clone(), p);
    s
}

fn id(n: u128) -> ItemId {
    ItemId(v7(0x7000 + n))
}

fn create_free(s: &mut GameState, n: u128, name: &str) -> ItemId {
    let item = id(n);
    run(
        s,
        Role::Gm,
        Intent::CreateItem {
            id: item,
            item_kind: ItemKind::Item,
            name: name.into(),
            type_id: None,
            description: "A chipped ring.".into(),
            sheet_json: None,
            visible_to_players: false,
        },
        Some(Version::INITIAL),
    )
    .unwrap();
    item
}

/// Creates a free item in the campaign, gives a copy to character 0 and returns the instance id.
fn worn(s: &mut GameState, n: u128, name: &str) -> ItemId {
    let template = create_free(s, n, name);
    let instance = id(n + 100);
    run(
        s,
        Role::Gm,
        Intent::InstantiateItem {
            id: instance,
            template,
            owner: Some(cid(0)),
        },
        Some(Version::INITIAL),
    )
    .unwrap();
    instance
}

fn set_effects(s: &mut GameState, item: ItemId, effects: Vec<ItemEffect>) {
    let version = s.items[&item].version;
    run(
        s,
        Role::Gm,
        Intent::UpdateItem {
            id: item,
            name: None,
            description: None,
            sheet_json: None,
            visible_to_players: None,
            state: None,
            effects: Some(effects),
            grants: None,
            values: None,
        },
        Some(version),
    )
    .unwrap();
}

fn set_state(s: &mut GameState, item: ItemId, state: Option<&str>, role: Role) -> Result<(), ()> {
    let version = s.items[&item].version;
    run(
        s,
        role,
        Intent::UpdateItem {
            id: item,
            name: None,
            description: None,
            sheet_json: None,
            visible_to_players: None,
            state: Some(state.unwrap_or_default().to_owned()),
            effects: None,
            grants: None,
            values: None,
        },
        Some(version),
    )
    .map(|_| ())
    .map_err(|_| ())
}

fn ac(s: &GameState) -> i64 {
    let fields = effective_fields(s, &s.characters[&cid(0)]).unwrap();
    dnd_dice::FieldLookup::lookup(&fields, "ac").unwrap()
}

#[test]
fn a_free_item_needs_no_plugin_type() {
    let mut s = world();
    let item = create_free(&mut s, 1, "Chipped ring");
    let stored = &s.items[&item];
    assert_eq!(stored.type_id, None);
    assert_eq!(stored.sheet_json, "{}");
    assert_eq!(stored.description, "A chipped ring.");
    assert!(stored.is_template);
    // A free item refuses plugin data rather than storing data nothing reads.
    assert!(
        run(
            &mut s,
            Role::Gm,
            Intent::UpdateItem {
                id: item,
                name: None,
                description: None,
                sheet_json: Some(r#"{"charges":2}"#.into()),
                visible_to_players: None,
                state: None,
                effects: None,
                grants: None,
                values: None,
            },
            Some(Version::INITIAL),
        )
        .is_err()
    );
}

#[test]
fn an_active_item_adjusts_its_owners_fields() {
    let mut s = world();
    let ring = worn(&mut s, 2, "Ring of protection");
    assert_eq!(ac(&s), 10, "an item with no effects changes nothing");
    set_effects(
        &mut s,
        ring,
        vec![ItemEffect {
            path: "ac".into(),
            add: 2,
            minimum: None,
            maximum: None,
        }],
    );
    assert_eq!(ac(&s), 12);
    // The sheet itself is untouched: only the effective view changes.
    let sheet: serde_json::Value = serde_json::from_str(&s.characters[&cid(0)].sheet_json).unwrap();
    assert_eq!(sheet["ac"], 10);
    assert!(
        active_modifier_labels(&s, &s.characters[&cid(0)])
            .iter()
            .any(|l| matches!(l, Label::Text(name) if name == "Ring of protection"))
    );
}

#[test]
fn a_broken_item_stops_working() {
    let mut s = world();
    let ring = worn(&mut s, 3, "Ring of protection");
    set_effects(
        &mut s,
        ring,
        vec![ItemEffect {
            path: "ac".into(),
            add: 2,
            minimum: None,
            maximum: None,
        }],
    );
    assert_eq!(ac(&s), 12);
    set_state(&mut s, ring, Some("broken"), Role::Gm).unwrap();
    assert_eq!(
        ac(&s),
        10,
        "the broken ring still exists, it just stops helping"
    );
    set_state(&mut s, ring, Some("ok"), Role::Gm).unwrap();
    assert_eq!(ac(&s), 12, "and it works again once repaired");
}

#[test]
fn an_unknown_state_counts_as_inactive() {
    let mut s = world();
    let ring = worn(&mut s, 4, "Ring of protection");
    set_effects(
        &mut s,
        ring,
        vec![ItemEffect {
            path: "ac".into(),
            add: 2,
            minimum: None,
            maximum: None,
        }],
    );
    // Simulate a campaign that dropped the state the item names (an older save).
    s.items.get_mut(&ring).unwrap().state = Some("lost_state".into());
    assert_eq!(ac(&s), 10);
}

#[test]
fn an_active_item_grants_a_condition() {
    let mut s = world();
    let doll = worn(&mut s, 5, "Cursed doll");
    let version = s.items[&doll].version;
    run(
        &mut s,
        Role::Gm,
        Intent::UpdateItem {
            id: doll,
            name: None,
            description: None,
            sheet_json: None,
            visible_to_players: None,
            state: None,
            effects: None,
            grants: Some(vec!["poisoned".into()]),
            values: None,
        },
        Some(version),
    )
    .unwrap();
    assert!(active_conditions(&s, &s.characters[&cid(0)]).contains("poisoned"));
    set_state(&mut s, doll, Some("deactivated"), Role::Gm).unwrap();
    assert!(!active_conditions(&s, &s.characters[&cid(0)]).contains("poisoned"));
}

#[test]
fn a_character_without_the_item_gets_nothing() {
    let mut s = world();
    let ring = worn(&mut s, 6, "Ring of protection");
    set_effects(
        &mut s,
        ring,
        vec![ItemEffect {
            path: "ac".into(),
            add: 2,
            minimum: None,
            maximum: None,
        }],
    );
    let other = effective_fields(&s, &s.characters[&cid(1)]).unwrap();
    assert_eq!(dnd_dice::FieldLookup::lookup(&other, "ac"), Ok(10));
    // A template is a blueprint: however configured, it belongs to nobody.
    let template = s.items.values().find(|i| i.is_template).unwrap().id;
    s.items.get_mut(&template).unwrap().owner = None;
    assert_eq!(ac(&s), 12);
}

#[test]
fn effects_and_states_are_validated() {
    let mut s = world();
    let ring = worn(&mut s, 7, "Ring");
    // A field the sheet does not have.
    assert!(
        run(
            &mut s,
            Role::Gm,
            Intent::UpdateItem {
                id: ring,
                name: None,
                description: None,
                sheet_json: None,
                visible_to_players: None,
                state: None,
                effects: Some(vec![ItemEffect {
                    path: "no_such_field".into(),
                    add: 1,
                    minimum: None,
                    maximum: None,
                }]),
                grants: None,
                values: None,
            },
            Some(Version(1)),
        )
        .is_err()
    );
    // A state the campaign does not define.
    assert!(set_state(&mut s, ring, Some("melted"), Role::Gm).is_err());
    // A condition the system does not have.
    assert!(
        run(
            &mut s,
            Role::Gm,
            Intent::UpdateItem {
                id: ring,
                name: None,
                description: None,
                sheet_json: None,
                visible_to_players: None,
                state: None,
                effects: None,
                grants: Some(vec!["no_such_condition".into()]),
                values: None,
            },
            Some(Version(1)),
        )
        .is_err()
    );
    // A minimum above the maximum can never hold.
    assert!(
        run(
            &mut s,
            Role::Gm,
            Intent::UpdateItem {
                id: ring,
                name: None,
                description: None,
                sheet_json: None,
                visible_to_players: None,
                state: None,
                effects: Some(vec![ItemEffect {
                    path: "ac".into(),
                    add: 1,
                    minimum: Some(20),
                    maximum: Some(10),
                }]),
                grants: None,
                values: None,
            },
            Some(Version(1)),
        )
        .is_err()
    );
}

#[test]
fn a_player_flips_the_state_of_their_item_but_cannot_rewrite_its_effects() {
    let mut s = world();
    let ring = worn(&mut s, 8, "Ring");
    set_effects(
        &mut s,
        ring,
        vec![ItemEffect {
            path: "ac".into(),
            add: 2,
            minimum: None,
            maximum: None,
        }],
    );
    // Activating or breaking a wand is the table's business.
    set_state(&mut s, ring, Some("broken"), Role::Player).unwrap();
    assert_eq!(ac(&s), 10);
    // What the item does is the GM's.
    let version = s.items[&ring].version;
    let forbidden = run(
        &mut s,
        Role::Player,
        Intent::UpdateItem {
            id: ring,
            name: None,
            description: None,
            sheet_json: None,
            visible_to_players: None,
            state: None,
            effects: Some(vec![ItemEffect {
                path: "ac".into(),
                add: 30,
                minimum: None,
                maximum: None,
            }]),
            grants: None,
            values: None,
        },
        Some(version),
    );
    assert_eq!(forbidden, Err(RejectReason::Forbidden));
}

#[test]
fn duplicate_copies_templates_and_instances_independently() {
    let mut s = world();
    let template = create_free(&mut s, 9, "Lantern");
    let copy = id(10);
    run(
        &mut s,
        Role::Gm,
        Intent::DuplicateItem {
            id: template,
            new_id: copy,
        },
        None,
    )
    .unwrap();
    assert_eq!(s.items[&copy].name, "Lantern");
    assert!(s.items[&copy].is_template);
    assert_eq!(s.items[&copy].version, Version::INITIAL);

    // Editing the copy leaves the original alone.
    run(
        &mut s,
        Role::Gm,
        Intent::UpdateItem {
            id: copy,
            name: Some("Lantern (cracked)".into()),
            description: None,
            sheet_json: None,
            visible_to_players: None,
            state: None,
            effects: None,
            grants: None,
            values: None,
        },
        Some(Version::INITIAL),
    )
    .unwrap();
    assert_eq!(s.items[&template].name, "Lantern");
    assert_eq!(s.items[&copy].name, "Lantern (cracked)");

    // An instance duplicates as another instance of the same template.
    let instance = worn(&mut s, 11, "Dagger");
    let instance_copy = id(12);
    run(
        &mut s,
        Role::Gm,
        Intent::DuplicateItem {
            id: instance,
            new_id: instance_copy,
        },
        None,
    )
    .unwrap();
    assert!(!s.items[&instance_copy].is_template);
    assert_eq!(s.items[&instance_copy].owner, Some(cid(0)));
    assert_eq!(
        s.items[&instance_copy].template,
        s.items[&instance].template
    );

    // Duplicating something that is not there is a plain not-found.
    assert_eq!(
        run(
            &mut s,
            Role::Gm,
            Intent::DuplicateItem {
                id: id(99),
                new_id: id(98),
            },
            None,
        ),
        Err(RejectReason::NotFound)
    );
}

#[test]
fn campaign_states_can_be_replaced_and_reset() {
    let mut s = world();
    let custom = vec![
        ItemStateDef {
            id: "sealed".into(),
            label: Label::Text("Sealed".into()),
            active: true,
        },
        ItemStateDef {
            id: "spent".into(),
            label: Label::Text("Spent".into()),
            active: false,
        },
    ];
    let version = s.campaign.version;
    run(
        &mut s,
        Role::Gm,
        Intent::UpdateCampaignMeta {
            name: None,
            description: None,
            token_inspector: None,
            item_states: Some(custom.clone()),
            visibility: None,
        },
        Some(version),
    )
    .unwrap();
    assert_eq!(s.campaign.item_states, custom);

    let ring = worn(&mut s, 13, "Scroll");
    set_effects(
        &mut s,
        ring,
        vec![ItemEffect {
            path: "ac".into(),
            add: 1,
            minimum: None,
            maximum: None,
        }],
    );
    set_state(&mut s, ring, Some("spent"), Role::Gm).unwrap();
    assert_eq!(ac(&s), 10, "a custom inactive state switches the item off");

    // Duplicate ids and unsafe ids are refused; an empty list restores the built-ins.
    let version = s.campaign.version;
    assert!(
        run(
            &mut s,
            Role::Gm,
            Intent::UpdateCampaignMeta {
                name: None,
                description: None,
                token_inspector: None,
                item_states: Some(vec![custom[0].clone(), custom[0].clone()]),
                visibility: None,
            },
            Some(version),
        )
        .is_err()
    );
    let version = s.campaign.version;
    run(
        &mut s,
        Role::Gm,
        Intent::UpdateCampaignMeta {
            name: None,
            description: None,
            token_inspector: None,
            item_states: Some(Vec::new()),
            visibility: None,
        },
        Some(version),
    )
    .unwrap();
    assert_eq!(s.campaign.item_states(), ItemStateDef::builtin());
    assert_eq!(ac(&s), 10);
}
