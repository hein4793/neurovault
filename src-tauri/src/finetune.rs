//! Fine-Tuning Automation — Phase 3.2 of the master plan.
//!
//! The brain doesn't actually run LoRA fine-tuning itself (that needs
//! Python + GPU compute outside the Tauri process), but it does all the
//! prep work: monitors the personal training dataset, detects when it
//! has grown enough to warrant a fresh fine-tune, generates an updated
//! export, writes a ready-to-run training script, and emits a brain
//! event so the UI can prompt the user.
//!
//! ## What this module does
//!
//! 1. **Tracks dataset growth.** Reads the size of `training-personal.jsonl`
//!    and compares against the size at the last fine-tune (stored in
//!    the `fine_tune_run` table).
//! 2. **Decides when to retrain.** A fresh fine-tune is recommended when:
//!    - The dataset has at least 200 entries (seed threshold), AND
//!    - It has grown by >=30% since the last successful fine-tune, OR
//!    - 30+ days have passed since the last fine-tune
//! 3. **Generates a fresh export** by calling the existing Phase 2.3
//!    `export_personal_training`.
//! 4. **Writes an unsloth-compatible training script** to
//!    `~/.neurovault/finetune/run-<timestamp>.py` so the user can
//!    execute it with one command in their Python env.
//! 5. **Logs the run** to `fine_tune_run` table so the next cycle has
//!    a baseline to compare against.
//!
//! ## What this module does NOT do
//!
//! - It does **not** spawn a Python process or run the actual training
//!   (that's outside the Tauri sandbox boundary and would need a separate
//!   compute environment).
//! - It does **not** download base models from HuggingFace.
//! - It does **not** evaluate the resulting model — that's the user's call
//!   when they swap the new GGUF into Ollama.
//!
//! ## Schedule
//!
//! Runs as a background task spawned at startup, with a 1-hour warm-up
//! delay, then once every 24 hours. The check itself is O(1) — just a
//! file size read + a DB row lookup — so the cost is trivial.

use crate::db::BrainDb;
use rusqlite::params;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Spawn the fine-tune scheduler forever. Called from lib.rs setup().
pub async fn run_finetune_scheduler(db: Arc<BrainDb>) {
    // Wait 1 hour after startup so the rest of the system is steady-state
    // and the personal training export has had a chance to run at least
    // once via the daily autonomy task.
    tokio::time::sleep(Duration::from_secs(3600)).await;
    log::info!("Fine-tune scheduler started — daily check, recommends retrain when dataset grows >=30% or 30+ days old");

    loop {
        match check_and_prep(&db).await {
            Ok(action) => log::info!("Fine-tune scheduler: {}", action),
            Err(e) => log::warn!("Fine-tune scheduler error: {}", e),
        }
        tokio::time::sleep(Duration::from_secs(86_400)).await; // 24 hours
    }
}

