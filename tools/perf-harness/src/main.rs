//! A4.1 baseline: latency, snapshot cost and export time on two fixed campaigns.
//!
//! Run with `cargo run --release -p dnd-perf-harness`. The numbers describe one machine; they are
//! a baseline for the budgets in `docs/perf-baseline.md`, not a promise about other hardware.
//!
//! What is measured, per campaign size:
//! - p50/p95/max wall time of `CampaignEngine::submit` for a token move, a sheet edit and a dice
//!   roll (the hot paths a table exercises);
//! - time and JSON size of a full snapshot and of a player's snapshot;
//! - export: checkpoint + copy + archive, the operation the GM waits for on stop/export.

use std::time::Instant;

use anyhow::{Context, Result};
use dnd_db::{ArchiveManifest, CampaignDb, CampaignEngine, LocalUser};
use dnd_game_core::{
    Actor, ForPlayers, IntentWindow, SplitMix64, filter_for_players, snapshot_for_user,
};
use dnd_protocol::*;

/// A campaign shape: maps, characters and tokens per map.
struct Shape {
    label: &'static str,
    maps: usize,
    tokens_per_map: usize,
}

const SHAPES: [Shape; 2] = [
    Shape {
        label: "small (1 map, 8 tokens, 2 characters)",
        maps: 1,
        tokens_per_map: 8,
    },
    Shape {
        label: "large (20 maps, 20 tokens each, 20 characters)",
        maps: 20,
        tokens_per_map: 20,
    },
];

const SAMPLES: usize = 120;

#[tokio::main]
async fn main() -> Result<()> {
    println!("# A4.1 baseline (release build)");
    println!();
    println!("| campaign | command | p50 ms | p95 ms | max ms |");
    println!("| --- | --- | ---: | ---: | ---: |");
    for shape in &SHAPES {
        measure(shape).await?;
    }
    Ok(())
}

