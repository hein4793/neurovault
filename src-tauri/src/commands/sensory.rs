//! Phase Omega Part V — Sensory Expansion Tauri commands
//!
//! Exposes visual intelligence, audio intelligence, and data stream
//! capabilities to the frontend via IPC:
//!
//! - `analyze_image` — analyze an image file with vision LLM
//! - `analyze_screenshot` — analyze a screenshot
//! - `ingest_diagram` — extract entities from an architecture diagram
//! - `transcribe_audio` — transcribe audio via Whisper endpoint
//! - `ingest_voice_note` — full pipeline: transcribe + extract insights + create nodes
//! - `add_data_stream` — register a new RSS/API data stream
//! - `get_data_streams` — list all registered streams
//! - `poll_streams_now` — immediately poll all due streams

use crate::audio::{self, Transcription};
use crate::data_streams::{self, DataStream};
use crate::db::BrainDb;
use crate::error::BrainError;
use crate::visual::{self, VisualAnalysis};
use std::sync::Arc;
use tauri::State;

// =========================================================================
// VISUAL INTELLIGENCE
// =========================================================================

#[tauri::command]
pub async fn analyze_image(
    db: State<'_, Arc<BrainDb>>,
    image_path: String,
) -> Result<VisualAnalysis, BrainError> {
    visual::analyze_image(&db, &image_path).await
}

#[tauri::command]
pub async fn analyze_screenshot(
    db: State<'_, Arc<BrainDb>>,
    screenshot_path: String,
) -> Result<VisualAnalysis, BrainError> {
    visual::analyze_screenshot(&db, &screenshot_path).await
}

#[tauri::command]
pub async fn ingest_diagram(
    db: State<'_, Arc<BrainDb>>,
    image_path: String,
) -> Result<VisualAnalysis, BrainError> {
    visual::ingest_diagram(&db, &image_path).await
}

// =========================================================================
// AUDIO INTELLIGENCE
// =========================================================================

#[tauri::command]
pub async fn transcribe_audio(
    db: State<'_, Arc<BrainDb>>,
    audio_path: String,
) -> Result<Transcription, BrainError> {
    audio::transcribe_audio(&db, &audio_path).await
}

#[tauri::command]
pub async fn ingest_voice_note(
    db: State<'_, Arc<BrainDb>>,
    audio_path: String,
) -> Result<Transcription, BrainError> {
    audio::ingest_voice_note(&db, &audio_path).await
}

// =========================================================================
// DATA STREAMS
// =========================================================================

#[tauri::command]
pub async fn add_data_stream(
    db: State<'_, Arc<BrainDb>>,
    name: String,
    stream_type: String,
    url: String,
    interval: u64,
) -> Result<DataStream, BrainError> {
    data_streams::add_stream(&db, name, stream_type, url, interval).await
}

#[tauri::command]
pub async fn get_data_streams(
    db: State<'_, Arc<BrainDb>>,
) -> Result<Vec<DataStream>, BrainError> {
    data_streams::get_streams(&db).await
}

#[tauri::command]
pub async fn poll_streams_now(
    db: State<'_, Arc<BrainDb>>,
) -> Result<u64, BrainError> {
    data_streams::poll_all_streams(&db).await
}