/// One scheduler iteration. Returns a one-line summary of what happened.
pub async fn check_and_prep(db: &BrainDb) -> Result<String, String> {
    let export_dir = db.config.export_dir();
    let dataset_path = export_dir.join("training-personal.jsonl");

    // Make sure we have a current export — if the file doesn't exist or is
    // older than 24h, regenerate it now (don't wait for the daily autonomy task).
    let needs_export = !dataset_path.exists()
        || file_age_hours(&dataset_path).unwrap_or(999.0) > 24.0;
    if needs_export {
        log::info!("Fine-tune scheduler: regenerating personal training export...");
        let count: u64 = crate::export::training::export_personal_training(
            db,
            &dataset_path.to_string_lossy(),
        )
        .await
        .map_err(|e| format!("export failed: {}", e))?;
        log::info!("Fine-tune scheduler: exported {} pairs", count);
    }

    // Read current dataset size + entry count
    let (current_size, current_entries) = inspect_dataset(&dataset_path)
        .ok_or_else(|| "could not inspect dataset".to_string())?;

    if current_entries < 200 {
        return Ok(format!(
            "dataset too small ({} entries, need >=200) — skipping",
            current_entries
        ));
    }

    // Look up the last successful fine-tune from the DB
    let last = load_last_run(db).await;

    let should_retrain = match &last {
        None => {
            // No previous run — recommend the first one
            log::info!("Fine-tune scheduler: no previous run found, recommending first fine-tune");
            true
        }
        Some(last) => {
            let growth_ratio = if last.dataset_entries > 0 {
                current_entries as f64 / last.dataset_entries as f64
            } else {
                999.0
            };
            let days_since = days_since_iso(&last.completed_at).unwrap_or(999.0);
            let grown_enough = growth_ratio >= 1.30;
            let stale = days_since >= 30.0;
            log::debug!(
                "Fine-tune scheduler: growth={:.2}x, days_since={:.1} (need >=1.30x or >=30 days)",
                growth_ratio, days_since
            );
            grown_enough || stale
        }
    };

    if !should_retrain {
        return Ok(format!(
            "{} entries ({} bytes) — no retrain needed yet",
            current_entries, current_size
        ));
    }

    // Generate a ready-to-run training script
    let finetune_dir = db.config.data_dir.join("finetune");
    std::fs::create_dir_all(&finetune_dir).map_err(|e| format!("mkdir: {}", e))?;
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let script_path = finetune_dir.join(format!("run-{}.py", timestamp));
    let modelfile_path = finetune_dir.join(format!("Modelfile-{}", timestamp));
    let script_body = render_unsloth_script(&dataset_path, &finetune_dir, &timestamp);
    let modelfile_body = render_modelfile(&timestamp);
    std::fs::write(&script_path, script_body)
        .map_err(|e| format!("write script: {}", e))?;
    std::fs::write(&modelfile_path, modelfile_body)
        .map_err(|e| format!("write modelfile: {}", e))?;

    // Record the run as "prepared" — the user marks it "completed" later
    // (or we update it from a separate command after the GGUF is built).
    record_prepared_run(
        db,
        &timestamp,
        current_size as u64,
        current_entries as u64,
        &script_path,
    )
    .await;

    log::info!(
        "Fine-tune scheduler: prepared run {} -> {}",
        timestamp,
        script_path.display()
    );

    Ok(format!(
        "Prepared fine-tune run {} ({} entries, {} KB) -> {}",
        timestamp,
        current_entries,
        current_size / 1024,
        script_path.display()
    ))
}

// =========================================================================
// Dataset inspection
// =========================================================================

fn inspect_dataset(path: &PathBuf) -> Option<(u64, u64)> {
    let metadata = std::fs::metadata(path).ok()?;
    let size = metadata.len();
    let content = std::fs::read_to_string(path).ok()?;
    let entries = content.lines().filter(|l| !l.trim().is_empty()).count() as u64;
    Some((size, entries))
}

fn file_age_hours(path: &PathBuf) -> Option<f64> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let elapsed = modified.elapsed().ok()?;
    Some(elapsed.as_secs_f64() / 3600.0)
}

fn days_since_iso(iso: &str) -> Option<f64> {
    let parsed = chrono::DateTime::parse_from_rfc3339(iso).ok()?;
    let now = chrono::Utc::now();
    let elapsed = now.signed_duration_since(parsed.with_timezone(&chrono::Utc));
    Some(elapsed.num_seconds() as f64 / 86400.0)
}

// =========================================================================
// fine_tune_run table I/O
// =========================================================================

#[allow(dead_code)]
struct FineTuneRunRow {
    timestamp: String,
    dataset_size_bytes: u64,
    dataset_entries: u64,
    completed_at: String,
}

async fn load_last_run(db: &BrainDb) -> Option<FineTuneRunRow> {
    db.with_conn(|conn| -> Result<Option<FineTuneRunRow>, crate::error::BrainError> {
        let mut stmt = conn.prepare(
            "SELECT timestamp, dataset_size_bytes, dataset_entries, completed_at \
             FROM fine_tune_run WHERE completed_at != '' \
             ORDER BY completed_at DESC LIMIT 1"
        ).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let mut rows = stmt.query_map([], |row| {
            Ok(FineTuneRunRow {
                timestamp: row.get(0)?,
                dataset_size_bytes: row.get(1)?,
                dataset_entries: row.get(2)?,
                completed_at: row.get(3)?,
            })
        }).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        match rows.next() {
            Some(Ok(r)) => Ok(Some(r)),
            _ => Ok(None),
        }
    }).await.ok().flatten()
}