async fn measure(shape: &Shape) -> Result<()> {
    let dir = tempfile::tempdir().context("temp dir")?;
    let path = dir.path().join("campaign.sqlite");
    let gm = LocalUser {
        id: UserId::new_v7(),
        name: "GM".into(),
        role: Role::Gm,
    };
    let db = CampaignDb::create(&path, "Perf", &gm, 0).await?;
    let mut engine =
        CampaignEngine::load(db, IntentWindow::DEFAULT, Box::new(SplitMix64::new(7)), 0).await?;
    let actor = Actor {
        user_id: gm.id,
        role: gm.role,
    };
    let player = gm.id;

    let mut clock = 0_i64;
    let mut envelopes = Vec::new();
    let mut maps = Vec::new();
    for m in 0..shape.maps {
        let id = MapId::new_v7();
        envelopes.push(Intent::CreateMap {
            id,
            name: format!("Map {m}"),
            description: String::new(),
            background_asset_id: None,
            width: 4000,
            height: 3000,
            grid: Grid::default(),
            visible_to_players: true,
        });
        maps.push(id);
    }
    let mut characters = Vec::new();
    for c in 0..shape.maps.max(2) {
        let id = CharacterId::new_v7();
        envelopes.push(Intent::CreateCharacter {
            id,
            character_kind: CharacterKind::Pc,
            name: format!("Hero {c}"),
            owner: None,
            sheet_json: "{}".into(),
        });
        characters.push(id);
    }
    let mut tokens = Vec::new();
    for (m, map) in maps.iter().enumerate() {
        for t in 0..shape.tokens_per_map {
            let id = TokenId::new_v7();
            envelopes.push(Intent::CreateToken {
                id,
                map_id: *map,
                character_id: (!characters.is_empty())
                    .then(|| characters[(m * shape.tokens_per_map + t) % characters.len()]),
                asset_id: None,
                name: format!("Token {t}"),
                x: 1120,
                y: 1120,
                rotation: 0.0,
                scale: 1.0,
                layer: TokenLayer::Tokens,
                visible: true,
                disposition: TokenDisposition::Neutral,
            });
            tokens.push(id);
        }
    }
    for (n, intent) in envelopes.into_iter().enumerate() {
        let encoded = IntentEnvelope {
            intent_id: IntentId::new_v7(),
            expected_version: None,
            ts_ms: clock,
            intent,
        };
        clock += 1;
        let (outcome, _) = engine.submit(&encoded, &actor, clock).await?;
        anyhow::ensure!(
            matches!(outcome, IntentOutcome::Accepted { .. }),
            "setup intent {n} rejected: {outcome:?}"
        );
    }

    let token = tokens[0];
    let character = characters[0];
    let mut sheet_version = engine.state().characters[&character].version;

    // Token moves: last-write-wins, the most frequent command at a table.
    let mut moves = Vec::new();
    for i in 0..SAMPLES {
        clock += 1;
        let envelope = IntentEnvelope {
            intent_id: IntentId::new_v7(),
            expected_version: None,
            ts_ms: clock,
            intent: Intent::MoveToken {
                id: token,
                x: 1120 + (i as i64 % 16) * 16,
                y: 1120,
            },
        };
        let started = Instant::now();
        let (outcome, _) = engine.submit(&envelope, &actor, clock).await?;
        moves.push(started.elapsed());
        anyhow::ensure!(
            matches!(outcome, IntentOutcome::Accepted { .. }),
            "{outcome:?}"
        );
    }

    // Sheet edits: an optimistic-version write of the whole sheet.
    let mut edits = Vec::new();
    for i in 0..SAMPLES {
        clock += 1;
        let envelope = IntentEnvelope {
            intent_id: IntentId::new_v7(),
            expected_version: Some(sheet_version),
            ts_ms: clock,
            intent: Intent::UpdateCharacter {
                id: character,
                name: None,
                sheet_json: Some(format!("{{\"mark\":{i}}}")),
            },
        };
        let started = Instant::now();
        let (outcome, _) = engine.submit(&envelope, &actor, clock).await?;
        edits.push(started.elapsed());
        anyhow::ensure!(
            matches!(outcome, IntentOutcome::Accepted { .. }),
            "{outcome:?}"
        );
        sheet_version = engine.state().characters[&character].version;
    }

    // Dice rolls: RNG + chat message + persistence.
    let mut rolls = Vec::new();
    for _ in 0..SAMPLES {
        clock += 1;
        let envelope = IntentEnvelope {
            intent_id: IntentId::new_v7(),
            expected_version: None,
            ts_ms: clock,
            intent: Intent::RollDice {
                id: ChatMessageId::new_v7(),
                expression: "2d6 + 1".into(),
                hidden: false,
                character_id: None,
                roll: None,
            },
        };
        let started = Instant::now();
        let (outcome, _) = engine.submit(&envelope, &actor, clock).await?;
        rolls.push(started.elapsed());
        anyhow::ensure!(
            matches!(outcome, IntentOutcome::Accepted { .. }),
            "{outcome:?}"
        );
    }

    for (name, samples) in [("move_token", &moves), ("update_character", &edits)] {
        print_row(shape.label, name, samples);
    }
    print_row(shape.label, "roll_dice", &rolls);

    // Traffic: one token move as the authority commits it and as a player receives it.
    let before = engine.state().clone();
    clock += 1;
    let move_envelope = IntentEnvelope {
        intent_id: IntentId::new_v7(),
        expected_version: None,
        ts_ms: clock,
        intent: Intent::MoveToken {
            id: token,
            x: 1120,
            y: 1200,
        },
    };
    let (_, batch) = engine.submit(&move_envelope, &actor, clock).await?;
    let batch = batch.context("a move commits a batch")?;
    let full_bytes = serde_json::to_vec(&batch)?.len();
    let player_bytes = match filter_for_players(&before, engine.state(), &batch) {
        ForPlayers::Batch(filtered) => serde_json::to_vec(&filtered)?.len(),
        ForPlayers::Resnapshot => serde_json::to_vec(&engine.snapshot())?.len(),
    };
    println!(
        "| {} | token move payload (authority / player) | {} B / {} B |",
        shape.label, full_bytes, player_bytes
    );

    // Snapshots: what connecting and resyncing costs.
    let started = Instant::now();
    let full = engine.snapshot();
    let full_ms = started.elapsed().as_secs_f64() * 1000.0;
    let full_bytes = serde_json::to_vec(&full)?.len();
    let started = Instant::now();
    let personal = snapshot_for_user(
        Role::Player,
        player,
        engine.state(),
        engine.last_seq(),
        engine.recent_chat(),
    );
    let personal_ms = started.elapsed().as_secs_f64() * 1000.0;
    let personal_bytes = serde_json::to_vec(&personal)?.len();
    println!(
        "| {} | snapshot (full, {} tokens) | {:.2} ms, {} KiB |",
        shape.label,
        full.tokens.len(),
        full_ms,
        full_bytes / 1024
    );
    println!(
        "| {} | snapshot (player) | {:.2} ms, {} KiB |",
        shape.label,
        personal_ms,
        personal_bytes / 1024
    );

    // Resync (ADR-0053): what a client that missed the last N events would receive as a snapshot
    // versus as a replayed delta (the sum of the missed events as JSON).
    let last = engine.last_seq();
    let mut row = format!("| {} | resync payload (gap) |", shape.label);
    for gap in [10_i64, 100, 1000] {
        let from = Seq(last.0.saturating_sub(gap).max(0));
        let missed = engine.db().events_between(from, last).await?;
        let delta_bytes: usize = missed
            .iter()
            .map(|stored| {
                serde_json::to_vec(&stored.event)
                    .map(|b| b.len())
                    .unwrap_or(0)
            })
            .sum();
        row.push_str(&format!(" {gap}: {} KiB;", delta_bytes / 1024));
    }
    println!("{row} snapshot: {} KiB |", full_bytes / 1024);

    // Export: checkpoint, copy the file, archive it — what the GM waits for.
    let started = Instant::now();
    engine.checkpoint().await?;
    let copy = dir.path().join("export.sqlite");
    std::fs::copy(&path, &copy)?;
    let assets = dir.path().join("assets");
    std::fs::create_dir_all(&assets)?;
    let manifest = ArchiveManifest {
        format: dnd_db::ARCHIVE_FORMAT,
        name: "Perf".into(),
        exported_at_ms: clock,
        generator: "perf-harness".into(),
    };
    let out = dir.path().join("campaign.zip");
    let stats = dnd_db::write_archive(&out, &copy, &assets, &manifest, false)?;
    println!(
        "| {} | export (checkpoint+copy+zip) | {:.1} ms, {} KiB |",
        shape.label,
        started.elapsed().as_secs_f64() * 1000.0,
        stats.bytes / 1024
    );
    println!();

    println!(
        "| {} | peak RSS (process) | {} MiB |",
        shape.label,
        peak_rss_mib()
    );
    println!();

    engine.close().await?;
    Ok(())
}

fn print_row(campaign: &str, command: &str, samples: &[std::time::Duration]) {
    let mut micros: Vec<u128> = samples.iter().map(|d| d.as_micros()).collect();
    micros.sort_unstable();
    let at = |q: f64| -> f64 {
        let index = ((micros.len() - 1) as f64 * q).round() as usize;
        micros[index] as f64 / 1000.0
    };
    println!(
        "| {campaign} | {command} | {:.2} | {:.2} | {:.2} |",
        at(0.5),
        at(0.95),
        micros.last().map_or(0.0, |m| *m as f64 / 1000.0)
    );
}

/// Peak resident memory of this process, in MiB (Linux `/proc/self/status`).
fn peak_rss_mib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                let rest = line.strip_prefix("VmHWM:")?;
                let kib: u64 = rest.split_whitespace().next()?.parse().ok()?;
                Some(kib / 1024)
            })
        })
        .unwrap_or(0)
}
