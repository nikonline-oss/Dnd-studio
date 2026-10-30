//! Board intents (ADR-0024): boards, their nodes (notes, links, pictures) and edges. GM roles only
//! (checked in `permissions::authorize`); what players see is decided in `visibility`.

use dnd_protocol::{
    BoardDto, BoardEdgeDto, BoardId, BoardLink, BoardNodeContent, BoardNodeDto, BoardNodeId,
    BoardVisibility, Event, Intent, MAX_BOARD_COORD, MAX_BOARD_EDGES, MAX_BOARD_NODES,
    MAX_NODE_SIZE, MAX_NOTE_LEN, MIN_NODE_SIZE, RejectReason, Version,
};

use crate::apply::{MAX_NAME_LEN, bump, valid_name};
use crate::state::GameState;

/// Events for a board intent; `None` if `intent` is not about boards.
pub(crate) fn apply(
    state: &GameState,
    intent: &Intent,
) -> Option<Result<Vec<Event>, RejectReason>> {
    let result = match intent {
        Intent::CreateBoard { id, name } => (|| {
            let sort_order = state
                .boards
                .values()
                .map(|b| b.sort_order)
                .max()
                .map_or(0, |m| m.saturating_add(1));
            Ok(vec![Event::BoardCreated {
                board: BoardDto {
                    id: *id,
                    name: valid_name(name, "name")?,
                    sort_order,
                    visible_to_players: false,
                    version: Version::INITIAL,
                },
            }])
        })(),
        Intent::UpdateBoard {
            id,
            name,
            visible_to_players,
        } => (|| {
            if name.is_none() && visible_to_players.is_none() {
                return Err(RejectReason::invalid("intent"));
            }
            let mut board = state
                .boards
                .get(id)
                .cloned()
                .ok_or(RejectReason::NotFound)?;
            if let Some(name) = name {
                board.name = valid_name(name, "name")?;
            }
            if let Some(visible) = visible_to_players {
                board.visible_to_players = *visible;
            }
            board.version = bump(board.version)?;
            Ok(vec![Event::BoardUpdated { board }])
        })(),
        Intent::DeleteBoard { id } => (|| {
            if !state.boards.contains_key(id) {
                return Err(RejectReason::NotFound);
            }
            Ok(delete_board(state, *id))
        })(),
        Intent::CreateBoardNode {
            id,
            board_id,
            content,
            x,
            y,
            width,
            height,
            visibility,
        } => (|| {
            if !state.boards.contains_key(board_id) {
                return Err(RejectReason::NotFound);
            }
            if nodes_on(state, *board_id) >= MAX_BOARD_NODES {
                return Err(RejectReason::invalid("board_id"));
            }
            Ok(vec![Event::BoardNodeCreated {
                node: BoardNodeDto {
                    id: *id,
                    board_id: *board_id,
                    content: valid_content(state, content)?,
                    x: coord(*x, "x")?,
                    y: coord(*y, "y")?,
                    width: size(*width, "width")?,
                    height: size(*height, "height")?,
                    visibility: shown_as(content, *visibility),
                    version: Version::INITIAL,
                },
            }])
        })(),
        Intent::UpdateBoardNode {
            id,
            content,
            width,
            height,
            visibility,
        } => (|| {
            if content.is_none() && width.is_none() && height.is_none() && visibility.is_none() {
                return Err(RejectReason::invalid("intent"));
            }
            let mut node = state
                .board_nodes
                .get(id)
                .cloned()
                .ok_or(RejectReason::NotFound)?;
            if let Some(content) = content {
                node.content = valid_content(state, content)?;
            }
            if let Some(width) = width {
                node.width = size(*width, "width")?;
            }
            if let Some(height) = height {
                node.height = size(*height, "height")?;
            }
            if let Some(visibility) = visibility {
                node.visibility = *visibility;
            }
            node.visibility = shown_as(&node.content, node.visibility);
            node.version = bump(node.version)?;
            Ok(vec![Event::BoardNodeUpdated { node }])
        })(),
        Intent::MoveBoardNode { id, x, y } => (|| {
            if !state.board_nodes.contains_key(id) {
                return Err(RejectReason::NotFound);
            }
            Ok(vec![Event::BoardNodeMoved {
                id: *id,
                x: coord(*x, "x")?,
                y: coord(*y, "y")?,
            }])
        })(),
        Intent::DeleteBoardNode { id } => (|| {
            let node = state.board_nodes.get(id).ok_or(RejectReason::NotFound)?;
            if let BoardNodeContent::Link {
                target: BoardLink::Map { id: map },
            } = node.content
            {
                return Ok(delete_map(state, map));
            }
            Ok(delete_node(state, *id))
        })(),
        Intent::CreateBoardEdge {
            id,
            board_id,
            from,
            to,
            label,
            directed,
            visibility,
        } => (|| {
            if !state.boards.contains_key(board_id) {
                return Err(RejectReason::NotFound);
            }
            // Both ends on this board, and not a loop.
            let on_board = |node: &BoardNodeId| {
                state
                    .board_nodes
                    .get(node)
                    .is_some_and(|n| n.board_id == *board_id)
            };
            if !on_board(from) {
                return Err(RejectReason::invalid("from"));
            }
            if !on_board(to) || from == to {
                return Err(RejectReason::invalid("to"));
            }
            let edges = state
                .board_edges
                .values()
                .filter(|e| e.board_id == *board_id)
                .count();
            if edges >= MAX_BOARD_EDGES {
                return Err(RejectReason::invalid("board_id"));
            }
            Ok(vec![Event::BoardEdgeCreated {
                edge: BoardEdgeDto {
                    id: *id,
                    board_id: *board_id,
                    from: *from,
                    to: *to,
                    label: valid_label(label)?,
                    directed: *directed,
                    visibility: *visibility,
                    version: Version::INITIAL,
                },
            }])
        })(),
        Intent::UpdateBoardEdge {
            id,
            label,
            directed,
            visibility,
        } => (|| {
            if label.is_none() && directed.is_none() && visibility.is_none() {
                return Err(RejectReason::invalid("intent"));
            }
            let mut edge = state
                .board_edges
                .get(id)
                .cloned()
                .ok_or(RejectReason::NotFound)?;
            if let Some(label) = label {
                edge.label = valid_label(label)?;
            }
            if let Some(directed) = directed {
                edge.directed = *directed;
            }
            if let Some(visibility) = visibility {
                edge.visibility = *visibility;
            }
            edge.version = bump(edge.version)?;
            Ok(vec![Event::BoardEdgeUpdated { edge }])
        })(),
        Intent::DeleteBoardEdge { id } => (|| {
            if !state.board_edges.contains_key(id) {
                return Err(RejectReason::NotFound);
            }
            Ok(vec![Event::BoardEdgeDeleted { id: *id }])
        })(),
        _ => return None,
    };
    Some(result)
}

