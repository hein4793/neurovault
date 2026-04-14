# Circuit Guide

## How Circuits Work

Circuits are NeuroVault's autonomous self-improvement engine. They are the background processes that make the brain grow smarter over time without user intervention.

### Rotation System

- **One circuit runs every 20 minutes**, selected by the dispatcher
- The dispatcher uses a **rotation window of 3** -- a circuit can't run again if it was one of the last 3 to execute
- This yields approximately **72 improvement cycles per day**
- Rotation state is **persisted to SQLite** (`autonomy_circuit_rotation`), so the brain resumes correctly after restarts
- Every run is **logged** to `autonomy_circuit_log` with circuit name, status (ok/err/skipped), result summary, duration, and timestamp

### Dispatch Flow

```
  Autonomy Loop (60s tick)
        |
  Is 20 minutes since last circuit? ──No──> Wait
        |
       Yes
        |
  Load recent rotation (last 3 circuits)
        |
  Pick next circuit (not in last 3)
        |
  Execute circuit
        |
  Log result to autonomy_circuit_log
        |
  Update rotation state
```

### Master Cognitive Loop

Separate from the circuit dispatcher, the **Master Cognitive Loop** runs every 30 minutes as a higher-order monitor:

1. **OBSERVE** -- Pull recent activity signals (circuit logs, MCP calls, node creation rates)
2. **ANALYZE** -- Identify patterns and inefficiencies
3. **IMPROVE** -- Create insight nodes, queue research missions
4. **ACT** -- Persist findings to `master_loop_log`

---

## How to Write a New Circuit

### Step 1: Register the Circuit

Add your circuit name to the `ALL_CIRCUITS` array in `src-tauri/src/circuits.rs`:

```rust
pub const ALL_CIRCUITS: &[&str] = &[
    // ... existing circuits ...
    "my_new_circuit",  // <-- add here
];
```

### Step 2: Add the Match Arm

In the `run_circuit` function in `circuits.rs`, add a match arm:

```rust
async fn run_circuit(db: &Arc<BrainDb>, circuit: &str) -> Result<String, BrainError> {
    match circuit {
        // ... existing arms ...
        "my_new_circuit" => my_new_circuit(db).await,
        _ => Err(BrainError::Circuit(format!("Unknown circuit: {}", circuit))),
    }
}
```

### Step 3: Implement the Circuit

```rust
/// My New Circuit -- brief description of what it does.
///
/// This circuit [explain the improvement it provides].
/// It runs as part of the 20-minute rotation and should complete
/// within 5 minutes.
async fn my_new_circuit(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    // 1. QUERY: Gather data from the database
    let items: Vec<SomeType> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, content FROM nodes
             WHERE node_type = 'reference'
             ORDER BY created_at DESC LIMIT 50"
        ).map_err(|e| BrainError::Database(e.to_string()))?;

        let rows = stmt.query_map([], |row| {
            Ok(SomeType {
                id: row.get(0)?,
                title: row.get(1)?,
                content: row.get(2)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;

        let mut result = Vec::new();
        for r in rows {
            if let Ok(item) = r { result.push(item); }
        }
        Ok(result)
    }).await?;

    // Early return if nothing to process
    if items.is_empty() {
        return Ok("No items to process".to_string());
    }

    // 2. PROCESS: Analyze, transform, or compute
    let mut processed = 0;
    for item in &items {
        // ... your logic here ...
        processed += 1;
    }

    // 3. WRITE: Store results back (new nodes, updated scores, etc.)
    let node_id = format!("node:{}", uuid::Uuid::now_v7());
    let now = chrono::Utc::now().to_rfc3339();
    db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO nodes (id, title, content, summary, domain, topic, tags,
                                node_type, source_type, quality_score, visual_size,
                                decay_score, access_count, synthesized_by_brain,
                                created_at, updated_at, accessed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, '[]', 'insight', 'circuit', 0.7, 3.0, 1.0, 0, 1, ?7, ?7, ?7)",
            rusqlite::params![node_id, "My Insight Title", "Content...", "Summary...", "general", "general", now],
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await?;

    // 4. RETURN: Summary string for the circuit log
    Ok(format!("Processed {} items, created 1 insight node", processed))
}
```

---

## Circuit Design Guidelines

### Do

- **Keep it fast**: Target under 5 minutes execution time. Sample data if the full set is too large.
- **Be idempotent**: Running the circuit twice shouldn't cause duplicate data or broken state.
- **Handle errors gracefully**: Return `Err(BrainError::...)` instead of panicking. A failing circuit shouldn't crash the rotation.
- **Use `db.with_conn()`**: All database access goes through the connection pool.
- **Return descriptive results**: The result string appears in the circuit log and is visible to the master loop.
- **Create thinking nodes**: Circuits that generate insight, hypothesis, decision, strategy, contradiction, or prediction nodes add the most value.

### Don't

- **Don't block the async runtime**: Use `tokio::task::spawn_blocking` for CPU-heavy work.
- **Don't process everything at once**: Sample or limit your queries. Process 50-100 items per cycle, not 10,000.
- **Don't make external network calls** unless the circuit explicitly needs them (like `curiosity_gap_fill` querying Ollama).
- **Don't modify circuit rotation state**: The dispatcher handles rotation. Your circuit just does its work and returns.
- **Don't use `unwrap()`**: Handle all errors with `?` or `.unwrap_or_default()`.

---

## Testing Circuits

### Manual Testing

You can trigger a specific circuit by temporarily modifying the dispatch logic, or by calling the circuit function directly in a test:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_my_circuit() {
        // Set up a test database
        let db = Arc::new(BrainDb::open_test().await);

        // Insert test data
        // ...

        // Run the circuit
        let result = my_new_circuit(&db).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Processed"));
    }
}
```

### Checking Circuit Logs

Query the circuit log to see how your circuit performed:

```sql
SELECT circuit_name, status, result, duration_ms, started_at
FROM autonomy_circuit_log
WHERE circuit_name = 'my_new_circuit'
ORDER BY started_at DESC
LIMIT 10;
```

### Performance Monitoring

The `circuit_optimizer` circuit automatically tracks circuit performance over time. Check `circuit_performance` table for metrics on execution time, success rate, and impact.
