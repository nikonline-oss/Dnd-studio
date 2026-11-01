#![allow(clippy::unwrap_used)]
use dnd_db::{CAMPAIGN_MIGRATOR, CampaignDb};

#[tokio::test]
async fn legacy_chat_rows_keep_their_text_and_default_to_no_associations() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.sqlite");
    let opts = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true);
    let pool = sqlx::SqlitePool::connect_with(opts).await.unwrap();
    CAMPAIGN_MIGRATOR.run_to(22, &pool).await.unwrap();
    sqlx::query("INSERT INTO campaign_meta (key,value) VALUES ('campaign.name','Legacy')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO chat_messages (id,kind,body,created_at,sender_name,visibility) VALUES ('0190a000-0000-7000-8000-000000000001','text','Old message',1,'Old sender','all')").execute(&pool).await.unwrap();
    pool.close().await;
    let (db, report) = CampaignDb::open(&path, 1000).await.unwrap();
    assert!(report.pre_migration_backup.is_some());
    let messages = db.chat_history(None, 10).await.unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].body, "Old message");
    assert!(messages[0].related_characters.is_empty());
}