/// Phase 2.3 / 3.2 — Spawn the prepared training script as a subprocess
/// and capture its output.
///
/// Requires the user to have Python + unsloth installed in the system
/// path. Gracefully fails with a helpful error if not. Updates the
/// `fine_tune_run` row with `status='running'` -> `'completed'` or
/// `'failed'` plus the captured stderr/stdout.
///
/// **Long-running:** training a 14B LoRA on personal data takes 30 min
/// to several hours depending on dataset size. The function awaits the
/// full subprocess; callers should run it as its own tokio task.
pub async fn run_finetune(db: &BrainDb, timestamp: &str) -> Result<String, String> {
    use tokio::process::Command;

    // Look up the prepared run
    let ts = timestamp.to_string();
    let row = db.with_conn(move |conn| -> Result<(String, String, String), crate::error::BrainError> {
        let mut stmt = conn.prepare(
            "SELECT timestamp, script_path, status FROM fine_tune_run WHERE timestamp = ?1 LIMIT 1"
        ).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let mut rows = stmt.query_map(params![ts], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        }).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        match rows.next() {
            Some(Ok(r)) => Ok(r),
            _ => Err(crate::error::BrainError::NotFound(
                format!("no prepared fine-tune run with timestamp '{}'", ts)
            )),
        }
    }).await.map_err(|e| e.to_string())?;

    let (_ts, script_path, _status) = row;

    if !std::path::Path::new(&script_path).exists() {
        return Err(format!("script file missing: {}", script_path));
    }

    // Mark as running
    let now = chrono::Utc::now().to_rfc3339();
    let ts_for_update = timestamp.to_string();
    let now_clone = now.clone();
    let _ = db.with_conn(move |conn| -> Result<(), crate::error::BrainError> {
        conn.execute(
            "UPDATE fine_tune_run SET status = 'running', started_at = ?1 WHERE timestamp = ?2",
            params![now_clone, ts_for_update],
        ).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        Ok(())
    }).await;

    log::info!("Fine-tune {}: starting subprocess `python {}`", timestamp, script_path);

    // Spawn python — try `python` first, then `python3`. Inherit env so
    // PYTHONPATH / virtual env / CUDA paths come through.
    let python_cmd = if which("python") {
        "python"
    } else if which("python3") {
        "python3"
    } else {
        let msg = "neither 'python' nor 'python3' found in PATH — install Python and unsloth first";
        let ts_for_err = timestamp.to_string();
        let msg_clone = msg.to_string();
        let _ = db.with_conn(move |conn| -> Result<(), crate::error::BrainError> {
            conn.execute(
                "UPDATE fine_tune_run SET status = 'failed', error = ?1 WHERE timestamp = ?2",
                params![msg_clone, ts_for_err],
            ).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
            Ok(())
        }).await;
        return Err(msg.to_string());
    };

    let output = Command::new(python_cmd)
        .arg(&script_path)
        .output()
        .await;

    let now = chrono::Utc::now().to_rfc3339();
    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let exit_code = out.status.code().unwrap_or(-1);
            let success = out.status.success();

            // Truncate captured output to keep DB row reasonable
            let stdout_trim: String = stdout.chars().rev().take(4000).collect::<String>().chars().rev().collect();
            let stderr_trim: String = stderr.chars().rev().take(4000).collect::<String>().chars().rev().collect();

            let new_status = if success { "completed" } else { "failed" };
            let ts_for_update = timestamp.to_string();
            let now_clone = now.clone();
            let status_clone = new_status.to_string();
            let stdout_clone = stdout_trim.clone();
            let stderr_clone = stderr_trim.clone();
            let _ = db.with_conn(move |conn| -> Result<(), crate::error::BrainError> {
                conn.execute(
                    "UPDATE fine_tune_run SET \
                     status = ?1, \
                     completed_at = ?2, \
                     exit_code = ?3, \
                     stdout_tail = ?4, \
                     stderr_tail = ?5 \
                     WHERE timestamp = ?6",
                    params![status_clone, now_clone, exit_code as i64, stdout_clone, stderr_clone, ts_for_update],
                ).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
                Ok(())
            }).await;

            if success {
                log::info!("Fine-tune {}: completed (exit 0)", timestamp);
                Ok(format!("Fine-tune {} completed successfully", timestamp))
            } else {
                log::warn!(
                    "Fine-tune {}: failed (exit {}), stderr tail: {}",
                    timestamp, exit_code, stderr_trim
                );
                Err(format!(
                    "Fine-tune subprocess exited {}: {}",
                    exit_code, stderr_trim
                ))
            }
        }
        Err(e) => {
            let msg = format!("subprocess spawn failed: {}", e);
            let ts_for_err = timestamp.to_string();
            let msg_clone = msg.clone();
            let now_clone = now.clone();
            let _ = db.with_conn(move |conn| -> Result<(), crate::error::BrainError> {
                conn.execute(
                    "UPDATE fine_tune_run SET status = 'failed', error = ?1, completed_at = ?2 WHERE timestamp = ?3",
                    params![msg_clone, now_clone, ts_for_err],
                ).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
                Ok(())
            }).await;
            Err(msg)
        }
    }
}

