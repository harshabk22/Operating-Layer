use super::db;
use super::models::{KnowledgeDocument, KnowledgeQueryResult};
use crate::MoatResult;
use rusqlite::Connection;
use uuid::Uuid;

pub fn add_document(
    conn: &Connection,
    title: &str,
    content: &str,
    category: &str,
) -> MoatResult<KnowledgeDocument> {
    let now = chrono::Utc::now().timestamp();
    let doc = KnowledgeDocument {
        id: Uuid::new_v4().to_string(),
        title: title.to_string(),
        content: content.to_string(),
        category: category.to_string(),
        updated_at: now,
    };

    db::insert_knowledge_document(conn, &doc)?;
    Ok(doc)
}

pub fn query_knowledge_store(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> MoatResult<Vec<KnowledgeDocument>> {
    // Check if query is empty or only whitespace
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    // Escape query parameters for SQLite FTS5 MATCH syntax if they contain special characters
    let clean_query = trimmed
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect::<String>();
    let fts_query = clean_query
        .split_whitespace()
        .map(|word| format!("\"{}\"", word))
        .collect::<Vec<_>>()
        .join(" OR ");

    if fts_query.is_empty() {
        return Ok(Vec::new());
    }

    db::search_knowledge_documents_ranked(conn, &fts_query, limit).map(|results| results.into_iter().map(|(doc, _)| doc).collect())
}

pub fn query_knowledge_store_ranked(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> MoatResult<Vec<KnowledgeQueryResult>> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let clean_query = trimmed
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect::<String>();
    let fts_query = clean_query
        .split_whitespace()
        .map(|word| format!("\"{}\"", word))
        .collect::<Vec<_>>()
        .join(" OR ");

    if fts_query.is_empty() {
        return Ok(Vec::new());
    }

    let results = db::search_knowledge_documents_ranked(conn, &fts_query, limit)?;
    let mut query_results = Vec::new();

    for (doc, rank) in results {
        // Sigmoid mapping: since rank is more negative for better matches,
        // rank.exp() goes to 0 as rank goes to -infinity, meaning relevance goes to 1.
        // rank.exp() goes to infinity as rank goes to +infinity, meaning relevance goes to 0.
        let relevance = 1.0 / (1.0 + rank.exp());
        let confidence = relevance;

        query_results.push(KnowledgeQueryResult {
            document_id: doc.id,
            title: doc.title,
            content: doc.content,
            relevance,
            category: doc.category,
            confidence,
        });
    }

    Ok(query_results)
}

pub fn search_knowledge_documents_ranked(_conn: &rusqlite::Connection, _query: &str) -> crate::MoatResult<Vec<super::models::KnowledgeDocument>> { Ok(vec![]) }
