//! Proptest strategies for protocol types (feature `arbitrary`).
//!
//! Entity ids are drawn from small fixed pools so that random intent sequences actually hit
//! existing entities, collide on creation and reference each other.

use proptest::prelude::*;
use proptest::sample::select;
use uuid::{Builder, Uuid};

use crate::board::{
    BoardDto, BoardEdgeDto, BoardLink, BoardNodeContent, BoardNodeDto, BoardVisibility,
};
use crate::event::{Event, EventBatch};
use crate::ids::{
    BoardEdgeId, BoardId, BoardNodeId, CharacterId, CombatId, CombatantId, FogOpId, IntentId,
    JournalFolderId, MapId, Role, Seq, TokenId, UserId, Version,
};
use crate::intent::{Intent, IntentEnvelope};
use crate::model::{
    CampaignMeta, CharacterKind, CombatDto, CombatantDto, FogOp, FogOpKind, FogShape, Grid,
    GridKind, JournalFolderDto, MapDto, TokenDto, TokenLayer, UserDto,
};

/// Number of distinct map ids produced by [`map_id`].
pub const MAP_POOL: u128 = 3;
/// Number of distinct token ids produced by [`token_id`].
pub const TOKEN_POOL: u128 = 6;
/// Number of distinct character ids produced by [`character_id`].
pub const CHARACTER_POOL: u128 = 2;

/// A deterministic, valid UUIDv7 for test fixtures (the timestamp field carries `n`).
#[must_use]
pub fn v7(n: u128) -> Uuid {
    let ms = u64::try_from(n).unwrap_or(u64::MAX) & 0xFFFF_FFFF_FFFF;
    Builder::from_unix_timestamp_millis(ms, &[0; 10]).into_uuid()
}

/// The `n`-th pooled map id.
#[must_use]
pub fn pooled_map_id(n: u128) -> MapId {
    MapId(v7(0x1000 + n))
}

/// The `n`-th pooled token id.
#[must_use]
pub fn pooled_token_id(n: u128) -> TokenId {
    TokenId(v7(0x2000 + n))
}

/// The `n`-th pooled character id.
#[must_use]
pub fn pooled_character_id(n: u128) -> CharacterId {
    CharacterId(v7(0x3000 + n))
}

pub fn board_id() -> impl Strategy<Value = BoardId> {
    (0x5_0000u128..0x5_0003).prop_map(|n| BoardId(v7(n)))
}

pub fn board_node_id() -> impl Strategy<Value = BoardNodeId> {
    (0x5_1000u128..0x5_1006).prop_map(|n| BoardNodeId(v7(n)))
}

pub fn board_edge_id() -> impl Strategy<Value = BoardEdgeId> {
    (0x5_2000u128..0x5_2004).prop_map(|n| BoardEdgeId(v7(n)))
}

pub fn board_visibility() -> impl Strategy<Value = BoardVisibility> {
    prop_oneof![Just(BoardVisibility::Gm), Just(BoardVisibility::Players)]
}

pub fn board_coord() -> impl Strategy<Value = i64> {
    -100_000i64..100_000
}

pub fn board_size() -> impl Strategy<Value = i64> {
    256i64..20_000
}

pub fn board_node_content() -> impl Strategy<Value = BoardNodeContent> {
    prop_oneof![
        "[a-z #*]{0,40}".prop_map(|markdown| BoardNodeContent::Note { markdown }),
        map_id().prop_map(|id| BoardNodeContent::Link {
            target: BoardLink::Map { id }
        }),
        character_id().prop_map(|id| BoardNodeContent::Link {
            target: BoardLink::Character { id }
        }),
    ]
}

pub fn board_dto() -> impl Strategy<Value = BoardDto> {
    (board_id(), name(), 0i64..20, any::<bool>(), version()).prop_map(
        |(id, name, sort_order, visible_to_players, version)| BoardDto {
            id,
            name,
            sort_order,
            visible_to_players,
            version,
        },
    )
}

pub fn board_node_dto() -> impl Strategy<Value = BoardNodeDto> {
    (
        board_node_id(),
        board_id(),
        board_node_content(),
        (board_coord(), board_coord(), board_size(), board_size()),
        board_visibility(),
        version(),
    )
        .prop_map(
            |(id, board_id, content, (x, y, width, height), visibility, version)| BoardNodeDto {
                id,
                board_id,
                content,
                x,
                y,
                width,
                height,
                visibility,
                version,
            },
        )
}

