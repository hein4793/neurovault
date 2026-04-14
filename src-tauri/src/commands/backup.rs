use crate::db::BrainDb;
use crate::error::BrainError;
use crate::backup::BackupInfo;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn create_backup(db: State<'_, Arc<BrainDb>>) -> Result<BackupInfo, BrainError> {
    crate::backup::create_backup(&db).await
}

#[tauri::command]
pub async fn list_backups(db: State<'_, Arc<BrainDb>>) -> Result<Vec<BackupInfo>, BrainError> {
    crate::backup::list_backups(&db).await
}

#[tauri::command]
pub async fn restore_backup(
    db: State<'_, Arc<BrainDb>>,
    path: String,
) -> Result<(u64, u64), BrainError> {
    crate::backup::restore_backup(&db, &path).await
}

#[tauri::command]
pub async fn export_json(
    db: State<'_, Arc<BrainDb>>,
    path: String,
) -> Result<u64, BrainError> {
    crate::export::export_json(&db, &path).await
}

#[tauri::command]
pub async fn export_markdown(
    db: State<'_, Arc<BrainDb>>,
    dir: String,
) -> Result<u64, BrainError> {
    crate::export::export_markdown(&db, &dir).await
}

#[tauri::command]
pub async fn export_csv(
    db: State<'_, Arc<BrainDb>>,
    path: String,
) -> Result<u64, BrainError> {
    crate::export::export_csv(&db, &path).await
}

#[tauri::command]
pub async fn generate_training_dataset(
    db: State<'_, Arc<BrainDb>>,
    format: String,
    path: String,
) -> Result<u64, BrainError> {
    crate::export::training::generate_training_dataset(&db, &format, &path).await
}

/// Phase 2.3 — Personalized Q&A export. Pulls decisions, user_cognition,
/// thinking nodes, and summary clusters into OpenAI fine-tune format
/// pairs that capture how the user specifically works.
#[tauri::command]
pub async fn export_personal_training(
    db: State<'_, Arc<BrainDb>>,
    path: Option<String>,
) -> Result<u64, BrainError> {
    let default_path = db.config.export_dir().join("training-personal.jsonl");
    let p = path.unwrap_or_else(|| default_path.to_string_lossy().to_string());
    crate::export::training::export_personal_training(&db, &p).await
}

/// Phase 3.5 — list cold archives. Returns metadata for every cold-storage
/// archive that's ever been created, newest first. Used by the Backup panel.
#[tauri::command]
pub async fn list_cold_archives(
    db: State<'_, Arc<BrainDb>>,
) -> Result<Vec<crate::cold_storage::ColdArchiveEntry>, BrainError> {
    crate::cold_storage::list_archives(&db)
        .await
        .map_err(BrainError::Internal)
}

/// Phase 3.5 — Re-import a cold archive back into SurrealDB. Recovery path
/// for nodes that were archived but should be re-activated. Idempotent.
#[tauri::command]
pub async fn import_cold_archive(
    db: State<'_, Arc<BrainDb>>,
    path: String,
) -> Result<u64, BrainError> {
    crate::cold_storage::import_archive(&db, &path)
        .await
        .map_err(BrainError::Internal)
}

/// Phase 3.5 — Trigger one cold-storage archival pass on demand. Useful
/// for testing or when you've just promoted a batch of nodes to cold and
/// don't want to wait for the weekly loop.
#[tauri::command]
pub async fn run_cold_storage_pass(
    db: State<'_, Arc<BrainDb>>,
) -> Result<(u64, u64), BrainError> {
    let stats = crate::cold_storage::run_one_pass(&db)
        .await
        .map_err(BrainError::Internal)?;
    Ok((stats.archived, stats.bytes_written))
}

/// Phase 4.3 — Compute the safety token for a cold archive. The user must
/// pass this exact token to `purge_cold_archive` to confirm the destructive
/// delete.
#[tauri::command]
pub async fn cold_archive_token(
    _db: State<'_, Arc<BrainDb>>,
    archive_path: String,
) -> Result<String, BrainError> {
    crate::cold_storage::archive_token(&archive_path).map_err(BrainError::Internal)
}

/// Phase 4.3 — Permanently delete the rows in a cold archive from
/// SurrealDB. **Destructive.** Requires `confirm_token` from
/// `cold_archive_token`. Returns the number of rows deleted.
#[tauri::command]
pub async fn purge_cold_archive(
    db: State<'_, Arc<BrainDb>>,
    archive_path: String,
    confirm_token: String,
) -> Result<u64, BrainError> {
    crate::cold_storage::purge_archive(&db, &archive_path, &confirm_token)
        .await
        .map_err(BrainError::Internal)
}

/// Phase 2.5 follow-up — Run one cold-storage pass writing to compressed
/// Parquet instead of JSONL. Requires the `parquet-storage` Cargo feature
/// to be enabled, otherwise returns a clear error pointing to the rebuild
/// command.
#[tauri::command]
pub async fn run_cold_storage_pass_parquet(
    db: State<'_, Arc<BrainDb>>,
) -> Result<(u64, u64), BrainError> {
    let stats = crate::cold_storage_parquet::run_one_pass_parquet(&db)
        .await
        .map_err(BrainError::Internal)?;
    Ok((stats.archived, stats.bytes_written))
}

/// Phase 2.3 / 3.2 — Trigger an actual fine-tune run for the given prepared
/// timestamp. Spawns the prepared Python script as a subprocess. Long-running
/// (30 min to several hours). Updates fine_tune_run status as it progresses.
#[tauri::command]
pub async fn run_finetune_now(
    db: State<'_, Arc<BrainDb>>,
    timestamp: String,
) -> Result<String, BrainError> {
    crate::finetune::run_finetune(&db, &timestamp)
        .await
        .map_err(BrainError::Internal)
}
