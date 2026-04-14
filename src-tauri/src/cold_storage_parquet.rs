//! Parquet Cold Storage — compressed columnar format for cold-tier nodes.
#![allow(unused_imports, dead_code)]

use crate::db::BrainDb;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct ParquetColdStats { pub archived: u64, pub bytes_written: u64, pub archive_path: Option<PathBuf> }

pub async fn run_one_pass_parquet(db: &BrainDb) -> Result<ParquetColdStats, String> {
    #[cfg(not(feature = "parquet-storage"))]
    {
        let _ = db;
        Err("Parquet cold storage requires the 'parquet-storage' Cargo feature.".to_string())
    }

    #[cfg(feature = "parquet-storage")]
    {
        run_one_pass_parquet_real(db).await
    }
}

#[cfg(feature = "parquet-storage")]
async fn run_one_pass_parquet_real(db: &BrainDb) -> Result<ParquetColdStats, String> {
    use arrow_array::{Float64Array, StringArray, UInt64Array, builder::ListBuilder, builder::StringBuilder};
    use arrow_schema::{DataType, Field, Schema};
    use parquet::arrow::ArrowWriter;
    use parquet::basic::Compression;
    use parquet::file::properties::WriterProperties;
    use rusqlite::params;
    use std::sync::Arc;

    let mut stats = ParquetColdStats::default();
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();

    let cold_nodes: Vec<(String, String, String, String, String, String, String, String, String, String, Option<String>, Option<String>, f64, f64, String, String, String, u64)> = db
        .with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, title, content, summary, content_hash, domain, topic, tags, \
                 node_type, source_type, source_url, source_file, quality_score, decay_score, \
                 created_at, updated_at, accessed_at, access_count \
                 FROM nodes WHERE memory_tier = 'cold' AND updated_at < ?1 LIMIT 5000"
            ).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
            let rows = stmt.query_map(params![cutoff], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?,
                    row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?,
                    row.get(10)?, row.get(11)?, row.get(12)?, row.get(13)?,
                    row.get(14)?, row.get(15)?, row.get(16)?, row.get(17)?))
            }).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
            let mut result = Vec::new();
            for r in rows { if let Ok(row) = r { result.push(row); } }
            Ok(result)
        }).await.map_err(|e| e.to_string())?;

    if cold_nodes.is_empty() { return Ok(stats); }

    let n = cold_nodes.len();
    let mut id_arr = StringBuilder::with_capacity(n, n * 32);
    let mut title_arr = StringBuilder::with_capacity(n, n * 64);
    let mut content_arr = StringBuilder::with_capacity(n, n * 1024);
    let mut summary_arr = StringBuilder::with_capacity(n, n * 256);
    let mut hash_arr = StringBuilder::with_capacity(n, n * 32);
    let mut domain_arr = StringBuilder::with_capacity(n, n * 16);
    let mut topic_arr = StringBuilder::with_capacity(n, n * 32);
    let mut ntype_arr = StringBuilder::with_capacity(n, n * 16);
    let mut stype_arr = StringBuilder::with_capacity(n, n * 16);
    let mut surl_arr = StringBuilder::with_capacity(n, n * 64);
    let mut sfile_arr = StringBuilder::with_capacity(n, n * 64);
    let mut quality_arr: Vec<f64> = Vec::with_capacity(n);
    let mut decay_arr: Vec<f64> = Vec::with_capacity(n);
    let mut created_arr = StringBuilder::with_capacity(n, n * 32);
    let mut updated_arr = StringBuilder::with_capacity(n, n * 32);
    let mut accessed_arr = StringBuilder::with_capacity(n, n * 32);
    let mut access_count_arr: Vec<u64> = Vec::with_capacity(n);
    let inner_builder = StringBuilder::new();
    let mut tags_builder = ListBuilder::new(inner_builder);

    for row in &cold_nodes {
        id_arr.append_value(&row.0);
        title_arr.append_value(&row.1);
        content_arr.append_value(&row.2);
        summary_arr.append_value(&row.3);
        hash_arr.append_value(&row.4);
        domain_arr.append_value(&row.5);
        topic_arr.append_value(&row.6);
        ntype_arr.append_value(&row.8);
        stype_arr.append_value(&row.9);
        match &row.10 { Some(s) => surl_arr.append_value(s), None => surl_arr.append_null() }
        match &row.11 { Some(s) => sfile_arr.append_value(s), None => sfile_arr.append_null() }
        quality_arr.push(row.12);
        decay_arr.push(row.13);
        created_arr.append_value(&row.14);
        updated_arr.append_value(&row.15);
        accessed_arr.append_value(&row.16);
        access_count_arr.push(row.17);
        let tags: Vec<String> = serde_json::from_str(&row.7).unwrap_or_default();
        for tag in &tags { tags_builder.values().append_value(tag); }
        tags_builder.append(true);
    }

    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false), Field::new("title", DataType::Utf8, false),
        Field::new("content", DataType::Utf8, false), Field::new("summary", DataType::Utf8, false),
        Field::new("content_hash", DataType::Utf8, false), Field::new("domain", DataType::Utf8, false),
        Field::new("topic", DataType::Utf8, false),
        Field::new("tags", DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))), false),
        Field::new("node_type", DataType::Utf8, false), Field::new("source_type", DataType::Utf8, false),
        Field::new("source_url", DataType::Utf8, true), Field::new("source_file", DataType::Utf8, true),
        Field::new("quality_score", DataType::Float64, false), Field::new("decay_score", DataType::Float64, false),
        Field::new("created_at", DataType::Utf8, false), Field::new("updated_at", DataType::Utf8, false),
        Field::new("accessed_at", DataType::Utf8, false), Field::new("access_count", DataType::UInt64, false),
    ]));

    let batch = arrow_array::RecordBatch::try_new(schema.clone(), vec![
        Arc::new(id_arr.finish()), Arc::new(title_arr.finish()), Arc::new(content_arr.finish()),
        Arc::new(summary_arr.finish()), Arc::new(hash_arr.finish()), Arc::new(domain_arr.finish()),
        Arc::new(topic_arr.finish()), Arc::new(tags_builder.finish()), Arc::new(ntype_arr.finish()),
        Arc::new(stype_arr.finish()), Arc::new(surl_arr.finish()), Arc::new(sfile_arr.finish()),
        Arc::new(Float64Array::from(quality_arr)), Arc::new(Float64Array::from(decay_arr)),
        Arc::new(created_arr.finish()), Arc::new(updated_arr.finish()), Arc::new(accessed_arr.finish()),
        Arc::new(UInt64Array::from(access_count_arr)),
    ]).map_err(|e| format!("arrow batch: {}", e))?;

    let cold_dir = db.config.data_dir.join("cold");
    std::fs::create_dir_all(&cold_dir).map_err(|e| format!("mkdir: {}", e))?;
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let archive_path = cold_dir.join(format!("archive-{}.parquet", timestamp));

    let file = std::fs::File::create(&archive_path).map_err(|e| format!("create: {}", e))?;
    let props = WriterProperties::builder().set_compression(Compression::SNAPPY).build();
    let mut writer = ArrowWriter::try_new(file, schema, Some(props)).map_err(|e| format!("parquet writer: {}", e))?;
    writer.write(&batch).map_err(|e| format!("parquet write: {}", e))?;
    writer.close().map_err(|e| format!("parquet close: {}", e))?;

    let bytes_written = std::fs::metadata(&archive_path).map(|m| m.len()).unwrap_or(0);
    stats.archived = n as u64;
    stats.bytes_written = bytes_written;
    stats.archive_path = Some(archive_path.clone());

    log::info!("Parquet cold storage: archived {} nodes ({} bytes)", stats.archived, stats.bytes_written);
    Ok(stats)
}