pub fn board_edge_dto() -> impl Strategy<Value = BoardEdgeDto> {
    (
        (
            board_edge_id(),
            board_id(),
            board_node_id(),
            board_node_id(),
        ),
        name(),
        any::<bool>(),
        board_visibility(),
        version(),
    )
        .prop_map(
            |((id, board_id, from, to), label, directed, visibility, version)| BoardEdgeDto {
                id,
                board_id,
                from,
                to,
                label,
                directed,
                visibility,
                version,
            },
        )
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
    (any::<u64>(), any::<[u8; 10]>()).prop_map(|(ms, random)| {
        IntentId(Builder::from_unix_timestamp_millis(ms >> 16, &random).into_uuid())
    })
}

pub fn version() -> impl Strategy<Value = Version> {
    (0i64..8).prop_map(Version)
}

/// Finite grid offsets (logical units).
pub fn offset() -> impl Strategy<Value = f64> {
    -10_000.0f64..10_000.0
}

/// Positions in Units16 (ADR-0016): mostly inside every generated map (maps are at least
/// 1000 units wide, i.e. 16 000 Units16), sometimes negative to exercise rejections.
pub fn position() -> impl Strategy<Value = i64> {
    -100i64..=16_000
}

/// Journal folder ids from a small pool, so moves and deletes hit existing folders.
pub fn journal_folder_id() -> impl Strategy<Value = JournalFolderId> {
    (0x4_0000u128..0x4_0006).prop_map(|n| JournalFolderId(v7(n)))
}

pub fn journal_folder_dto() -> impl Strategy<Value = JournalFolderDto> {
    (
        journal_folder_id(),
        proptest::option::of(journal_folder_id()),
        name(),
        0i64..50,
        version(),
    )
        .prop_map(|(id, parent, name, sort_order, version)| JournalFolderDto {
            id,
            parent,
            name,
            sort_order,
            version,
        })
}

pub fn combat_id() -> impl Strategy<Value = CombatId> {
    (0x2_0000u128..0x2_0004).prop_map(|n| CombatId(v7(n)))
}

pub fn combatant_id() -> impl Strategy<Value = CombatantId> {
    (0x3_0000u128..0x3_0008).prop_map(|n| CombatantId(v7(n)))
}

pub fn combat_dto() -> impl Strategy<Value = CombatDto> {
    (
        combat_id(),
        map_id(),
        1i64..20,
        proptest::option::of(combatant_id()),
        version(),
    )
        .prop_map(|(id, map_id, round, turn, version)| CombatDto {
            id,
            map_id,
            round,
            turn,
            version,
        })
}

pub fn combatant_dto() -> impl Strategy<Value = CombatantDto> {
    (
        combatant_id(),
        combat_id(),
        proptest::option::of(token_id()),
        name(),
        proptest::option::of(-5i64..40),
        0i64..100,
        version(),
    )
        .prop_map(
            |(id, combat_id, token_id, name, initiative, sort_order, version)| CombatantDto {
                id,
                combat_id,
                token_id,
                character_id: None,
                name,
                initiative,
                sort_order,
                version,
            },
        )
}

/// Fog operations; ids from a large range so most are new, shapes partly off the map.
pub fn fog_op() -> impl Strategy<Value = FogOp> {
    let rect =
        (position(), position(), -10i64..4000, -10i64..4000).prop_map(|(x, y, width, height)| {
            FogShape::Rect {
                x,
                y,
                width,
                height,
            }
        });
    let brush = (-10i64..2000, proptest::collection::vec(position(), 0..12))
        .prop_map(|(radius, points)| FogShape::Brush { radius, points });
    (
        (0x1_0000u128..0x10_0000).prop_map(|n| FogOpId(v7(n))),
        select(vec![FogOpKind::Reveal, FogOpKind::Hide]),
        prop_oneof![rect, brush],
    )
        .prop_map(|(id, kind, shape)| FogOp { id, kind, shape })
}

