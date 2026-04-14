use crate::db::BrainDb;
use crate::error::BrainError;
use crate::user::profile::UserProfile;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn get_user_profile(
    db: State<'_, Arc<BrainDb>>,
) -> Result<UserProfile, BrainError> {
    Ok(crate::user::profile::load_profile(&db).await)
}

#[tauri::command]
pub async fn synthesize_user_profile(
    db: State<'_, Arc<BrainDb>>,
) -> Result<String, BrainError> {
    crate::user::profile::synthesize_profile(&db).await
}
