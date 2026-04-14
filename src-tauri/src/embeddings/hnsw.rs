//! HNSW index for fast semantic search.

use crate::db::BrainDb;
use crate::error::BrainError;
use instant_distance::{Builder, HnswMap, Point as IdPoint, Search};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Point(pub Vec<f32>);

impl IdPoint for Point {
    fn distance(&self, other: &Self) -> f32 {
        debug_assert_eq!(self.0.len(), other.0.len());
        let mut sum = 0f32;
        for i in 0..self.0.len() { let d = self.0[i] - other.0[i]; sum += d * d; }
        sum
    }
}

fn normalize(v: &mut [f32]) {
    let mag: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if mag > 0.0 { for x in v.iter_mut() { *x /= mag; } }
}

pub fn embedding_to_point(emb: &[f64]) -> Point {
    let mut v: Vec<f32> = emb.iter().map(|x| *x as f32).collect();
    normalize(&mut v);
    Point(v)
}

const SNAPSHOT_VERSION: u32 = 2;

#[derive(Serialize, Deserialize)]
struct HnswSnapshot { version: u32, dimension: usize, node_ids: Vec<String>, flat_points: Vec<f32> }

pub struct HnswIndex {
    inner: Option<HnswMap<Point, usize>>,
    node_ids: Vec<String>,
    raw_points_storage: Vec<Point>,
    pending: usize,
    rebuild_threshold: usize,
    dimension: usize,
}

impl HnswIndex {
    pub fn new() -> Self {
        Self { inner: None, node_ids: Vec::new(), raw_points_storage: Vec::new(), pending: 0, rebuild_threshold: 200, dimension: 0 }
    }
    pub fn len(&self) -> usize { self.node_ids.len() }
    pub fn is_empty(&self) -> bool { self.inner.is_none() || self.node_ids.is_empty() }
    #[allow(dead_code)]
    pub fn dimension(&self) -> usize { self.dimension }
    pub fn pending(&self) -> usize { self.pending }
    pub fn needs_rebuild(&self) -> bool { self.pending >= self.rebuild_threshold }
    pub fn mark_dirty(&mut self) { self.pending += 1; }

    pub fn load_from_disk(path: &Path) -> Result<Option<Self>, BrainError> {
        if !path.exists() { return Ok(None); }
        let bytes = std::fs::read(path).map_err(|e| BrainError::Internal(format!("read hnsw.bin: {}", e)))?;
        let snapshot: HnswSnapshot = match bincode::deserialize(&bytes) {
            Ok(s) => s, Err(e) => { log::warn!("HNSW snapshot deserialise failed ({}) — rebuilding", e); return Ok(None); }
        };
        if snapshot.version != SNAPSHOT_VERSION { return Ok(None); }
        if snapshot.flat_points.len() != snapshot.node_ids.len() * snapshot.dimension { return Ok(None); }

        let dim = snapshot.dimension;
        let n = snapshot.node_ids.len();
        let mut points: Vec<Point> = Vec::with_capacity(n);
        for i in 0..n { let start = i * dim; points.push(Point(snapshot.flat_points[start..start + dim].to_vec())); }
        let values: Vec<usize> = (0..n).collect();
        let raw_storage = points.clone();
        let hnsw = Builder::default().build(points, values);
        log::info!("HNSW: index rebuilt from {} disk points", n);
        Ok(Some(Self { inner: Some(hnsw), node_ids: snapshot.node_ids, raw_points_storage: raw_storage, pending: 0, rebuild_threshold: 200, dimension: dim }))
    }