/// Cheap PATH check for an executable name.
fn which(cmd: &str) -> bool {
    if let Ok(path_var) = std::env::var("PATH") {
        let sep = if cfg!(windows) { ';' } else { ':' };
        for dir in path_var.split(sep) {
            let candidate = std::path::Path::new(dir).join(cmd);
            if candidate.is_file() {
                return true;
            }
            // Windows: also try .exe
            if cfg!(windows) {
                let exe = std::path::Path::new(dir).join(format!("{}.exe", cmd));
                if exe.is_file() {
                    return true;
                }
            }
        }
    }
    false
}

async fn record_prepared_run(
    db: &BrainDb,
    timestamp: &str,
    size_bytes: u64,
    entries: u64,
    script_path: &PathBuf,
) {
    let now = chrono::Utc::now().to_rfc3339();
    let id = format!("fine_tune_run:{}", uuid::Uuid::now_v7());
    let ts = timestamp.to_string();
    let sp = script_path.to_string_lossy().to_string();
    let _ = db.with_conn(move |conn| -> Result<(), crate::error::BrainError> {
        conn.execute(
            "INSERT INTO fine_tune_run (id, timestamp, dataset_size_bytes, dataset_entries, \
             status, script_path, prepared_at, completed_at) \
             VALUES (?1, ?2, ?3, ?4, 'prepared', ?5, ?6, '')",
            params![id, ts, size_bytes, entries, sp, now],
        ).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        Ok(())
    }).await;
}

// =========================================================================
// Training script + Modelfile rendering
// =========================================================================

