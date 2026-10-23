//! Campaign archives: a full round trip, and archives that must be refused (CLAUDE.md §9).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::io::Write;
use std::path::Path;

use dnd_db::{
    ArchiveLimits, ArchiveManifest, BatchRecord, CampaignDb, DbError, LocalUser, read_archive,
    write_archive,
};
use dnd_protocol::arbitrary::v7;
use dnd_protocol::*;
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn manifest() -> ArchiveManifest {
    ArchiveManifest {
        format: 1,
        name: "Lost Mine".into(),
        exported_at_ms: 42,
        generator: "test".into(),
    }
}

async fn campaign(dir: &Path) -> CampaignDb {
    let gm = LocalUser {
        id: UserId(v7(1)),
        name: "GM".into(),
        role: Role::Gm,
    };
    let db = CampaignDb::create(&dir.join("campaign.sqlite"), "Lost Mine", &gm, 0)
        .await
        .unwrap();
    db.project(&[BatchRecord {
        batch: EventBatch {
            seq: Seq(1),
            caused_by: None,
            events: vec![Event::MapCreated {
                map: MapDto {
                    id: MapId(v7(0x10)),
                    name: "Cave".into(),
                    background_asset_id: None,
                    width: 100,
                    height: 100,
                    grid: Grid::default(),
                    visible_to_players: true,
                    version: Version(1),
                },
            }],
        },
        intent_hash: None,
        actor: None,
        at_ms: 1,
    }])
    .await
    .unwrap();
    db
}

/// Writes a ZIP with the given (name, bytes) entries, deflated.
fn zip_with(path: &Path, entries: &[(&str, &[u8])]) {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, bytes) in entries {
        zip.start_file(*name, SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}

fn manifest_json() -> Vec<u8> {
    serde_json::to_vec(&manifest()).unwrap()
}

fn refused(archive: &Path, target: &Path, limits: ArchiveLimits) -> String {
    match read_archive(archive, target, limits) {
        Err(DbError::InvalidArchive { reason }) => {
            assert!(!target.exists(), "a refused archive leaves nothing behind");
            reason
        }
        other => panic!("expected InvalidArchive, got {other:?}"),
    }
}

#[tokio::test]
async fn export_and_import_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir_all(source.join("assets/originals")).unwrap();
    let db = campaign(&source).await;
    let served = b"webp bytes".as_slice();
    let original = b"png bytes".as_slice();
    std::fs::write(source.join(format!("assets/{}.webp", sha(served))), served).unwrap();
    std::fs::write(
        source.join(format!("assets/originals/{}.png", sha(original))),
        original,
    )
    .unwrap();
    // Stray files in the assets folder are not exported.
    std::fs::write(source.join("assets/notes.txt"), b"x").unwrap();

    let copy = dir.path().join("copy.sqlite");
    db.copy_to(&copy).await.unwrap();
    assert!(matches!(
        db.copy_to(&copy).await,
        Err(DbError::AlreadyExists(_))
    ));
    let (expected, _) = db.load_state().await.unwrap();

    for include_originals in [true, false] {
        let archive = dir.path().join(format!("export-{include_originals}.zip"));
        let written = write_archive(
            &archive,
            &copy,
            &source.join("assets"),
            &manifest(),
            include_originals,
        )
        .unwrap();
        assert_eq!(written.files, if include_originals { 4 } else { 3 });

        let target = dir.path().join(format!("imported-{include_originals}"));
        let (read_manifest, stats) =
            read_archive(&archive, &target, ArchiveLimits::DEFAULT).unwrap();
        assert_eq!(read_manifest, manifest());
        assert_eq!(stats, written);
        assert!(target.join(format!("assets/{}.webp", sha(served))).exists());
        assert_eq!(
            target
                .join(format!("assets/originals/{}.png", sha(original)))
                .exists(),
            include_originals
        );
        assert!(!target.join("assets/notes.txt").exists());

        let (imported, _) = CampaignDb::open(&target.join("campaign.sqlite"), 2)
            .await
            .unwrap();
        assert_eq!(imported.load_state().await.unwrap().0, expected);
        imported.close().await.unwrap();

        assert!(matches!(
            read_archive(&archive, &target, ArchiveLimits::DEFAULT),
            Err(DbError::AlreadyExists(_))
        ));
    }
    db.close().await.unwrap();
}

