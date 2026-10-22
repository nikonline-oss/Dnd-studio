//! Structural validation of incoming messages (ADR-0015). Runs before any game rule, on the
//! session server and on the desktop's local server alike (via `dnd_game_core::apply`).
//!
//! Checks shape only: no rights are ever derived from an id, and existence/collisions are decided
//! by the core against the current state.

use uuid::{Uuid, Variant};

use crate::error::RejectReason;
use crate::ids::{Seq, Version};
use crate::intent::{Intent, IntentEnvelope};
use crate::message::{ClientMessage, Envelope};
use crate::model::FogShape;

/// `Number.MAX_SAFE_INTEGER`: the largest integer a JS client represents exactly (2^53 − 1).
pub const MAX_SAFE_INTEGER: i64 = (1 << 53) - 1;

/// Whether `value` survives a round trip through a JS `number` (|value| ≤ 2^53 − 1).
#[must_use]
pub fn is_js_safe(value: i64) -> bool {
    (-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&value)
}

/// RFC 9562 version 7 (time-ordered) UUID with the standard variant.
#[must_use]
pub fn is_uuid_v7(id: &Uuid) -> bool {
    id.get_version_num() == 7 && id.get_variant() == Variant::RFC4122
}

fn js_safe(value: i64, field: &str) -> Result<(), RejectReason> {
    if is_js_safe(value) {
        Ok(())
    } else {
        Err(RejectReason::invalid(field))
    }
}

fn uuid_v7(id: &Uuid, field: &str) -> Result<(), RejectReason> {
    if is_uuid_v7(id) {
        Ok(())
    } else {
        Err(RejectReason::invalid(field))
    }
}

impl Version {
    /// The next version, or `None` once it would leave the JS-safe range.
    #[must_use]
    pub fn checked_next(self) -> Option<Self> {
        let next = self.0.checked_add(1)?;
        is_js_safe(next).then_some(Self(next))
    }
}

impl Seq {
    /// The next sequence number, or `None` once it would leave the JS-safe range.
    #[must_use]
    pub fn checked_next(self) -> Option<Self> {
        let next = self.0.checked_add(1)?;
        is_js_safe(next).then_some(Self(next))
    }
}

impl Intent {
    /// Every integer field of the intent, for the JS-safe range check (ADR-0015, ADR-0016).
    #[must_use]
    pub fn integers(&self) -> Vec<(&'static str, i64)> {
        match self {
            Self::CreateMap { width, height, .. } => vec![("width", *width), ("height", *height)],
            Self::CreateToken { x, y, .. } | Self::MoveToken { x, y, .. } => {
                vec![("x", *x), ("y", *y)]
            }
            Self::MoveJournalEntry { sort_order, .. } => vec![("sort_order", *sort_order)],
            Self::UpdateCampaignMeta { .. }
            | Self::UpdateMap { .. }
            | Self::DeleteMap { .. }
            | Self::UpdateToken { .. }
            | Self::DeleteToken { .. }
            | Self::CreateCharacter { .. }
            | Self::UpdateCharacter { .. }
            | Self::SubmitCharacter { .. }
            | Self::ApproveCharacter { .. }
            | Self::ReturnCharacter { .. }
            | Self::ArchiveCharacter { .. }
            | Self::AssignCharacterOwner { .. }
            | Self::CreateJournalEntry { .. }
            | Self::UpdateJournalEntry { .. }
            | Self::DeleteJournalEntry { .. }
            | Self::SendMessage { .. }
            | Self::RollDice { .. }
            | Self::ResetFog { .. } => Vec::new(),
            Self::RegisterAsset { asset } => vec![("size_bytes", asset.size_bytes)],
            Self::ApplyFog { op, .. } => match &op.shape {
                FogShape::Rect {
                    x,
                    y,
                    width,
                    height,
                } => vec![
                    ("op.shape.x", *x),
                    ("op.shape.y", *y),
                    ("op.shape.width", *width),
                    ("op.shape.height", *height),
                ],
                FogShape::Brush { radius, points } => std::iter::once(("op.shape.radius", *radius))
                    .chain(points.iter().map(|p| ("op.shape.points", *p)))
                    .collect(),
            },
        }
    }
}