fn render_unsloth_script(dataset_path: &PathBuf, output_dir: &PathBuf, timestamp: &str) -> String {
    let dataset = dataset_path.to_string_lossy();
    let out = output_dir.join(format!("model-{}", timestamp));
    let out_str = out.to_string_lossy();
    let gguf = output_dir.join(format!("model-{}.gguf", timestamp));
    let gguf_str = gguf.to_string_lossy();

    format!(
        r#"#!/usr/bin/env python3
"""
NeuroVault — Fine-tune run {timestamp}
Auto-generated by the brain's fine-tune scheduler.

Prerequisites:
    pip install unsloth datasets transformers trl peft bitsandbytes accelerate

Run:
    python3 {script_name}

This will fine-tune Qwen2.5-Coder-14B on the user's personal training dataset
using LoRA, then export to GGUF for Ollama.
"""

import os
from datasets import load_dataset
from unsloth import FastLanguageModel
from trl import SFTTrainer
from transformers import TrainingArguments

# === Config ===
BASE_MODEL = "unsloth/Qwen2.5-Coder-14B-Instruct-bnb-4bit"
DATASET_PATH = r"{dataset}"
OUTPUT_DIR = r"{out_str}"
GGUF_PATH = r"{gguf_str}"
MAX_SEQ_LENGTH = 4096
EPOCHS = 2  # 1-3 is usually enough for personal data
LORA_R = 16
LORA_ALPHA = 16

# === Load model ===
model, tokenizer = FastLanguageModel.from_pretrained(
    model_name=BASE_MODEL,
    max_seq_length=MAX_SEQ_LENGTH,
    load_in_4bit=True,
)

model = FastLanguageModel.get_peft_model(
    model,
    r=LORA_R,
    lora_alpha=LORA_ALPHA,
    target_modules=["q_proj", "k_proj", "v_proj", "o_proj",
                    "gate_proj", "up_proj", "down_proj"],
    use_gradient_checkpointing="unsloth",
)

# === Load dataset ===
dataset = load_dataset("json", data_files=DATASET_PATH, split="train")
print(f"Dataset loaded: {{len(dataset)}} examples")

# === Format for instruction tuning ===
def format_prompt(example):
    msgs = example["messages"]
    text = tokenizer.apply_chat_template(
        msgs, tokenize=False, add_generation_prompt=False,
    )
    return {{"text": text}}

dataset = dataset.map(format_prompt)

# === Train ===
trainer = SFTTrainer(
    model=model,
    tokenizer=tokenizer,
    train_dataset=dataset,
    dataset_text_field="text",
    max_seq_length=MAX_SEQ_LENGTH,
    args=TrainingArguments(
        per_device_train_batch_size=2,
        gradient_accumulation_steps=4,
        warmup_steps=10,
        num_train_epochs=EPOCHS,
        learning_rate=2e-4,
        fp16=True,
        logging_steps=10,
        optim="adamw_8bit",
        output_dir=OUTPUT_DIR,
        save_strategy="epoch",
    ),
)

trainer.train()

# === Save LoRA adapter ===
model.save_pretrained(OUTPUT_DIR)
tokenizer.save_pretrained(OUTPUT_DIR)

# === Export to GGUF for Ollama ===
model.save_pretrained_gguf(GGUF_PATH, tokenizer, quantization_method="q4_k_m")

print(f"\nFine-tuning complete!")
print(f"LoRA adapter:  {{OUTPUT_DIR}}")
print(f"GGUF for Ollama: {{GGUF_PATH}}")
print(f"\nNext step: load into Ollama")
print(f"  ollama create brain-personal-{timestamp} -f Modelfile-{timestamp}")
print(f"\nThen update ~/.neurovault/settings.json:")
print(f'  "llm_model_fast": "brain-personal-{timestamp}"')
"#,
        timestamp = timestamp,
        dataset = dataset,
        out_str = out_str,
        gguf_str = gguf_str,
        script_name = format!("run-{}.py", timestamp)
    )
}

fn render_modelfile(timestamp: &str) -> String {
    format!(
        r#"# NeuroVault — fine-tune {timestamp}
# Auto-generated by the brain's fine-tune scheduler.
#
# Load this into Ollama with:
#   ollama create brain-personal-{timestamp} -f Modelfile-{timestamp}
#
# Then point the brain at it via ~/.neurovault/settings.json:
#   "llm_model_fast": "brain-personal-{timestamp}"

FROM ./model-{timestamp}.gguf

TEMPLATE """{{{{ if .System }}}}<|im_start|>system
{{{{ .System }}}}<|im_end|>
{{{{ end }}}}{{{{ if .Prompt }}}}<|im_start|>user
{{{{ .Prompt }}}}<|im_end|>
{{{{ end }}}}<|im_start|>assistant
{{{{ .Response }}}}<|im_end|>
"""

PARAMETER stop "<|im_start|>"
PARAMETER stop "<|im_end|>"
PARAMETER temperature 0.3
PARAMETER num_ctx 8192

SYSTEM """You are NeuroVault, a personal AI assistant fine-tuned on the user's knowledge graph. Answer questions using the established patterns, decisions, and preferences extracted from his work."""
"#,
        timestamp = timestamp
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_unsloth_script() {
        let script = render_unsloth_script(
            &PathBuf::from("/tmp/dataset.jsonl"),
            &PathBuf::from("/tmp/finetune"),
            "20260410-150000",
        );
        assert!(script.contains("BASE_MODEL = \"unsloth/Qwen2.5-Coder-14B"));
        assert!(script.contains("/tmp/dataset.jsonl"));
        assert!(script.contains("save_pretrained_gguf"));
    }

    #[test]
    fn renders_modelfile() {
        let mf = render_modelfile("20260410-150000");
        assert!(mf.contains("FROM ./model-20260410-150000.gguf"));
        assert!(mf.contains("PARAMETER temperature"));
    }
}