/// The board with its edges, then its nodes, then itself, so no event leaves a dangling reference.
fn delete_board(state: &GameState, id: BoardId) -> Vec<Event> {
    let mut events: Vec<Event> = state
        .board_edges
        .values()
        .filter(|e| e.board_id == id)
        .map(|e| Event::BoardEdgeDeleted { id: e.id })
        .collect();
    events.extend(
        state
            .board_nodes
            .values()
            .filter(|n| n.board_id == id)
            .map(|n| Event::BoardNodeDeleted { id: n.id }),
    );
    events.push(Event::BoardDeleted { id });
    events
}

/// The node's edges, then the node.
pub(crate) fn delete_node(state: &GameState, id: BoardNodeId) -> Vec<Event> {
    let mut events: Vec<Event> = state
        .board_edges
        .values()
        .filter(|e| e.from == id || e.to == id)
        .map(|e| Event::BoardEdgeDeleted { id: e.id })
        .collect();
    events.push(Event::BoardNodeDeleted { id });
    events
}

/// Deleting a map card deletes its map everywhere, including links on other boards.
pub(crate) fn delete_map(state: &GameState, id: dnd_protocol::MapId) -> Vec<Event> {
    let nodes: std::collections::BTreeSet<_> = state.board_nodes.values()
        .filter(|n| matches!(n.content, BoardNodeContent::Link { target: BoardLink::Map { id: map } } if map == id))
        .map(|n| n.id).collect();
    let mut events: Vec<_> = state
        .board_edges
        .values()
        .filter(|e| nodes.contains(&e.from) || nodes.contains(&e.to))
        .map(|e| Event::BoardEdgeDeleted { id: e.id })
        .collect();
    events.extend(nodes.into_iter().map(|id| Event::BoardNodeDeleted { id }));
    events.extend(
        state
            .tokens_on(id)
            .map(|t| Event::TokenDeleted { id: t.id }),
    );
    events.push(Event::MapDeleted { id });
    events
}

fn nodes_on(state: &GameState, board: BoardId) -> usize {
    state
        .board_nodes
        .values()
        .filter(|n| n.board_id == board)
        .count()
}

/// Notes within the length limit; links and pictures to existing entities. A link may later
/// outlive its target (the card then shows that it is gone).
fn valid_content(
    state: &GameState,
    content: &BoardNodeContent,
) -> Result<BoardNodeContent, RejectReason> {
    let invalid = || RejectReason::invalid("content");
    match content {
        BoardNodeContent::Note { markdown } => {
            if markdown.chars().count() > MAX_NOTE_LEN {
                return Err(invalid());
            }
        }
        BoardNodeContent::Link { target } => {
            let exists = match target {
                BoardLink::Map { id } => state.maps.contains_key(id),
                BoardLink::Journal { id } => state.journal.contains_key(id),
                BoardLink::Character { id } => state.characters.contains_key(id),
            };
            if !exists {
                return Err(invalid());
            }
        }
        BoardNodeContent::Image { asset_id } => {
            // Any imported picture: every asset is an image (maps and tokens).
            if !state.assets.contains_key(asset_id) {
                return Err(invalid());
            }
        }
        BoardNodeContent::Trigger { id } => {
            if !state.triggers.contains_key(id) {
                return Err(invalid());
            }
        }
    }
    Ok(content.clone())
}

/// Trigger blocks are the GM's (ADR-0024): never open to players.
fn shown_as(content: &BoardNodeContent, visibility: BoardVisibility) -> BoardVisibility {
    match content {
        BoardNodeContent::Trigger { .. } => BoardVisibility::Gm,
        _ => visibility,
    }
}

fn coord(value: i64, field: &str) -> Result<i64, RejectReason> {
    if (-MAX_BOARD_COORD..=MAX_BOARD_COORD).contains(&value) {
        Ok(value)
    } else {
        Err(RejectReason::invalid(field))
    }
}

fn size(value: i64, field: &str) -> Result<i64, RejectReason> {
    if (MIN_NODE_SIZE..=MAX_NODE_SIZE).contains(&value) {
        Ok(value)
    } else {
        Err(RejectReason::invalid(field))
    }
}

/// Edge captions may be empty.
fn valid_label(label: &str) -> Result<String, RejectReason> {
    let trimmed = label.trim();
    if trimmed.chars().count() > MAX_NAME_LEN {
        return Err(RejectReason::invalid("label"));
    }
    Ok(trimmed.to_owned())
}
