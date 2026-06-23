use crate::MoatResult;
use rusqlite::Connection;

pub struct Migration {
    pub version: i32,
    pub up: &'static str,
    pub down: &'static str,
}

pub fn get_migrations() -> Vec<Migration> {
    vec![
        Migration {
            version: 1,
            up: r#"
        CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at INTEGER NOT NULL
        );
        
        CREATE TABLE IF NOT EXISTS business_external_contacts (
            id TEXT PRIMARY KEY,
            name TEXT,
            email TEXT,
            phone TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS business_identity_mappings (
            id TEXT PRIMARY KEY,
            contact_id TEXT NOT NULL,
            channel TEXT NOT NULL,
            external_id TEXT NOT NULL,
            FOREIGN KEY(contact_id) REFERENCES business_external_contacts(id),
            UNIQUE(channel, external_id)
        );

        CREATE TABLE IF NOT EXISTS business_interaction_threads (
            id TEXT PRIMARY KEY,
            contact_id TEXT NOT NULL,
            status TEXT NOT NULL,
            opportunity_status TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            control_mode TEXT NOT NULL DEFAULT 'ai',
            assigned_operator TEXT,
            is_paused INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY(contact_id) REFERENCES business_external_contacts(id)
        );

        CREATE TABLE IF NOT EXISTS business_message_records (
            id TEXT PRIMARY KEY,
            thread_id TEXT NOT NULL,
            sender TEXT NOT NULL,
            content TEXT NOT NULL,
            timestamp INTEGER NOT NULL,
            FOREIGN KEY(thread_id) REFERENCES business_interaction_threads(id)
        );

        CREATE TABLE IF NOT EXISTS business_opportunities (
            id TEXT PRIMARY KEY,
            contact_id TEXT NOT NULL,
            thread_id TEXT NOT NULL,
            status TEXT NOT NULL,
            value REAL,
            notes TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            pipeline_stage TEXT NOT NULL DEFAULT 'new',
            FOREIGN KEY(contact_id) REFERENCES business_external_contacts(id),
            FOREIGN KEY(thread_id) REFERENCES business_interaction_threads(id)
        );

        CREATE TABLE IF NOT EXISTS business_scheduled_engagements (
            id TEXT PRIMARY KEY,
            contact_id TEXT NOT NULL,
            thread_id TEXT NOT NULL,
            title TEXT NOT NULL,
            status TEXT NOT NULL,
            start_time INTEGER NOT NULL,
            end_time INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            FOREIGN KEY(contact_id) REFERENCES business_external_contacts(id),
            FOREIGN KEY(thread_id) REFERENCES business_interaction_threads(id)
        );

        CREATE TABLE IF NOT EXISTS business_operator_escalations (
            id TEXT PRIMARY KEY,
            thread_id TEXT NOT NULL,
            reason TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            FOREIGN KEY(thread_id) REFERENCES business_interaction_threads(id)
        );

        CREATE TABLE IF NOT EXISTS business_knowledge_documents (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            content TEXT NOT NULL,
            category TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS business_authorization_requests (
            id TEXT PRIMARY KEY,
            thread_id TEXT NOT NULL,
            engagement_id TEXT NOT NULL,
            proposed_by TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            FOREIGN KEY(thread_id) REFERENCES business_interaction_threads(id),
            FOREIGN KEY(engagement_id) REFERENCES business_scheduled_engagements(id)
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS business_knowledge_documents_fts USING fts5(
            title,
            content,
            category,
            content='business_knowledge_documents',
            content_rowid='rowid'
        );

        CREATE TRIGGER IF NOT EXISTS business_knowledge_docs_ai AFTER INSERT ON business_knowledge_documents BEGIN
            INSERT INTO business_knowledge_documents_fts(rowid, title, content, category)
            VALUES (new.rowid, new.title, new.content, new.category);
        END;

        CREATE TRIGGER IF NOT EXISTS business_knowledge_docs_ad AFTER DELETE ON business_knowledge_documents BEGIN
            INSERT INTO business_knowledge_documents_fts(business_knowledge_documents_fts, rowid, title, content, category)
            VALUES('delete', old.rowid, old.title, old.content, old.category);
        END;

        CREATE TRIGGER IF NOT EXISTS business_knowledge_docs_au AFTER UPDATE ON business_knowledge_documents BEGIN
            INSERT INTO business_knowledge_documents_fts(business_knowledge_documents_fts, rowid, title, content, category)
            VALUES('delete', old.rowid, old.title, old.content, old.category);
            INSERT INTO business_knowledge_documents_fts(rowid, title, content, category)
            VALUES (new.rowid, new.title, new.content, new.category);
        END;

        CREATE TABLE IF NOT EXISTS communication_events (
            id TEXT PRIMARY KEY,
            business_id TEXT NOT NULL,
            thread_id TEXT NOT NULL,
            connector_type TEXT NOT NULL,
            event_type TEXT NOT NULL,
            payload TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS business_opportunity_scores (
            opportunity_id TEXT PRIMARY KEY,
            intent_score INTEGER NOT NULL,
            urgency_score INTEGER NOT NULL,
            budget_score INTEGER NOT NULL,
            qualification_score INTEGER NOT NULL,
            total_score INTEGER NOT NULL,
            rating TEXT NOT NULL,
            updated_at INTEGER NOT NULL,
            FOREIGN KEY(opportunity_id) REFERENCES business_opportunities(id)
        );

        CREATE TABLE IF NOT EXISTS business_thread_objectives (
            thread_id TEXT PRIMARY KEY,
            objective TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            FOREIGN KEY(thread_id) REFERENCES business_interaction_threads(id)
        );

        CREATE TABLE IF NOT EXISTS business_operator_queue (
            thread_id TEXT PRIMARY KEY,
            customer_id TEXT NOT NULL,
            reason TEXT NOT NULL,
            priority TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            customer_summary TEXT,
            thread_summary TEXT,
            FOREIGN KEY(thread_id) REFERENCES business_interaction_threads(id)
        );

        CREATE TABLE IF NOT EXISTS business_kpi_metrics (
            metric_key TEXT PRIMARY KEY,
            metric_value INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS calendar_credentials (
            business_id TEXT NOT NULL,
            provider TEXT NOT NULL,
            access_token TEXT NOT NULL,
            refresh_token TEXT NOT NULL,
            expires_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            account_email TEXT,
            google_user_id TEXT,
            PRIMARY KEY(business_id, provider)
        );

        CREATE TABLE IF NOT EXISTS processed_whatsapp_events (
            event_id TEXT PRIMARY KEY,
            processed_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS business_owner_audit_trail (
            id TEXT PRIMARY KEY,
            action TEXT NOT NULL,
            target_id TEXT NOT NULL,
            performed_by TEXT NOT NULL,
            performed_at INTEGER NOT NULL,
            reason TEXT
        );

        CREATE TABLE IF NOT EXISTS business_response_suggestions (
            thread_id TEXT PRIMARY KEY,
            suggested_reply TEXT NOT NULL,
            confidence REAL NOT NULL,
            reasoning_summary TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS business_opportunity_pipeline_history (
            id TEXT PRIMARY KEY,
            opportunity_id TEXT NOT NULL,
            from_stage TEXT NOT NULL,
            to_stage TEXT NOT NULL,
            changed_by TEXT NOT NULL,
            changed_at INTEGER NOT NULL,
            reason TEXT
        );

        CREATE TABLE IF NOT EXISTS business_analytics_events (
            id TEXT PRIMARY KEY,
            event_type TEXT NOT NULL,
            thread_id TEXT,
            value REAL NOT NULL,
            timestamp INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS connection_instances (
            business_id TEXT NOT NULL,
            provider_id TEXT NOT NULL,
            configuration TEXT NOT NULL,
            PRIMARY KEY(business_id, provider_id)
        );

        CREATE TABLE IF NOT EXISTS connection_oauth_states (
            state TEXT PRIMARY KEY,
            nonce TEXT NOT NULL,
            business_id TEXT NOT NULL,
            expires_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS business_idempotency_keys (
            key TEXT PRIMARY KEY,
            status TEXT NOT NULL,
            response TEXT,
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS business_background_jobs (
            id TEXT PRIMARY KEY,
            job_type TEXT NOT NULL,
            payload TEXT NOT NULL,
            status TEXT NOT NULL,
            retries INTEGER NOT NULL DEFAULT 0,
            max_retries INTEGER NOT NULL,
            run_at INTEGER NOT NULL,
            last_error TEXT,
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS business_message_outbox (
            id TEXT PRIMARY KEY,
            business_id TEXT NOT NULL,
            thread_id TEXT NOT NULL,
            customer_id TEXT NOT NULL,
            channel TEXT NOT NULL,
            direction TEXT NOT NULL,
            body TEXT NOT NULL,
            status TEXT NOT NULL,
            retries INTEGER NOT NULL DEFAULT 0,
            max_retries INTEGER NOT NULL,
            run_at INTEGER NOT NULL,
            last_error TEXT,
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS business_event_store (
            id TEXT PRIMARY KEY,
            business_id TEXT NOT NULL DEFAULT 'system',
            event_type TEXT NOT NULL,
            aggregate_id TEXT NOT NULL,
            payload TEXT NOT NULL,
            timestamp INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS business_operational_supervisor_tasks (
            id TEXT PRIMARY KEY,
            business_id TEXT NOT NULL,
            title TEXT NOT NULL,
            affected_component TEXT NOT NULL,
            root_cause TEXT NOT NULL,
            severity TEXT NOT NULL,
            category TEXT NOT NULL,
            summary TEXT NOT NULL,
            recommended_action TEXT NOT NULL,
            status TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS supervisor_chat_messages (
            id TEXT PRIMARY KEY,
            business_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );
            "#,
            down: r#"
DROP TABLE IF EXISTS supervisor_chat_messages;
DROP TABLE IF EXISTS business_event_store;
DROP TABLE IF EXISTS business_message_outbox;
DROP TABLE IF EXISTS business_background_jobs;
DROP TABLE IF EXISTS business_idempotency_keys;
DROP TABLE IF EXISTS processed_whatsapp_events;
DROP TABLE IF EXISTS business_owner_audit_trail;
DROP TABLE IF EXISTS calendar_credentials;
DROP TABLE IF EXISTS business_kpi_metrics;
DROP TABLE IF EXISTS business_operator_queue;
DROP TABLE IF EXISTS business_thread_objectives;
DROP TABLE IF EXISTS business_opportunity_scores;
DROP TABLE IF EXISTS communication_events;
DROP TABLE IF EXISTS business_authorization_requests;
DROP TABLE IF EXISTS business_knowledge_documents_fts;
DROP TABLE IF EXISTS business_knowledge_documents;
DROP TABLE IF EXISTS business_operator_escalations;
DROP TABLE IF EXISTS business_scheduled_engagements;
DROP TABLE IF EXISTS business_opportunities;
DROP TABLE IF EXISTS business_message_records;
DROP TABLE IF EXISTS business_interaction_threads;
DROP TABLE IF EXISTS business_identity_mappings;
DROP TABLE IF EXISTS business_external_contacts;
DROP TABLE IF EXISTS business_response_suggestions;
DROP TABLE IF EXISTS business_opportunity_pipeline_history;
DROP TABLE IF EXISTS business_analytics_events;
DROP TABLE IF EXISTS connection_instances;
DROP TABLE IF EXISTS connection_oauth_states;
            "#,
        },
        Migration {
            version: 2,
            up: r#"
-- Projection indexes for faster reads
CREATE INDEX IF NOT EXISTS idx_proj_opp_status ON business_opportunities(status, value);
CREATE INDEX IF NOT EXISTS idx_proj_thread_contact ON business_interaction_threads(contact_id, status);
CREATE INDEX IF NOT EXISTS idx_proj_eng_thread ON business_scheduled_engagements(thread_id, status, start_time);
CREATE INDEX IF NOT EXISTS idx_proj_ident_contact ON business_identity_mappings(external_id, channel, contact_id);
            "#,
            down: r#"
DROP INDEX IF EXISTS idx_proj_opp_status;
DROP INDEX IF EXISTS idx_proj_thread_contact;
DROP INDEX IF EXISTS idx_proj_eng_thread;
DROP INDEX IF EXISTS idx_proj_ident_contact;
            "#,
        },
    ]
}

pub fn init_migrations_table(conn: &Connection) -> MoatResult<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at INTEGER NOT NULL
        )",
        [],
    )
    ?;
    Ok(())
}

pub fn get_current_version(conn: &Connection) -> MoatResult<i32> {
    init_migrations_table(conn)?;
    let mut stmt = conn
        .prepare("SELECT MAX(version) FROM schema_migrations")
        ?;
    let version: Option<i32> = stmt.query_row([], |row| row.get(0)).unwrap_or(None);
    Ok(version.unwrap_or(0))
}

pub fn upgrade(conn: &Connection) -> MoatResult<()> {
    let current_version = get_current_version(conn)?;
    let migrations = get_migrations();

    for m in migrations {
        if m.version > current_version {
            conn.execute_batch("BEGIN")?;
            conn.execute_batch(m.up)?;
            conn.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                rusqlite::params![m.version, chrono::Utc::now().timestamp()],
            )?;
            crate::business::event_store::record_event(
                conn,
                "StateMutated",
                "aggregate_id",
                &serde_json::json!({ "type": "LegacyMutationReplaced" }),
            )?;
            conn.execute_batch("COMMIT")?;
            println!("Applied migration version {}", m.version);
        }
    }
    Ok(())
}

pub fn rollback(conn: &Connection) -> MoatResult<()> {
    let current_version = get_current_version(conn)?;
    if current_version == 0 {
        return Ok(());
    }

    let migrations = get_migrations();
    if let Some(m) = migrations.iter().find(|m| m.version == current_version) {
        conn.execute_batch("BEGIN")?;
        conn.execute_batch(m.down)?;
        conn.execute(
            "DELETE FROM schema_migrations WHERE version = ?1",
            rusqlite::params![m.version],
        )?;
        crate::business::event_store::record_event(
            conn,
            "StateMutated",
            "aggregate_id",
            &serde_json::json!({ "type": "LegacyMutationReplaced" }),
        )?;
        conn.execute_batch("COMMIT")?;
        println!("Rolled back migration version {}", m.version);
    }
    Ok(())
}
