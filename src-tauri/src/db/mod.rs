pub mod models;
pub mod queries;

use crate::config::BrainConfig;
use crate::embeddings::hnsw::SharedHnsw;
use crate::error::BrainError;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

pub struct BrainDb {
    conn: Arc<Mutex<Connection>>,
    pub config: BrainConfig,
    pub hnsw: SharedHnsw,
}

impl BrainDb {
    pub async fn init() -> Result<Self, BrainError> {
        let config = BrainConfig::default();
        config.ensure_dirs().map_err(|e| BrainError::Database(e.to_string()))?;

        let db_path = config.sqlite_path();
        let conn = Connection::open(&db_path)
            .map_err(|e| BrainError::Database(format!("Failed to open SQLite at {:?}: {}", db_path, e)))?;

        // WAL mode for crash safety + concurrent readers
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA cache_size = -64000;"
        ).map_err(|e| BrainError::Database(format!("PRAGMA init failed: {}", e)))?;

        Self::init_schema(&conn)?;

        log::info!("SQLite initialized at {:?} (WAL mode)", db_path);

        let conn = Arc::new(Mutex::new(conn));

        Ok(Self {
            conn,
            config,
            hnsw: crate::embeddings::hnsw::shared(),
        })
    }

    fn init_schema(conn: &Connection) -> Result<(), BrainError> {
        conn.execute_batch(
            "
            -- Core node table
            CREATE TABLE IF NOT EXISTS nodes (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                summary TEXT NOT NULL DEFAULT '',
                content_hash TEXT UNIQUE NOT NULL,
                domain TEXT NOT NULL DEFAULT 'general',
                topic TEXT NOT NULL DEFAULT '',
                tags TEXT NOT NULL DEFAULT '[]',
                node_type TEXT NOT NULL DEFAULT 'reference',
                source_type TEXT NOT NULL DEFAULT 'manual',
                source_url TEXT,
                source_file TEXT,
                vault_path TEXT,
                quality_score REAL NOT NULL DEFAULT 0.7,
                decay_score REAL NOT NULL DEFAULT 1.0,
                visual_size REAL NOT NULL DEFAULT 3.0,
                access_count INTEGER NOT NULL DEFAULT 0,
                synthesized_by_brain INTEGER NOT NULL DEFAULT 0,
                cognitive_type TEXT,
                confidence REAL,
                memory_tier TEXT DEFAULT 'hot',
                compression_parent TEXT,
                brain_id TEXT,
                cluster_id TEXT,
                embedding TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                accessed_at TEXT NOT NULL
            );

            -- FTS5 full-text search index
            CREATE VIRTUAL TABLE IF NOT EXISTS nodes_fts USING fts5(
                title, content, summary, domain, topic, tags,
                content='nodes',
                content_rowid='rowid'
            );

            -- FTS5 sync triggers
            CREATE TRIGGER IF NOT EXISTS nodes_ai AFTER INSERT ON nodes BEGIN
                INSERT INTO nodes_fts(rowid, title, content, summary, domain, topic, tags)
                VALUES (new.rowid, new.title, new.content, new.summary, new.domain, new.topic, new.tags);
            END;

            CREATE TRIGGER IF NOT EXISTS nodes_ad AFTER DELETE ON nodes BEGIN
                INSERT INTO nodes_fts(nodes_fts, rowid, title, content, summary, domain, topic, tags)
                VALUES ('delete', old.rowid, old.title, old.content, old.summary, old.domain, old.topic, old.tags);
            END;

            CREATE TRIGGER IF NOT EXISTS nodes_au AFTER UPDATE ON nodes BEGIN
                INSERT INTO nodes_fts(nodes_fts, rowid, title, content, summary, domain, topic, tags)
                VALUES ('delete', old.rowid, old.title, old.content, old.summary, old.domain, old.topic, old.tags);
                INSERT INTO nodes_fts(rowid, title, content, summary, domain, topic, tags)
                VALUES (new.rowid, new.title, new.content, new.summary, new.domain, new.topic, new.tags);
            END;

            -- Edges (graph relationships)
            CREATE TABLE IF NOT EXISTS edges (
                id TEXT PRIMARY KEY,
                source_id TEXT NOT NULL,
                target_id TEXT NOT NULL,
                relation_type TEXT NOT NULL,
                strength REAL NOT NULL DEFAULT 0.5,
                discovered_by TEXT NOT NULL DEFAULT 'user_created',
                evidence TEXT NOT NULL DEFAULT '',
                animated INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL,
                traversal_count INTEGER NOT NULL DEFAULT 0
            );

            -- Embeddings stored as BLOBs
            CREATE TABLE IF NOT EXISTS embeddings (
                node_id TEXT PRIMARY KEY,
                vector BLOB NOT NULL,
                dimension INTEGER NOT NULL
            );

            -- User cognition
            CREATE TABLE IF NOT EXISTS user_cognition (
                id TEXT PRIMARY KEY,
                timestamp TEXT NOT NULL,
                trigger_node_ids TEXT NOT NULL DEFAULT '[]',
                pattern_type TEXT NOT NULL,
                extracted_rule TEXT NOT NULL,
                structured_rule TEXT,
                confidence REAL NOT NULL DEFAULT 0.5,
                times_confirmed INTEGER NOT NULL DEFAULT 1,
                times_contradicted INTEGER NOT NULL DEFAULT 0,
                embedding TEXT,
                linked_to_nodes TEXT NOT NULL DEFAULT '[]'
            );

            -- Autonomy circuit log
            CREATE TABLE IF NOT EXISTS autonomy_circuit_log (
                id TEXT PRIMARY KEY,
                circuit_name TEXT NOT NULL,
                started_at TEXT NOT NULL,
                duration_ms INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'ok',
                result TEXT NOT NULL DEFAULT '',
                details TEXT
            );

            -- Autonomy circuit rotation (singleton)
            CREATE TABLE IF NOT EXISTS autonomy_circuit_rotation (
                id TEXT PRIMARY KEY,
                recent_circuits TEXT NOT NULL DEFAULT '[]',
                updated_at TEXT NOT NULL
            );

            -- Autonomy state
            CREATE TABLE IF NOT EXISTS autonomy_state (
                task_name TEXT PRIMARY KEY,
                last_run_at TEXT NOT NULL,
                last_result TEXT NOT NULL DEFAULT '',
                runs_today INTEGER NOT NULL DEFAULT 0,
                today_date TEXT NOT NULL
            );

            -- Research missions
            CREATE TABLE IF NOT EXISTS research_missions (
                id TEXT PRIMARY KEY,
                topic TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'pending',
                priority INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                completed_at TEXT,
                result TEXT
            );

            -- Sync state
            CREATE TABLE IF NOT EXISTS sync_state (
                file_path TEXT PRIMARY KEY,
                content_hash TEXT NOT NULL,
                last_synced_at TEXT NOT NULL
            );

            -- Learning log
            CREATE TABLE IF NOT EXISTS learning_log (
                id TEXT PRIMARY KEY,
                topic TEXT NOT NULL,
                content TEXT NOT NULL DEFAULT '',
                learned_at TEXT NOT NULL,
                source TEXT NOT NULL DEFAULT 'auto'
            );

            -- User profile
            CREATE TABLE IF NOT EXISTS user_profile (
                id TEXT PRIMARY KEY,
                data TEXT NOT NULL DEFAULT '{}'
            );

            -- User interaction
            CREATE TABLE IF NOT EXISTS user_interaction (
                id TEXT PRIMARY KEY,
                interaction_type TEXT NOT NULL,
                data TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL
            );

            -- Projects
            CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                path TEXT,
                data TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL
            );

            -- Node archive
            CREATE TABLE IF NOT EXISTS node_archive (
                id TEXT PRIMARY KEY,
                node_data TEXT NOT NULL,
                archived_at TEXT NOT NULL
            );

            -- MCP call log
            CREATE TABLE IF NOT EXISTS mcp_call_log (
                id TEXT PRIMARY KEY,
                tool_name TEXT NOT NULL,
                args TEXT NOT NULL DEFAULT '{}',
                result TEXT NOT NULL DEFAULT '',
                called_at TEXT NOT NULL
            );

            -- Compression log
            CREATE TABLE IF NOT EXISTS compression_log (
                id TEXT PRIMARY KEY,
                parent_id TEXT NOT NULL,
                child_ids TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL
            );

            -- Master loop log
            CREATE TABLE IF NOT EXISTS master_loop_log (
                id TEXT PRIMARY KEY,
                phase TEXT NOT NULL,
                result TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL
            );

            -- Memory tier log
            CREATE TABLE IF NOT EXISTS memory_tier_log (
                id TEXT PRIMARY KEY,
                stats TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL
            );

            -- Fine-tune runs
            CREATE TABLE IF NOT EXISTS fine_tune_run (
                id TEXT PRIMARY KEY,
                status TEXT NOT NULL DEFAULT 'pending',
                dataset_size INTEGER NOT NULL DEFAULT 0,
                started_at TEXT NOT NULL,
                completed_at TEXT,
                result TEXT
            );

            -- Cold archive log
            CREATE TABLE IF NOT EXISTS cold_archive_log (
                id TEXT PRIMARY KEY,
                archive_path TEXT NOT NULL,
                node_count INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL
            );

            -- Multi-brain support
            CREATE TABLE IF NOT EXISTS brains (
                id TEXT PRIMARY KEY,
                slug TEXT UNIQUE NOT NULL,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                color TEXT NOT NULL DEFAULT '#00A8FF',
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS active_brain_state (
                id TEXT PRIMARY KEY DEFAULT 'current',
                active_brain_slug TEXT NOT NULL DEFAULT 'main',
                updated_at TEXT NOT NULL
            );

            -- Synapse prune log
            CREATE TABLE IF NOT EXISTS synapse_prune_log (
                id TEXT PRIMARY KEY,
                pruned_count INTEGER NOT NULL DEFAULT 0,
                reason TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL
            );

            -- Phase Omega Part III — World Model
            CREATE TABLE IF NOT EXISTS world_entities (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                entity_type TEXT NOT NULL,
                properties TEXT NOT NULL DEFAULT '{}',
                last_updated TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS causal_links (
                id TEXT PRIMARY KEY,
                cause_id TEXT NOT NULL,
                effect_id TEXT NOT NULL,
                relationship TEXT NOT NULL,
                strength REAL NOT NULL DEFAULT 0.5,
                lag_days INTEGER NOT NULL DEFAULT 0,
                evidence_node_ids TEXT NOT NULL DEFAULT '[]',
                confidence REAL NOT NULL DEFAULT 0.5,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS temporal_patterns (
                id TEXT PRIMARY KEY,
                pattern_type TEXT NOT NULL,
                domain TEXT NOT NULL,
                description TEXT NOT NULL,
                period_days INTEGER,
                confidence REAL NOT NULL DEFAULT 0.5,
                evidence TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS future_predictions (
                id TEXT PRIMARY KEY,
                prediction TEXT NOT NULL,
                confidence REAL NOT NULL DEFAULT 0.5,
                timeframe_days INTEGER NOT NULL,
                evidence_node_ids TEXT NOT NULL DEFAULT '[]',
                causal_chain TEXT NOT NULL DEFAULT '[]',
                validated INTEGER NOT NULL DEFAULT 0,
                invalidated INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                due_at TEXT NOT NULL
            );

            -- Phase Omega IV — Recursive Self-Improvement
            CREATE TABLE IF NOT EXISTS knowledge_rules (
                id TEXT PRIMARY KEY,
                source_node_ids TEXT NOT NULL DEFAULT '[]',
                rule_type TEXT NOT NULL,
                condition TEXT NOT NULL,
                action TEXT NOT NULL,
                confidence REAL NOT NULL DEFAULT 0.5,
                times_applied INTEGER NOT NULL DEFAULT 0,
                times_correct INTEGER NOT NULL DEFAULT 0,
                accuracy REAL NOT NULL DEFAULT 0.0,
                compiled_at TEXT NOT NULL,
                invalidated INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS circuit_performance (
                circuit_name TEXT PRIMARY KEY,
                total_runs INTEGER NOT NULL DEFAULT 0,
                success_runs INTEGER NOT NULL DEFAULT 0,
                avg_duration_ms INTEGER NOT NULL DEFAULT 0,
                nodes_created INTEGER NOT NULL DEFAULT 0,
                edges_created INTEGER NOT NULL DEFAULT 0,
                iq_delta REAL NOT NULL DEFAULT 0.0,
                efficiency REAL NOT NULL DEFAULT 0.0,
                last_computed TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS capabilities (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                proficiency REAL NOT NULL DEFAULT 0.0,
                evidence_count INTEGER NOT NULL DEFAULT 0,
                last_tested TEXT,
                status TEXT NOT NULL DEFAULT 'unknown',
                improvement_plan TEXT,
                updated_at TEXT NOT NULL
            );

            -- Phase Omega IX — Consciousness Layer: Self-Model
            CREATE TABLE IF NOT EXISTS self_model (
                id TEXT PRIMARY KEY DEFAULT 'current',
                identity TEXT NOT NULL DEFAULT 'NeuroVault',
                purpose TEXT NOT NULL DEFAULT 'Make work and businesses better',
                current_iq REAL NOT NULL DEFAULT 0.0,
                iq_trajectory TEXT NOT NULL DEFAULT '[]',
                strongest_areas TEXT NOT NULL DEFAULT '[]',
                weakest_areas TEXT NOT NULL DEFAULT '[]',
                current_bottleneck TEXT NOT NULL DEFAULT '',
                improvement_priorities TEXT NOT NULL DEFAULT '[]',
                active_experiments TEXT NOT NULL DEFAULT '[]',
                recent_discoveries TEXT NOT NULL DEFAULT '[]',
                user_satisfaction REAL NOT NULL DEFAULT 0.5,
                user_current_focus TEXT NOT NULL DEFAULT '',
                total_nodes INTEGER NOT NULL DEFAULT 0,
                total_edges INTEGER NOT NULL DEFAULT 0,
                total_rules INTEGER NOT NULL DEFAULT 0,
                last_updated TEXT NOT NULL DEFAULT ''
            );

            -- Phase Omega IX — Consciousness Layer: Attention Focus
            CREATE TABLE IF NOT EXISTS attention_focus (
                node_id TEXT PRIMARY KEY,
                attention_score REAL NOT NULL,
                reason TEXT NOT NULL DEFAULT '',
                updated_at TEXT NOT NULL
            );

            -- Phase Omega IX — Consciousness Layer: Learning Velocity
            CREATE TABLE IF NOT EXISTS learning_velocity (
                domain TEXT PRIMARY KEY,
                nodes_per_day REAL NOT NULL DEFAULT 0.0,
                quality_trend REAL NOT NULL DEFAULT 0.0,
                last_computed TEXT NOT NULL DEFAULT ''
            );

            -- Phase Omega V — Visual Intelligence
            CREATE TABLE IF NOT EXISTS visual_analysis (
                id TEXT PRIMARY KEY,
                image_path TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                entities TEXT NOT NULL DEFAULT '[]',
                context TEXT NOT NULL DEFAULT '',
                node_id TEXT,
                created_at TEXT NOT NULL
            );

            -- Phase Omega V — Audio Intelligence
            CREATE TABLE IF NOT EXISTS transcriptions (
                id TEXT PRIMARY KEY,
                audio_path TEXT NOT NULL,
                text TEXT NOT NULL DEFAULT '',
                duration_seconds INTEGER NOT NULL DEFAULT 0,
                action_items TEXT NOT NULL DEFAULT '[]',
                key_decisions TEXT NOT NULL DEFAULT '[]',
                node_id TEXT,
                created_at TEXT NOT NULL
            );

            -- Phase Omega V — Real-Time Data Streams
            CREATE TABLE IF NOT EXISTS data_streams (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                stream_type TEXT NOT NULL,
                url TEXT NOT NULL,
                poll_interval_mins INTEGER NOT NULL DEFAULT 60,
                last_polled TEXT,
                enabled INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS stream_events (
                id TEXT PRIMARY KEY,
                stream_id TEXT NOT NULL,
                title TEXT NOT NULL,
                content TEXT NOT NULL DEFAULT '',
                source_url TEXT,
                content_hash TEXT,
                created_at TEXT NOT NULL
            );

            -- Phase Omega — Cognitive Fingerprint (singleton table)
            CREATE TABLE IF NOT EXISTS cognitive_fingerprint (
                risk_tolerance REAL NOT NULL DEFAULT 0.5,
                decision_speed REAL NOT NULL DEFAULT 0.5,
                information_threshold REAL NOT NULL DEFAULT 0.5,
                reversibility_preference REAL NOT NULL DEFAULT 0.5,
                approach_style TEXT NOT NULL DEFAULT '[]',
                abstraction_level REAL NOT NULL DEFAULT 0.5,
                iteration_speed REAL NOT NULL DEFAULT 0.5,
                debugging_style TEXT NOT NULL DEFAULT 'systematic',
                verbosity REAL NOT NULL DEFAULT 0.5,
                formality REAL NOT NULL DEFAULT 0.5,
                directness REAL NOT NULL DEFAULT 0.5,
                technical_depth REAL NOT NULL DEFAULT 0.5,
                peak_hours TEXT NOT NULL DEFAULT '[]',
                context_switch_cost REAL NOT NULL DEFAULT 0.5,
                deep_work_duration INTEGER NOT NULL DEFAULT 90,
                expertise TEXT NOT NULL DEFAULT '{}',
                last_updated TEXT NOT NULL DEFAULT '',
                version INTEGER NOT NULL DEFAULT 0,
                confidence REAL NOT NULL DEFAULT 0.0
            );
            "
        ).map_err(|e| BrainError::Database(format!("Schema init failed: {}", e)))?;

        // Create indices (fast on empty or small tables, background for large)
        conn.execute_batch(
            "
            CREATE INDEX IF NOT EXISTS idx_nodes_domain ON nodes(domain);
            CREATE INDEX IF NOT EXISTS idx_nodes_topic ON nodes(topic);
            CREATE INDEX IF NOT EXISTS idx_nodes_type ON nodes(node_type);
            CREATE INDEX IF NOT EXISTS idx_nodes_source ON nodes(source_type);
            CREATE INDEX IF NOT EXISTS idx_nodes_quality ON nodes(quality_score);
            CREATE INDEX IF NOT EXISTS idx_nodes_decay ON nodes(decay_score);
            CREATE INDEX IF NOT EXISTS idx_nodes_created ON nodes(created_at);
            CREATE INDEX IF NOT EXISTS idx_nodes_content_hash ON nodes(content_hash);
            CREATE INDEX IF NOT EXISTS idx_nodes_cognitive ON nodes(cognitive_type);
            CREATE INDEX IF NOT EXISTS idx_nodes_synthesized ON nodes(synthesized_by_brain);
            CREATE INDEX IF NOT EXISTS idx_nodes_tier ON nodes(memory_tier);
            CREATE INDEX IF NOT EXISTS idx_nodes_brain ON nodes(brain_id);
            CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source_id);
            CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target_id);
            CREATE INDEX IF NOT EXISTS idx_edges_type ON edges(relation_type);
            CREATE INDEX IF NOT EXISTS idx_cognition_type ON user_cognition(pattern_type);
            CREATE INDEX IF NOT EXISTS idx_circuit_log_name ON autonomy_circuit_log(circuit_name);
            CREATE INDEX IF NOT EXISTS idx_circuit_log_started ON autonomy_circuit_log(started_at);
            CREATE INDEX IF NOT EXISTS idx_mcp_call_at ON mcp_call_log(called_at);
            CREATE INDEX IF NOT EXISTS idx_knowledge_rules_type ON knowledge_rules(rule_type);
            CREATE INDEX IF NOT EXISTS idx_knowledge_rules_invalidated ON knowledge_rules(invalidated);
            CREATE INDEX IF NOT EXISTS idx_capabilities_status ON capabilities(status);
            CREATE INDEX IF NOT EXISTS idx_capabilities_name ON capabilities(name);
            CREATE INDEX IF NOT EXISTS idx_world_entities_type ON world_entities(entity_type);
            CREATE INDEX IF NOT EXISTS idx_causal_links_cause ON causal_links(cause_id);
            CREATE INDEX IF NOT EXISTS idx_causal_links_effect ON causal_links(effect_id);
            CREATE INDEX IF NOT EXISTS idx_temporal_patterns_domain ON temporal_patterns(domain);
            CREATE INDEX IF NOT EXISTS idx_future_predictions_due ON future_predictions(due_at);
            CREATE INDEX IF NOT EXISTS idx_future_predictions_validated ON future_predictions(validated);
            CREATE INDEX IF NOT EXISTS idx_attention_focus_score ON attention_focus(attention_score);
            CREATE INDEX IF NOT EXISTS idx_learning_velocity_domain ON learning_velocity(domain);
            CREATE INDEX IF NOT EXISTS idx_visual_analysis_path ON visual_analysis(image_path);
            CREATE INDEX IF NOT EXISTS idx_transcriptions_path ON transcriptions(audio_path);
            CREATE INDEX IF NOT EXISTS idx_data_streams_type ON data_streams(stream_type);
            CREATE INDEX IF NOT EXISTS idx_data_streams_enabled ON data_streams(enabled);
            CREATE INDEX IF NOT EXISTS idx_stream_events_stream ON stream_events(stream_id);
            CREATE INDEX IF NOT EXISTS idx_stream_events_hash ON stream_events(content_hash);

            -- Power telemetry (Phase 1 — every LLM inference logs one row here)
            CREATE TABLE IF NOT EXISTS inference_log (
                id          TEXT    PRIMARY KEY,
                circuit     TEXT    NOT NULL DEFAULT 'unknown',
                backend     TEXT    NOT NULL,
                model       TEXT    NOT NULL,
                tokens_in   INTEGER NOT NULL DEFAULT 0,
                tokens_out  INTEGER NOT NULL DEFAULT 0,
                duration_ms INTEGER NOT NULL,
                energy_wh   REAL    NOT NULL,
                created_at  TEXT    NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_inference_log_created_at ON inference_log(created_at);
            CREATE INDEX IF NOT EXISTS idx_inference_log_circuit ON inference_log(circuit);
            CREATE INDEX IF NOT EXISTS idx_inference_log_backend ON inference_log(backend);
            "
        ).map_err(|e| BrainError::Database(format!("Index creation failed: {}", e)))?;

        Ok(())
    }

    /// Direct access to the raw connection (for use inside spawn_blocking).
    pub fn conn_raw(&self) -> Arc<Mutex<Connection>> {
        self.conn.clone()
    }

    /// Async-safe access to the SQLite connection. Wraps the closure in
    /// `spawn_blocking` so the Mutex lock never blocks the Tokio runtime.
    pub async fn with_conn<F, R>(&self, f: F) -> Result<R, BrainError>
    where
        F: FnOnce(&Connection) -> Result<R, BrainError> + Send + 'static,
        R: Send + 'static,
    {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| BrainError::Database(format!("Lock: {}", e)))?;
            f(&conn)
        })
        .await
        .map_err(|e| BrainError::Internal(e.to_string()))?
    }

    /// Background index builder — no-op now since SQLite creates all
    /// indices at init (they're fast even on large tables).
    pub async fn build_performance_indices(&self) {
        log::info!("SQLite: all indices created at init — nothing to do in background");
    }
}