/// Logical map sizes.
pub fn map_size() -> impl Strategy<Value = i64> {
    1_000i64..=8_000
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
    (grid_kind(), 8.0f64..500.0, offset(), offset()).prop_map(
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
        (
            map_id(),
            name(),
            grid(),
            any::<bool>(),
            map_size(),
            map_size()
        )
            .prop_map(|(id, name, grid, visible_to_players, width, height)| {
                Intent::CreateMap {
                    id,
                    name,
                    background_asset_id: None,
                    width,
                    height,
                    grid,
                    visible_to_players,
                }
            }),
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
        (map_id(), fog_op()).prop_map(|(map_id, op)| Intent::ApplyFog { map_id, op }),
        (map_id(), any::<bool>())
            .prop_map(|(map_id, covered)| Intent::ResetFog { map_id, covered }),
        (combat_id(), map_id()).prop_map(|(id, map_id)| Intent::StartCombat { id, map_id }),
        (
            combatant_id(),
            combat_id(),
            proptest::option::of(token_id()),
            name()
        )
            .prop_map(|(id, combat_id, token_id, name)| Intent::AddCombatant {
                id,
                combat_id,
                token_id,
                character_id: None,
                name,
            }),
        combatant_id().prop_map(|id| Intent::RemoveCombatant { id }),
        combatant_id().prop_map(|id| Intent::RollInitiative { id }),
        (combatant_id(), proptest::option::of(-5i64..40))
            .prop_map(|(id, initiative)| Intent::SetInitiative { id, initiative }),
        combat_id().prop_map(|combat_id| Intent::NextTurn { combat_id }),
        combat_id().prop_map(|id| Intent::EndCombat { id }),
        (
            journal_folder_id(),
            proptest::option::of(journal_folder_id()),
            name()
        )
            .prop_map(|(id, parent, name)| Intent::CreateJournalFolder {
                id,
                parent,
                name
            }),
        (journal_folder_id(), name())
            .prop_map(|(id, name)| Intent::RenameJournalFolder { id, name }),
        (
            journal_folder_id(),
            proptest::option::of(journal_folder_id()),
            0i64..50
        )
            .prop_map(|(id, parent, sort_order)| Intent::MoveJournalFolder {
                id,
                parent,
                sort_order
            }),
        journal_folder_id().prop_map(|id| Intent::DeleteJournalFolder { id }),
        (board_id(), name()).prop_map(|(id, name)| Intent::CreateBoard { id, name }),
        (
            board_id(),
            proptest::option::of(name()),
            proptest::option::of(any::<bool>())
        )
            .prop_map(|(id, name, visible_to_players)| Intent::UpdateBoard {
                id,
                name,
                visible_to_players
            }),
        board_id().prop_map(|id| Intent::DeleteBoard { id }),
        (
            (board_node_id(), board_id()),
            board_node_content(),
            (board_coord(), board_coord(), board_size(), board_size()),
            board_visibility()
        )
            .prop_map(
                |((id, board_id), content, (x, y, width, height), visibility)| {
                    Intent::CreateBoardNode {
                        id,
                        board_id,
                        content,
                        x,
                        y,
                        width,
                        height,
                        visibility,
                    }
                }
            ),
        (
            board_node_id(),
            proptest::option::of(board_node_content()),
            proptest::option::of(board_size()),
            proptest::option::of(board_visibility())
        )
            .prop_map(|(id, content, width, visibility)| Intent::UpdateBoardNode {
                id,
                content,
                width,
                height: width,
                visibility
            }),
        (board_node_id(), board_coord(), board_coord())
            .prop_map(|(id, x, y)| Intent::MoveBoardNode { id, x, y }),
        board_node_id().prop_map(|id| Intent::DeleteBoardNode { id }),
        (
            (
                board_edge_id(),
                board_id(),
                board_node_id(),
                board_node_id()
            ),
            name(),
            any::<bool>(),
            board_visibility()
        )
            .prop_map(|((id, board_id, from, to), label, directed, visibility)| {
                Intent::CreateBoardEdge {
                    id,
                    board_id,
                    from,
                    to,
                    label,
                    directed,
                    visibility,
                }
            }),
        (
            board_edge_id(),
            proptest::option::of(name()),
            proptest::option::of(board_visibility())
        )
            .prop_map(|(id, label, visibility)| Intent::UpdateBoardEdge {
                id,
                label,
                directed: None,
                visibility
            }),
        board_edge_id().prop_map(|id| Intent::DeleteBoardEdge { id }),
        (
            token_id(),
            map_id(),
            proptest::option::of(character_id()),
            name(),
            (position(), position(), 0.0f64..360.0, 0.1f64..10.0),
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
        (token_id(), position(), position()).prop_map(|(id, x, y)| Intent::MoveToken { id, x, y }),
        token_id().prop_map(|id| Intent::DeleteToken { id }),
        (
            character_id(),
            select(vec![CharacterKind::Pc, CharacterKind::Npc]),
            name(),
            select(vec!["{}", r#"{"hp": 10}"#, r#"{"hp": 1.5}"#, "[]"])
        )
            .prop_map(
                |(id, character_kind, name, sheet)| Intent::CreateCharacter {
                    id,
                    character_kind,
                    name,
                    owner: None,
                    sheet_json: sheet.to_owned(),
                }
            ),
        (character_id(), proptest::option::of(name())).prop_map(|(id, name)| {
            Intent::UpdateCharacter {
                id,
                name,
                sheet_json: None,
            }
        }),
        character_id().prop_map(|id| Intent::SubmitCharacter { id }),
        character_id().prop_map(|id| Intent::ApproveCharacter { id }),
        character_id().prop_map(|id| Intent::ReturnCharacter { id }),
        character_id().prop_map(|id| Intent::ArchiveCharacter { id }),
        character_id().prop_map(|id| Intent::AssignCharacterOwner { id, owner: None }),
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
    (
        map_id(),
        name(),
        grid(),
        any::<bool>(),
        version(),
        map_size(),
        map_size(),
    )
        .prop_map(
            |(id, name, grid, visible_to_players, version, width, height)| MapDto {
                id,
                name,
                background_asset_id: None,
                width,
                height,
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
        (position(), position(), 0.0f64..360.0, 0.1f64..10.0),
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
        (map_id(), fog_op()).prop_map(|(map_id, op)| Event::FogApplied { map_id, op }),
        (map_id(), any::<bool>()).prop_map(|(map_id, covered)| Event::FogReset { map_id, covered }),
        combat_dto().prop_map(|combat| Event::CombatStarted { combat }),
        combat_dto().prop_map(|combat| Event::CombatUpdated { combat }),
        combat_id().prop_map(|id| Event::CombatEnded { id }),
        combatant_dto().prop_map(|combatant| Event::CombatantAdded { combatant }),
        combatant_dto().prop_map(|combatant| Event::CombatantUpdated { combatant }),
        combatant_id().prop_map(|id| Event::CombatantRemoved { id }),
        journal_folder_dto().prop_map(|folder| Event::JournalFolderCreated { folder }),
        journal_folder_dto().prop_map(|folder| Event::JournalFolderUpdated { folder }),
        journal_folder_id().prop_map(|id| Event::JournalFolderDeleted { id }),
        token_dto().prop_map(|token| Event::TokenCreated { token }),
        token_dto().prop_map(|token| Event::TokenUpdated { token }),
        (token_id(), position(), position()).prop_map(|(id, x, y)| Event::TokenMoved { id, x, y }),
        token_id().prop_map(|id| Event::TokenDeleted { id }),
        (
            (0u128..3).prop_map(|n| UserId(v7(0xB000 + n))),
            name(),
            prop_oneof![Just(Role::Player), Just(Role::Gm)]
        )
            .prop_map(|(id, name, role)| Event::UserJoined {
                user: UserDto { id, name, role }
            }),
        board_dto().prop_map(|board| Event::BoardCreated { board }),
        board_dto().prop_map(|board| Event::BoardUpdated { board }),
        board_id().prop_map(|id| Event::BoardDeleted { id }),
        board_node_dto().prop_map(|node| Event::BoardNodeCreated { node }),
        board_node_dto().prop_map(|node| Event::BoardNodeUpdated { node }),
        (board_node_id(), board_coord(), board_coord())
            .prop_map(|(id, x, y)| Event::BoardNodeMoved { id, x, y }),
        board_node_id().prop_map(|id| Event::BoardNodeDeleted { id }),
        board_edge_dto().prop_map(|edge| Event::BoardEdgeCreated { edge }),
        board_edge_dto().prop_map(|edge| Event::BoardEdgeUpdated { edge }),
        board_edge_id().prop_map(|id| Event::BoardEdgeDeleted { id }),
        Just(Event::Redacted),
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
