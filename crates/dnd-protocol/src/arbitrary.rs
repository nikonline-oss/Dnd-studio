//! Proptest strategies for protocol types (feature `arbitrary`).
//!
//! Entity ids are drawn from small fixed pools so that random intent sequences actually hit
//! existing entities, collide on creation and reference each other.

use proptest::prelude::*;
use proptest::sample::select;
use uuid::Uuid;

use crate::event::{Event, EventBatch};
use crate::ids::{CharacterId, IntentId, MapId, Seq, TokenId, Version};
use crate::intent::{Intent, IntentEnvelope};
use crate::model::{CampaignMeta, Grid, GridKind, MapDto, TokenDto, TokenLayer};

/// Number of distinct map ids produced by [`map_id`].
pub const MAP_POOL: u128 = 3;
/// Number of distinct token ids produced by [`token_id`].
pub const TOKEN_POOL: u128 = 6;
/// Number of distinct character ids produced by [`character_id`].
pub const CHARACTER_POOL: u128 = 2;

/// The `n`-th pooled map id.
#[must_use]
pub fn pooled_map_id(n: u128) -> MapId {
    MapId(Uuid::from_u128(0x1000 + n))
}

/// The `n`-th pooled token id.
#[must_use]
pub fn pooled_token_id(n: u128) -> TokenId {
    TokenId(Uuid::from_u128(0x2000 + n))
}

/// The `n`-th pooled character id.
#[must_use]
pub fn pooled_character_id(n: u128) -> CharacterId {
    CharacterId(Uuid::from_u128(0x3000 + n))
}

pub fn map_id() -> impl Strategy<Value = MapId> {
    (0..MAP_POOL).prop_map(pooled_map_id)
}

pub fn token_id() -> impl Strategy<Value = TokenId> {
    (0..TOKEN_POOL).prop_map(pooled_token_id)
}

pub fn character_id() -> impl Strategy<Value = CharacterId> {
    (0..CHARACTER_POOL).prop_map(pooled_character_id)
}

pub fn intent_id() -> impl Strategy<Value = IntentId> {
    any::<u128>().prop_map(|n| IntentId(Uuid::from_u128(n)))
}

pub fn version() -> impl Strategy<Value = Version> {
    (0i64..8).prop_map(Version)
}

/// Finite coordinates / sizes.
pub fn coord() -> impl Strategy<Value = f64> {
    -10_000.0f64..10_000.0
}

pub fn name() -> impl Strategy<Value = String> {
    "[a-zA-Zа-яА-Я0-9 ]{0,16}"
}

pub fn grid_kind() -> impl Strategy<Value = GridKind> {
    select(vec![
        GridKind::None,
        GridKind::Square,
        GridKind::HexFlat,
        GridKind::HexPointy,
    ])
}

pub fn grid() -> impl Strategy<Value = Grid> {
    (grid_kind(), 1.0f64..500.0, coord(), coord()).prop_map(
        |(kind, cell_size, offset_x, offset_y)| Grid {
            kind,
            cell_size,
            offset_x,
            offset_y,
        },
    )
}

pub fn layer() -> impl Strategy<Value = TokenLayer> {
    select(vec![
        TokenLayer::Background,
        TokenLayer::Tokens,
        TokenLayer::Gm,
    ])
}

