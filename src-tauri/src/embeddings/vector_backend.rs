//! Vector Backend Abstraction — Phase 3.4.
#![allow(dead_code)]
use crate::error::BrainError;
use async_trait::async_trait;

#[async_trait]
pub trait VectorBackend: Send + Sync {
    async fn search(&self, query: &[f64], k: usize) -> Vec<(String, f64)>;
    async fn len(&self) -> usize;
    async fn is_empty(&self) -> bool { self.len().await == 0 }
    async fn mark_dirty(&self);
    fn name(&self) -> &'static str;
}

pub struct HnswBackend { inner: crate::embeddings::hnsw::SharedHnsw }

impl HnswBackend {
    pub fn new(inner: crate::embeddings::hnsw::SharedHnsw) -> Self { Self { inner } }
}

#[async_trait]
impl VectorBackend for HnswBackend {
    async fn search(&self, query: &[f64], k: usize) -> Vec<(String, f64)> { self.inner.read().await.search(query, k) }
    async fn len(&self) -> usize { self.inner.read().await.len() }
    async fn is_empty(&self) -> bool { self.inner.read().await.is_empty() }
    async fn mark_dirty(&self) { self.inner.write().await.mark_dirty(); }
    fn name(&self) -> &'static str { "hnsw (instant-distance)" }
}

pub struct LanceDbBackend {
    #[cfg(feature = "lancedb-backend")]
    conn: lancedb::Connection,
    #[cfg(feature = "lancedb-backend")]
    table_name: String,
    #[cfg(feature = "lancedb-backend")]
    dim: usize,
    #[cfg(not(feature = "lancedb-backend"))]
    _disabled: (),
}

impl LanceDbBackend {
    #[allow(unused_variables)]
    pub async fn new(path: std::path::PathBuf, dim: usize) -> Result<Self, BrainError> {
        #[cfg(not(feature = "lancedb-backend"))]
        { let _ = (path, dim); Err(BrainError::Internal("LanceDB requires 'lancedb-backend' feature".into())) }
        #[cfg(feature = "lancedb-backend")]
        {
            let conn = lancedb::connect(&path.to_string_lossy()).execute().await
                .map_err(|e| BrainError::Internal(format!("lancedb connect: {}", e)))?;
            Ok(Self { conn, table_name: "embeddings".to_string(), dim })
        }
    }
}

#[async_trait]
impl VectorBackend for LanceDbBackend {
    #[allow(unused_variables)]
    async fn search(&self, query: &[f64], k: usize) -> Vec<(String, f64)> {
        #[cfg(not(feature = "lancedb-backend"))] { Vec::new() }
        #[cfg(feature = "lancedb-backend")]
        {
            use futures::TryStreamExt;
            let table = match self.conn.open_table(&self.table_name).execute().await { Ok(t) => t, Err(_) => return Vec::new() };
            let query_f32: Vec<f32> = query.iter().map(|x| *x as f32).collect();
            let mut results = Vec::new();
            if let Ok(qb) = table.vector_search(query_f32).and_then(|q| Ok(q.limit(k))) {
                if let Ok(mut stream) = qb.execute().await {
                    while let Ok(Some(batch)) = stream.try_next().await {
                        if let (Some(ids), Some(dists)) = (
                            batch.column_by_name("id").and_then(|c| c.as_any().downcast_ref::<arrow_array::StringArray>()),
                            batch.column_by_name("_distance").and_then(|c| c.as_any().downcast_ref::<arrow_array::Float32Array>()),
                        ) {
                            for i in 0..ids.len() {
                                let sim = (1.0 - dists.value(i) as f64 / 2.0).clamp(0.0, 1.0);
                                results.push((ids.value(i).to_string(), sim));
                            }
                        }
                    }
                }
            }
            results.truncate(k);
            results
        }
    }
    async fn len(&self) -> usize {
        #[cfg(not(feature = "lancedb-backend"))] { 0 }
        #[cfg(feature = "lancedb-backend")]
        { match self.conn.open_table(&self.table_name).execute().await { Ok(t) => t.count_rows(None).await.unwrap_or(0), Err(_) => 0 } }
    }
    async fn mark_dirty(&self) {}
    fn name(&self) -> &'static str {
        #[cfg(feature = "lancedb-backend")] { "lancedb" }
        #[cfg(not(feature = "lancedb-backend"))] { "lancedb (disabled)" }
    }
}