impl Intent {
    /// Ids of entities this intent creates. They must be client-generated UUIDv7 (ADR-0007).
    #[must_use]
    pub fn created_id(&self) -> Option<Uuid> {
        match self {
            Self::CreateMap { id, .. } => Some(id.0),
            Self::CreateToken { id, .. } => Some(id.0),
            Self::CreateCharacter { id, .. } => Some(id.0),
            Self::CreateJournalEntry { id, .. } => Some(id.0),
            Self::SendMessage { id, .. } | Self::RollDice { id, .. } => Some(id.0),
            Self::RegisterAsset { asset } => Some(asset.id.0),
            Self::ApplyFog { op, .. } => Some(op.id.0),
            Self::UpdateCampaignMeta { .. }
            | Self::UpdateMap { .. }
            | Self::DeleteMap { .. }
            | Self::ResetFog { .. }
            | Self::UpdateToken { .. }
            | Self::MoveToken { .. }
            | Self::DeleteToken { .. }
            | Self::UpdateCharacter { .. }
            | Self::SubmitCharacter { .. }
            | Self::ApproveCharacter { .. }
            | Self::ReturnCharacter { .. }
            | Self::ArchiveCharacter { .. }
            | Self::AssignCharacterOwner { .. }
            | Self::UpdateJournalEntry { .. }
            | Self::MoveJournalEntry { .. }
            | Self::DeleteJournalEntry { .. } => None,
        }
    }
}

impl IntentEnvelope {
    /// `intent_id` and created ids are UUIDv7; every integer is JS-safe.
    pub fn validate(&self) -> Result<(), RejectReason> {
        uuid_v7(&self.intent_id.0, "intent_id")?;
        if let Some(version) = self.expected_version {
            js_safe(version.0, "expected_version")?;
        }
        for (field, value) in self.intent.integers() {
            js_safe(value, field)?;
        }
        if let Some(id) = self.intent.created_id() {
            uuid_v7(&id, "id")?;
        }
        Ok(())
    }
}

impl ClientMessage {
    pub fn validate(&self) -> Result<(), RejectReason> {
        match self {
            Self::Intent(envelope) => envelope.validate(),
            Self::Ping => Ok(()),
            Self::Resync { last_seq } => js_safe(last_seq.0, "last_seq"),
        }
    }
}

impl Envelope<ClientMessage> {
    /// Protocol version, header fields and the body.
    pub fn validate(&self) -> Result<(), RejectReason> {
        self.check_version()?;
        uuid_v7(&self.id.0, "id")?;
        js_safe(self.ts, "ts")?;
        if let Some(seq) = self.seq {
            js_safe(seq.0, "seq")?;
        }
        self.body.validate()
    }
}

/// A character sheet must be a JSON object of at most [`MAX_SHEET_BYTES`] whose numbers are all
/// JS-safe integers (ADR-0016): no fractions, no exponents beyond the safe range.
pub fn validate_sheet_json(sheet: &str) -> Result<(), RejectReason> {
    let invalid = || RejectReason::invalid("sheet_json");
    if sheet.len() > crate::model::MAX_SHEET_BYTES {
        return Err(invalid());
    }
    let value: serde_json::Value = serde_json::from_str(sheet).map_err(|_| invalid())?;
    if !value.is_object() {
        return Err(invalid());
    }
    fn integers_only(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Number(n) => n.as_i64().is_some_and(is_js_safe),
            serde_json::Value::Array(items) => items.iter().all(integers_only),
            serde_json::Value::Object(map) => map.values().all(integers_only),
            serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::String(_) => {
                true
            }
        }
    }
    if integers_only(&value) {
        Ok(())
    } else {
        Err(invalid())
    }
}

/// Why a sheet field could not be read as a dice/formula value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SheetFieldError {
    /// No such path in the sheet.
    Unknown,
    /// The value is not an integer (ADR-0016).
    NotAnInteger,
}

/// Reads the integer at a dotted `path` (`hp.max`) of a character sheet.
pub fn sheet_field(sheet: &str, path: &str) -> Result<i64, SheetFieldError> {
    let value: serde_json::Value =
        serde_json::from_str(sheet).map_err(|_| SheetFieldError::Unknown)?;
    let mut node = &value;
    for segment in path.split('.') {
        node = node.get(segment).ok_or(SheetFieldError::Unknown)?;
    }
    node.as_i64().ok_or(SheetFieldError::NotAnInteger)
}