    pub fn save_to_disk(&self, path: &Path) -> Result<(), BrainError> {
        if self.raw_points_storage.is_empty() { return Err(BrainError::Internal("no points to save".into())); }
        let mut flat_points: Vec<f32> = Vec::with_capacity(self.raw_points_storage.len() * self.dimension);
        for p in &self.raw_points_storage { flat_points.extend_from_slice(&p.0); }
        let snapshot = HnswSnapshot { version: SNAPSHOT_VERSION, dimension: self.dimension, node_ids: self.node_ids.clone(), flat_points };
        let bytes = bincode::serialize(&snapshot).map_err(|e| BrainError::Internal(format!("hnsw serialize: {}", e)))?;
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent).ok(); }
        std::fs::write(path, bytes).map_err(|e| BrainError::Internal(format!("write hnsw.bin: {}", e)))?;
        log::info!("HNSW: saved {} points to {}", self.node_ids.len(), path.display());
        Ok(())
    }

    pub async fn build_from_db(&mut self, db: &BrainDb, cap: usize) -> Result<usize, BrainError> {
        log::info!("HNSW: building index from DB (cap={})...", cap);

        let cap_val = cap;
        let (ids, raw_points, dim) = db.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT n.id, e.vector, e.dimension FROM nodes n \
                 INNER JOIN embeddings e ON e.node_id = n.id"
            ).map_err(|e| BrainError::Database(e.to_string()))?;

            let mut ids: Vec<String> = Vec::new();
            let mut points: Vec<Point> = Vec::new();
            let mut dim = 0usize;

            let rows = stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                let blob: Vec<u8> = row.get(1)?;
                let d: usize = row.get(2)?;
                Ok((id, blob, d))
            }).map_err(|e| BrainError::Database(e.to_string()))?;

            for r in rows {
                if points.len() >= cap_val { break; }
                if let Ok((id, blob, d)) = r {
                    if d == 0 { continue; }
                    if dim == 0 { dim = d; }
                    if d != dim { continue; }
                    let emb: Vec<f64> = blob.chunks_exact(8).take(d)
                        .map(|c| f64::from_le_bytes(c.try_into().unwrap_or([0u8; 8]))).collect();
                    if emb.is_empty() { continue; }
                    points.push(embedding_to_point(&emb));
                    ids.push(id);
                }
            }
            Ok((ids, points, dim))
        }).await?;

        if raw_points.is_empty() {
            log::warn!("HNSW: no embedded nodes found");
            return Ok(0);
        }

        log::info!("HNSW: building over {} points (dim={})...", raw_points.len(), dim);
        let values: Vec<usize> = (0..raw_points.len()).collect();
        let raw_storage = raw_points.clone();
        let hnsw = Builder::default().build(raw_points, values);

        self.inner = Some(hnsw);
        self.node_ids = ids;
        self.raw_points_storage = raw_storage;
        self.dimension = dim;
        self.pending = 0;

        log::info!("HNSW: build complete ({} points)", self.node_ids.len());
        Ok(self.node_ids.len())
    }

    pub fn search(&self, query: &[f64], k: usize) -> Vec<(String, f64)> {
        let hnsw = match &self.inner { Some(h) => h, None => return Vec::new() };
        if hnsw.values.is_empty() || self.node_ids.is_empty() { return Vec::new(); }
        let qpoint = embedding_to_point(query);
        let mut search = Search::default();
        let mut out: Vec<(String, f64)> = Vec::with_capacity(k);
        for item in hnsw.search(&qpoint, &mut search).take(k) {
            let cos_sim = (1.0 - (item.distance as f64) / 2.0).clamp(0.0, 1.0);
            let pos = *item.value;
            if let Some(id) = self.node_ids.get(pos) { out.push((id.clone(), cos_sim)); }
        }
        out
    }
}

impl Default for HnswIndex { fn default() -> Self { Self::new() } }

pub type SharedHnsw = Arc<RwLock<HnswIndex>>;
pub fn shared() -> SharedHnsw { Arc::new(RwLock::new(HnswIndex::new())) }

pub async fn load_or_build(db: Arc<BrainDb>, hnsw: SharedHnsw) {
    let path = db.config.hnsw_index_path();
    match HnswIndex::load_from_disk(&path) {
        Ok(Some(loaded)) => { *hnsw.write().await = loaded; log::info!("HNSW: ready from disk"); return; }
        Ok(None) => log::info!("HNSW: no snapshot, building from DB..."),
        Err(e) => log::warn!("HNSW: load failed ({}), building from DB...", e),
    }
    let mut idx = HnswIndex::new();
    match idx.build_from_db(&db, 100_000).await {
        Ok(0) => { *hnsw.write().await = idx; }
        Ok(n) => { let _ = idx.save_to_disk(&path); *hnsw.write().await = idx; log::info!("HNSW: ready ({} points)", n); }
        Err(e) => log::error!("HNSW: build failed: {}", e),
    }
}

pub async fn rebuild_loop(db: Arc<BrainDb>, hnsw: SharedHnsw) {
    tokio::time::sleep(std::time::Duration::from_secs(300)).await;
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        if !hnsw.read().await.needs_rebuild() { continue; }
        let pending = hnsw.read().await.pending();
        log::info!("HNSW: {} pending vectors, rebuilding...", pending);
        let mut new_idx = HnswIndex::new();
        match new_idx.build_from_db(&db, usize::MAX).await {
            Ok(n) if n > 0 => {
                let path = db.config.hnsw_index_path();
                let _ = new_idx.save_to_disk(&path);
                *hnsw.write().await = new_idx;
                log::info!("HNSW: rebuild complete ({} points)", n);
            }
            Ok(_) => log::warn!("HNSW: rebuild produced empty index"),
            Err(e) => log::warn!("HNSW: rebuild failed: {}", e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_unit_vector() {
        let mut v = vec![3.0f32, 4.0, 0.0];
        normalize(&mut v);
        let mag: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((mag - 1.0).abs() < 1e-6);
    }

    #[test]
    fn embedding_to_point_normalizes() {
        let p = embedding_to_point(&[3.0, 4.0]);
        let mag: f32 = p.0.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((mag - 1.0).abs() < 1e-6);
    }

    #[test]
    fn point_distance_self_zero() { let p = Point(vec![0.6, 0.8]); assert!(p.distance(&p) < 1e-6); }

    #[test]
    fn empty_index_returns_no_results() { let idx = HnswIndex::new(); assert!(idx.search(&[1.0, 0.0, 0.0], 5).is_empty()); }

    #[test]
    fn dirty_threshold() {
        let mut idx = HnswIndex::new();
        idx.rebuild_threshold = 3;
        assert!(!idx.needs_rebuild());
        idx.mark_dirty(); idx.mark_dirty();
        assert!(!idx.needs_rebuild());
        idx.mark_dirty();
        assert!(idx.needs_rebuild());
    }
}