#[test]
fn paths_outside_the_whitelist_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let webp = b"x".as_slice();
    let name = format!("assets/{}.webp", sha(webp));
    for evil in [
        "../evil.webp".to_owned(),
        format!("assets/../../{}.webp", sha(webp)),
        format!("/assets/{}.webp", sha(webp)),
        format!("assets\\{}.webp", sha(webp)),
        "assets/originals/../../campaign.sqlite".to_owned(),
        "readme.txt".to_owned(),
        format!("assets/{}.exe", sha(webp)),
    ] {
        let archive = dir.path().join("evil.zip");
        zip_with(
            &archive,
            &[
                ("manifest.json", &manifest_json()),
                ("campaign.sqlite", b"db"),
                (&name, webp),
                (&evil, webp),
            ],
        );
        let reason = refused(&archive, &dir.path().join("out"), ArchiveLimits::DEFAULT);
        assert!(reason.contains("unexpected"), "{evil}: {reason}");
        assert!(!dir.path().join("evil.webp").exists());
    }
}

#[test]
fn assets_must_match_their_hash() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("a.zip");
    zip_with(
        &archive,
        &[
            ("manifest.json", &manifest_json()),
            ("campaign.sqlite", b"db"),
            (&format!("assets/{}.webp", sha(b"other")), b"x"),
        ],
    );
    let reason = refused(&archive, &dir.path().join("out"), ArchiveLimits::DEFAULT);
    assert!(reason.contains("hash"), "{reason}");
}

#[test]
fn sizes_are_counted_on_what_is_read() {
    let dir = tempfile::tempdir().unwrap();
    // 1 MB of zeros deflates to about a kilobyte: a small archive, a big file.
    let zeros = vec![0u8; 1024 * 1024];
    let archive = dir.path().join("bomb.zip");
    zip_with(
        &archive,
        &[
            ("manifest.json", &manifest_json()),
            ("campaign.sqlite", &zeros),
        ],
    );
    assert!(std::fs::metadata(&archive).unwrap().len() < 16 * 1024);
    let small = ArchiveLimits {
        max_entries: 10,
        max_file_bytes: 64 * 1024,
        max_total_bytes: 1024 * 1024 * 1024,
    };
    let reason = refused(&archive, &dir.path().join("out"), small);
    assert!(reason.contains("too large"), "{reason}");

    let total = ArchiveLimits {
        max_entries: 10,
        max_file_bytes: 2 * 1024 * 1024,
        max_total_bytes: 512 * 1024,
    };
    refused(&archive, &dir.path().join("out"), total);

    let few = ArchiveLimits {
        max_entries: 1,
        ..ArchiveLimits::DEFAULT
    };
    let reason = refused(&archive, &dir.path().join("out"), few);
    assert!(reason.contains("more than"), "{reason}");
}

/// Archive entries: (name, bytes).
type Entries = Vec<(&'static str, Vec<u8>)>;

#[test]
fn manifest_and_database_are_required() {
    let dir = tempfile::tempdir().unwrap();
    let cases: [(&str, Entries); 4] = [
        ("manifest", vec![("campaign.sqlite", b"db".to_vec())]),
        ("campaign.sqlite", vec![("manifest.json", manifest_json())]),
        (
            "format",
            vec![
                (
                    "manifest.json",
                    serde_json::to_vec(&ArchiveManifest {
                        format: 99,
                        ..manifest()
                    })
                    .unwrap(),
                ),
                ("campaign.sqlite", b"db".to_vec()),
            ],
        ),
        (
            "manifest.json",
            vec![
                ("manifest.json", b"not json".to_vec()),
                ("campaign.sqlite", b"db".to_vec()),
            ],
        ),
    ];
    for (expected, entries) in cases {
        let archive = dir.path().join("m.zip");
        let entries: Vec<(&str, &[u8])> = entries.iter().map(|(n, b)| (*n, b.as_slice())).collect();
        zip_with(&archive, &entries);
        let reason = refused(&archive, &dir.path().join("out"), ArchiveLimits::DEFAULT);
        assert!(reason.contains(expected), "{expected}: {reason}");
    }

    std::fs::write(dir.path().join("junk.zip"), b"not a zip").unwrap();
    refused(
        &dir.path().join("junk.zip"),
        &dir.path().join("out"),
        ArchiveLimits::DEFAULT,
    );
}
