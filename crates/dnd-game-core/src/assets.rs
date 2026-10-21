//! Asset registration (ADR-0017). The import pipeline runs outside the core (I/O); the core checks
//! that the record is well formed, unique by original hash, and only from GM roles (authorize).

use dnd_protocol::{AssetDto, AssetUsage, Event, Intent, RejectReason, Version};

use crate::state::GameState;

/// Largest image side after import, per usage (mirrors `dnd_assets::max_side`).
fn max_side(usage: AssetUsage) -> u32 {
    match usage {
        AssetUsage::Map => 16_384,
        AssetUsage::Token => 2048,
    }
}

fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(crate) fn apply(
    state: &GameState,
    intent: &Intent,
) -> Option<Result<Vec<Event>, RejectReason>> {
    let Intent::RegisterAsset { asset } = intent else {
        return None;
    };
    Some(validate(state, asset).map(|asset| {
        vec![Event::AssetRegistered {
            asset: Box::new(asset),
        }]
    }))
}

fn validate(state: &GameState, asset: &AssetDto) -> Result<AssetDto, RejectReason> {
    let invalid = |field: &str| Err(RejectReason::invalid(field));
    if state.assets.values().any(|a| a.sha256 == asset.sha256) {
        return Err(RejectReason::AlreadyExists);
    }
    if !is_sha256_hex(&asset.sha256) || !is_sha256_hex(&asset.thumbnail_sha256) {
        return invalid("sha256");
    }
    if asset
        .display_sha256
        .as_deref()
        .is_some_and(|s| !is_sha256_hex(s))
    {
        return invalid("display_sha256");
    }
    let max = max_side(asset.usage);
    if asset.width == 0 || asset.height == 0 || asset.width > max || asset.height > max {
        return invalid("width");
    }
    if asset.size_bytes <= 0 {
        return invalid("size_bytes");
    }
    if !matches!(
        asset.mime.as_str(),
        "image/png" | "image/jpeg" | "image/webp"
    ) {
        return invalid("mime");
    }
    if asset
        .original_name
        .as_deref()
        .is_some_and(|n| n.chars().count() > 255 || n.chars().any(char::is_control))
    {
        return invalid("original_name");
    }
    match asset.usage {
        AssetUsage::Map => {
            // Tiles must exactly cover the image (ADR-0017), each a valid file.
            let area: u64 = asset
                .tiles
                .iter()
                .map(|t| u64::from(t.width) * u64::from(t.height))
                .sum();
            let inside = asset.tiles.iter().all(|t| {
                is_sha256_hex(&t.sha256)
                    && t.width > 0
                    && t.height > 0
                    && t.width <= 2048
                    && t.height <= 2048
                    && u64::from(t.x) + u64::from(t.width) <= u64::from(asset.width)
                    && u64::from(t.y) + u64::from(t.height) <= u64::from(asset.height)
            });
            if asset.tiles.is_empty()
                || !inside
                || area != u64::from(asset.width) * u64::from(asset.height)
            {
                return invalid("tiles");
            }
        }
        AssetUsage::Token => {
            if !asset.tiles.is_empty() || asset.display_sha256.is_none() {
                return invalid("tiles");
            }
        }
    }
    Ok(AssetDto {
        version: Version::INITIAL,
        ..asset.clone()
    })
}
