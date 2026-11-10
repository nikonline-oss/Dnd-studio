//! Characters as they live in the campaign file (ADR-0054): the storage adapter between the
//! domain value and the `characters` row.
//!
//! The domain type and the row differ in three ways the adapter owns: enums are stored as strings,
//! ids and owners as text, and the sheet is a JSON document in one column. Everything else in the
//! core keeps working with [`CharacterDto`]; only this module and the SQL statements in
//! `campaign.rs` know the column layout.

use dnd_protocol::{CharacterDto, CharacterId, CharacterKind, CharacterStatus, UserId, Version};

use crate::codec::{enum_from_str, enum_to_str, opt_uuid_from_str, uuid_from_str};
use crate::error::DbResult;

/// One `characters` row.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredCharacter {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub owner: Option<String>,
    pub status: String,
    pub sheet: String,
    pub conditions: String,
    pub version: i64,
}

impl StoredCharacter {
    /// The row of a character the authority decided to keep.
    pub fn from_character(character: &CharacterDto) -> DbResult<Self> {
        Ok(Self {
            id: character.id.to_string(),
            kind: enum_to_str(&character.kind)?,
            name: character.name.clone(),
            owner: character.owner.map(|user| user.to_string()),
            status: enum_to_str(&character.status)?,
            sheet: character.sheet_json.clone(),
            conditions: serde_json::to_string(&character.conditions)?,
            version: character.version.0,
        })
    }

    /// A character read back from the file. Broken columns are reported as invalid data with the
    /// column name, never silently replaced by a default (ADR-0054).
    pub fn into_character(self) -> DbResult<CharacterDto> {
        Ok(CharacterDto {
            id: CharacterId(uuid_from_str("characters.id", &self.id)?),
            kind: enum_from_str::<CharacterKind>("characters.kind", &self.kind)?,
            name: self.name,
            owner: opt_uuid_from_str("characters.owner_user_id", self.owner)?.map(UserId),
            status: enum_from_str::<CharacterStatus>("characters.status", &self.status)?,
            sheet_json: self.sheet,
            conditions: serde_json::from_str(&self.conditions)?,
            version: Version(self.version),
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn hero() -> CharacterDto {
        CharacterDto {
            id: CharacterId::new_v7(),
            kind: CharacterKind::Npc,
            name: "Bandit".into(),
            owner: Some(UserId::new_v7()),
            status: CharacterStatus::Active,
            sheet_json: r#"{"hp":{"current":3}}"#.into(),
            conditions: vec!["prone".into()],
            version: Version(4),
        }
    }

    #[test]
    fn a_character_survives_the_round_trip() {
        let character = hero();
        let stored = StoredCharacter::from_character(&character).unwrap();
        assert_eq!(stored.kind, "npc");
        assert_eq!(stored.status, "active");
        assert_eq!(stored.version, 4);
        assert_eq!(stored.into_character().unwrap(), character);
    }

    #[test]
    fn an_ownerless_character_stays_ownerless() {
        let mut character = hero();
        character.owner = None;
        let stored = StoredCharacter::from_character(&character).unwrap();
        assert_eq!(stored.owner, None);
        assert_eq!(stored.into_character().unwrap().owner, None);
    }

    #[test]
    fn unknown_columns_are_reported_not_defaulted() {
        let mut stored = StoredCharacter::from_character(&hero()).unwrap();
        stored.kind = "dragon".into();
        assert!(stored.into_character().is_err());
        let mut stored = StoredCharacter::from_character(&hero()).unwrap();
        stored.id = "not-a-uuid".into();
        assert!(stored.into_character().is_err());
        let mut stored = StoredCharacter::from_character(&hero()).unwrap();
        stored.conditions = "{oops".into();
        assert!(stored.into_character().is_err());
    }
}
