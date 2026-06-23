use crate::business::db;
use crate::business::models::{KnowledgeSource, KnowledgeSourceType};
use crate::MoatError;
use crate::MoatResult;
use rusqlite::Connection;
use scraper::{Html, Selector};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

pub struct KnowledgeIngestionPipeline {
    db: Arc<Mutex<Connection>>,
}

impl KnowledgeIngestionPipeline {
    pub fn new(db: Arc<Mutex<Connection>>) -> Self {
        Self { db }
    }

    pub async fn ingest_source(
        &self,
        title: &str,
        source_type: KnowledgeSourceType,
        url: Option<&str>,
        raw_content: &str,
    ) -> MoatResult<KnowledgeSource> {
        let timestamp = chrono::Utc::now().timestamp();
        let source_id = Uuid::new_v4().to_string();

        let extracted_text = match source_type {
            KnowledgeSourceType::Website => self.extract_text_from_html(raw_content),
            // For Text, PDF, DOCX, we rely on the caller providing raw extracted text to avoid binary dependencies
            _ => raw_content.to_string(),
        };

        let chunks = self.chunk_text(&extracted_text, 1000); // configurable chunk size

        let mut conn = self.db.lock().await;

        let tx = conn.transaction()?;

        for (i, chunk) in chunks.iter().enumerate() {
            let doc_id = Uuid::new_v4().to_string();
            let doc_title = format!("{} - Part {}", title, i + 1);

            tx.execute(
                "INSERT INTO business_knowledge_documents (id, title, content, category, source_id, updated_at) 
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![doc_id, doc_title, chunk, "ingested", source_id, timestamp],
            )?;
        }

        // FTS indexing is handled by triggers on business_knowledge_documents

        tx.commit()?;

        db::insert_knowledge_source(
            &conn,
            &source_id,
            source_type.as_str(),
            title,
            url,
            chunks.len() as i32,
            timestamp,
        )?;

        let sources = db::list_knowledge_sources(&conn)?;
        Ok(sources.into_iter().find(|s| s.id == source_id).unwrap())
    }

    pub async fn delete_source(&self, source_id: &str) -> MoatResult<()> {
        let conn = self.db.lock().await;
        db::delete_documents_by_source(&conn, source_id)?;
        db::delete_knowledge_source(&conn, source_id)?;
        Ok(())
    }

    fn extract_text_from_html(&self, html: &str) -> String {
        let document = Html::parse_document(html);
        let selector = Selector::parse("body").unwrap();
        
        let mut text = String::new();
        for element in document.select(&selector) {
            let t: Vec<&str> = element.text().collect();
            text.push_str(&t.join(" "));
        }
        text.replace("\n", " ").replace("  ", " ").trim().to_string()
    }

    fn chunk_text(&self, text: &str, chunk_size: usize) -> Vec<String> {
        let words: Vec<&str> = text.split_whitespace().collect();
        let mut chunks = Vec::new();
        let mut current_chunk = Vec::new();
        let mut current_len = 0;

        for word in words {
            if current_len + word.len() > chunk_size && !current_chunk.is_empty() {
                chunks.push(current_chunk.join(" "));
                current_chunk.clear();
                current_len = 0;
            }
            current_chunk.push(word);
            current_len += word.len() + 1; // +1 for space
        }

        if !current_chunk.is_empty() {
            chunks.push(current_chunk.join(" "));
        }

        chunks
    }
}