pub fn intent() -> impl Strategy<Value = Intent> {
    prop_oneof![
        (proptest::option::of(name()), proptest::option::of(name()))
            .prop_map(|(name, description)| Intent::UpdateCampaignMeta { name, description }),
        (map_id(), name(), grid(), any::<bool>()).prop_map(
            |(id, name, grid, visible_to_players)| Intent::CreateMap {
                id,
                name,
                background_asset_id: None,
                grid,
                visible_to_players,
            }
        ),
        (
            map_id(),
            proptest::option::of(name()),
            proptest::option::of(grid()),
            proptest::option::of(any::<bool>())
        )
            .prop_map(|(id, name, grid, visible_to_players)| Intent::UpdateMap {
                id,
                name,
                grid,
                visible_to_players,
            }),
        map_id().prop_map(|id| Intent::DeleteMap { id }),
        (
            token_id(),
            map_id(),
            proptest::option::of(character_id()),
            name(),
            (coord(), coord(), 0.0f64..360.0, 0.1f64..10.0),
            layer(),
            any::<bool>()
        )
            .prop_map(
                |(id, map_id, character_id, name, (x, y, rotation, scale), layer, visible)| {
                    Intent::CreateToken {
                        id,
                        map_id,
                        character_id,
                        asset_id: None,
                        name,
                        x,
                        y,
                        rotation,
                        scale,
                        layer,
                        visible,
                    }
                }
            ),
        (
            token_id(),
            proptest::option::of(name()),
            proptest::option::of(0.0f64..360.0),
            proptest::option::of(0.1f64..10.0),
            proptest::option::of(layer()),
            proptest::option::of(any::<bool>())
        )
            .prop_map(
                |(id, name, rotation, scale, layer, visible)| Intent::UpdateToken {
                    id,
                    name,
                    rotation,
                    scale,
                    layer,
                    visible,
                }
            ),
        (token_id(), coord(), coord()).prop_map(|(id, x, y)| Intent::MoveToken { id, x, y }),
        token_id().prop_map(|id| Intent::DeleteToken { id }),
    ]
}

pub fn intent_envelope() -> impl Strategy<Value = IntentEnvelope> {
    (intent_id(), proptest::option::of(version()), intent()).prop_map(
        |(intent_id, expected_version, intent)| IntentEnvelope {
            intent_id,
            expected_version,
            intent,
        },
    )
}

pub fn map_dto() -> impl Strategy<Value = MapDto> {
    (map_id(), name(), grid(), any::<bool>(), version()).prop_map(
        |(id, name, grid, visible_to_players, version)| MapDto {
            id,
            name,
            background_asset_id: None,
            grid,
            visible_to_players,
            version,
        },
    )
}

pub fn token_dto() -> impl Strategy<Value = TokenDto> {
    (
        token_id(),
        map_id(),
        proptest::option::of(character_id()),
        name(),
        (coord(), coord(), 0.0f64..360.0, 0.1f64..10.0),
        layer(),
        any::<bool>(),
        version(),
    )
        .prop_map(
            |(id, map_id, character_id, name, (x, y, rotation, scale), layer, visible, version)| {
                TokenDto {
                    id,
                    map_id,
                    character_id,
                    asset_id: None,
                    name,
                    x,
                    y,
                    rotation,
                    scale,
                    layer,
                    visible,
                    version,
                }
            },
        )
}

pub fn event() -> impl Strategy<Value = Event> {
    prop_oneof![
        (name(), name(), version()).prop_map(|(name, description, version)| {
            Event::CampaignMetaUpdated {
                campaign: CampaignMeta {
                    name,
                    description,
                    version,
                },
            }
        }),
        map_dto().prop_map(|map| Event::MapCreated { map }),
        map_dto().prop_map(|map| Event::MapUpdated { map }),
        map_id().prop_map(|id| Event::MapDeleted { id }),
        token_dto().prop_map(|token| Event::TokenCreated { token }),
        token_dto().prop_map(|token| Event::TokenUpdated { token }),
        (token_id(), coord(), coord()).prop_map(|(id, x, y)| Event::TokenMoved { id, x, y }),
        token_id().prop_map(|id| Event::TokenDeleted { id }),
    ]
}

pub fn event_batch() -> impl Strategy<Value = EventBatch> {
    (
        (1i64..1_000_000).prop_map(Seq),
        proptest::option::of(intent_id()),
        proptest::collection::vec(event(), 0..4),
    )
        .prop_map(|(seq, caused_by, events)| EventBatch {
            seq,
            caused_by,
            events,
        })
}