#[allow(dead_code, unused_variables)]
pub async fn migrate_hnsw_to_lancedb(db: &crate::db::BrainDb, output_path: std::path::PathBuf) -> Result<u64, BrainError> {
    #[cfg(not(feature = "lancedb-backend"))]
    { Err(BrainError::Internal("Migration requires 'lancedb-backend' feature".into())) }
    #[cfg(feature = "lancedb-backend")]
    {
        use arrow_array::{FixedSizeListArray, Float32Array, RecordBatch, RecordBatchIterator, StringArray};
        use arrow_schema::{DataType, Field, Schema};
        use rusqlite::params;
        use std::sync::Arc;

        std::fs::create_dir_all(&output_path).map_err(|e| BrainError::Internal(format!("mkdir: {}", e)))?;
        let conn = lancedb::connect(&output_path.to_string_lossy()).execute().await
            .map_err(|e| BrainError::Internal(format!("lancedb connect: {}", e)))?;

        let rows: Vec<(String, Vec<f64>, usize)> = db.with_conn(|conn2| {
            let mut stmt = conn2.prepare(
                "SELECT n.id, e.vector, e.dimension FROM nodes n \
                 INNER JOIN embeddings e ON e.node_id = n.id"
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            let r = stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                let blob: Vec<u8> = row.get(1)?;
                let dim: usize = row.get(2)?;
                let emb: Vec<f64> = blob.chunks_exact(8).take(dim)
                    .map(|c| f64::from_le_bytes(c.try_into().unwrap_or([0u8; 8]))).collect();
                Ok((id, emb, dim))
            }).map_err(|e| BrainError::Database(e.to_string()))?;
            let mut result = Vec::new();
            for row in r { if let Ok(v) = row { result.push(v); } }
            Ok(result)
        }).await?;

        if rows.is_empty() { return Ok(0); }
        let dim = rows[0].2;
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("vector", DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), dim as i32), false),
        ]));

        let mut ids = Vec::new();
        let mut flat: Vec<f32> = Vec::new();
        for (id, emb, d) in &rows {
            if *d != dim { continue; }
            ids.push(id.clone());
            let mut v: Vec<f32> = emb.iter().map(|x| *x as f32).collect();
            let mag: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            if mag > 0.0 { for x in v.iter_mut() { *x /= mag; } }
            flat.extend_from_slice(&v);
        }

        let id_arr = Arc::new(StringArray::from(ids));
        let values_arr = Arc::new(Float32Array::from(flat));
        let list_field = Arc::new(Field::new("item", DataType::Float32, true));
        let vec_arr = Arc::new(FixedSizeListArray::try_new(list_field, dim as i32, values_arr, None)
            .map_err(|e| BrainError::Internal(format!("FixedSizeList: {}", e)))?);
        let batch = RecordBatch::try_new(schema.clone(), vec![id_arr, vec_arr])
            .map_err(|e| BrainError::Internal(format!("RecordBatch: {}", e)))?;
        let total = batch.num_rows() as u64;

        let _ = conn.drop_table("embeddings").await.ok();
        let iter = RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema);
        conn.create_table("embeddings", Box::new(iter)).execute().await
            .map_err(|e| BrainError::Internal(format!("create_table: {}", e)))?;

        log::info!("LanceDB migration: {} embeddings written", total);
        Ok(total)
    }
}

pub struct QdrantBackend;
impl QdrantBackend {
    pub fn new(_url: String) -> Result<Self, BrainError> {
        Err(BrainError::Internal("Qdrant backend not yet implemented".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embeddings::hnsw;

    #[tokio::test]
    async fn hnsw_backend_wraps_shared() {
        let shared = hnsw::shared();
        let backend = HnswBackend::new(shared);
        assert!(backend.is_empty().await);
        assert_eq!(backend.len().await, 0);
        assert_eq!(backend.name(), "hnsw (instant-distance)");
        assert!(backend.search(&[1.0, 0.0, 0.0], 5).await.is_empty());
    }

    #[tokio::test]
    async fn hnsw_backend_mark_dirty_increments_pending() {
        let shared = hnsw::shared();
        let backend = HnswBackend::new(shared.clone());
        backend.mark_dirty().await;
        backend.mark_dirty().await;
        assert_eq!(shared.read().await.pending(), 2);
    }
}
