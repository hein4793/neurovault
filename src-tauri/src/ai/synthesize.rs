use crate::ai::client::LlmClient;
use crate::db::BrainDb;
use crate::db::models::GraphNode;
use crate::error::BrainError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainAnswer {
    pub answer: String,
    pub sources: Vec<GraphNode>,
    pub confidence: f64,
}

/// Ask a question to the brain and get an AI-synthesized answer
pub async fn answer_question(
    db: &BrainDb,
    llm: &LlmClient,
    embedding_client: Option<&crate::embeddings::OllamaClient>,
    question: &str,
) -> Result<BrainAnswer, BrainError> {
    // Try semantic search first, fall back to text search
    let search_results = if let Some(emb_client) = embedding_client {
        match emb_client.generate_embedding(question).await {
            Ok(query_emb) => db.vector_search(query_emb, 10).await.unwrap_or_default(),
            Err(_) => db.search_nodes(question).await.unwrap_or_default(),
        }
    } else {
        db.search_nodes(question).await.unwrap_or_default()
    };

    if search_results.is_empty() {
        return Ok(BrainAnswer {
            answer: "I don't have enough knowledge to answer this question. Try researching this topic first.".to_string(),
            sources: vec![],
            confidence: 0.0,
        });
    }

    // Build context from top results
    let sources: Vec<GraphNode> = search_results.iter().take(8).map(|r| r.node.clone()).collect();
    let context: String = sources
        .iter()
        .enumerate()
        .map(|(i, n)| format!("[Source {}] {}\n{}\n", i + 1, n.title, crate::truncate_str(&n.content, 800)))
        .collect();

    let prompt = format!(
        "You are a knowledgeable AI assistant. Based on the following knowledge sources, answer this question thoroughly and accurately. Reference which sources you used.\n\nQuestion: {}\n\nKnowledge Sources:\n{}\n\nProvide a clear, comprehensive answer:",
        question, context
    );

    let answer = llm.generate(&prompt, 1000).await?;

    let confidence = if search_results.len() >= 5 { 0.9 }
        else if search_results.len() >= 3 { 0.7 }
        else if search_results.len() >= 1 { 0.5 }
        else { 0.1 };

    Ok(BrainAnswer {
        answer,
        sources,
        confidence,
    })
}
