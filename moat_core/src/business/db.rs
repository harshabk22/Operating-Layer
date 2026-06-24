use super::models::{
    AuthorizationDetailResponse, AuthorizationRequest, AuthorizationStatus, BusinessIntegration,
    ContactIdentityMapping, EngagementStatus, ExternalContact, InteractionThread,
    KnowledgeDocument, MessageRecord, OperatorEscalation, Opportunity,
    OpportunityPipelineHistory, OpportunityStatus, OwnerAuditRecord, PipelineStage,
    ResponseSuggestion, ScheduledEngagement, ThreadControlMode, ThreadStatus,
};
use crate::MoatError;
use crate::MoatResult;
use rusqlite::{params, Connection};

pub fn prepare_tables(conn: &Connection) -> MoatResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS business_external_contacts (
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

        CREATE TABLE IF NOT EXISTS business_integrations (
            id TEXT PRIMARY KEY,
            provider TEXT UNIQUE NOT NULL,
            config TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
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

        CREATE TABLE IF NOT EXISTS connection_oauth_states (
            state TEXT PRIMARY KEY,
            nonce TEXT NOT NULL,
            business_id TEXT NOT NULL,
            expires_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS connection_instances (
            id TEXT PRIMARY KEY,
            business_id TEXT NOT NULL,
            provider_id TEXT NOT NULL,
            configuration TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
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
        );"
    ).map_err(MoatError::Database)?;

    let _ = conn.execute(
        "ALTER TABLE business_operator_queue ADD COLUMN customer_summary TEXT",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE business_operator_queue ADD COLUMN thread_summary TEXT",
        [],
    );

    // Phase 16 migrations
    let _ = conn.execute("ALTER TABLE business_interaction_threads ADD COLUMN control_mode TEXT NOT NULL DEFAULT 'ai'", []);
    let _ = conn.execute(
        "ALTER TABLE business_interaction_threads ADD COLUMN assigned_operator TEXT",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE business_interaction_threads ADD COLUMN is_paused INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE business_opportunities ADD COLUMN pipeline_stage TEXT NOT NULL DEFAULT 'new'",
        [],
    );

    // Phase 17 reliability schema upgrades
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS business_idempotency_keys (
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

        CREATE TABLE IF NOT EXISTS projections (
            id TEXT PRIMARY KEY,
            business_id TEXT NOT NULL,
            name TEXT NOT NULL,
            version INTEGER NOT NULL,
            timestamp INTEGER NOT NULL
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
        );",
    )
    .map_err(MoatError::Database)?;

    Ok(())
}

pub fn increment_kpi_metric(conn: &Connection, key: &str) -> MoatResult<()> {
    conn.execute(
        "INSERT INTO business_kpi_metrics (metric_key, metric_value) 
         VALUES (?1, 1) 
         ON CONFLICT(metric_key) DO UPDATE SET metric_value = metric_value + 1",
        params![key],
    )
    .map_err(MoatError::Database)?;
    Ok(())
}

pub fn get_kpi_metric(conn: &Connection, key: &str) -> MoatResult<u64> {
    let mut stmt = conn
        .prepare("SELECT metric_value FROM business_kpi_metrics WHERE metric_key = ?1")
        .map_err(MoatError::Database)?;
    let mut rows = stmt.query(params![key]).map_err(MoatError::Database)?;
    if let Some(row) = rows.next().map_err(MoatError::Database)? {
        let val: i64 = row.get(0).map_err(MoatError::Database)?;
        Ok(val as u64)
    } else {
        Ok(0)
    }
}

pub fn list_pending_authorization_requests(
    conn: &Connection,
) -> MoatResult<Vec<AuthorizationRequest>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, thread_id, engagement_id, proposed_by, status, created_at 
         FROM business_authorization_requests 
         WHERE status = 'pending' 
         ORDER BY created_at DESC",
        )
        .map_err(MoatError::Database)?;

    let mut rows = stmt.query([]).map_err(MoatError::Database)?;
    let mut requests = Vec::new();
    while let Some(row) = rows.next().map_err(MoatError::Database)? {
        let status_str: String = row.get::<_, String>(4).map_err(MoatError::Database)?;
        requests.push(AuthorizationRequest {
            id: row.get::<_, String>(0).map_err(MoatError::Database)?,
            thread_id: row.get::<_, String>(1).map_err(MoatError::Database)?,
            engagement_id: row.get::<_, String>(2).map_err(MoatError::Database)?,
            proposed_by: row.get::<_, String>(3).map_err(MoatError::Database)?,
            status: AuthorizationStatus::from_str(&status_str)
                .unwrap_or(AuthorizationStatus::Pending),
            created_at: row.get::<_, i64>(5).map_err(MoatError::Database)?,
        });
    }
    Ok(requests)
}

pub fn list_all_authorization_details(
    conn: &Connection,
) -> MoatResult<Vec<AuthorizationDetailResponse>> {
    let mut stmt = conn
        .prepare(
            "SELECT 
                a.id, a.thread_id, a.engagement_id, a.proposed_by, a.status, a.created_at,
                c.name, c.phone,
                e.title, e.start_time, e.end_time,
                aud.reason, aud.performed_by, aud.performed_at
             FROM business_authorization_requests a
             LEFT JOIN business_scheduled_engagements e ON a.engagement_id = e.id
             LEFT JOIN business_external_contacts c ON e.contact_id = c.id
             LEFT JOIN business_owner_audit_trail aud ON a.id = aud.target_id
             ORDER BY 
                CASE a.status 
                    WHEN 'pending' THEN 1 
                    ELSE 2 
                END ASC,
                a.created_at DESC",
        )
        .map_err(MoatError::Database)?;

    let mut rows = stmt.query([]).map_err(MoatError::Database)?;
    let mut details = Vec::new();
    while let Some(row) = rows.next().map_err(MoatError::Database)? {
        let status_str: String = row.get::<_, String>(4).map_err(MoatError::Database)?;
        details.push(AuthorizationDetailResponse {
            id: row.get::<_, String>(0).map_err(MoatError::Database)?,
            thread_id: row.get::<_, String>(1).map_err(MoatError::Database)?,
            engagement_id: row.get::<_, String>(2).map_err(MoatError::Database)?,
            proposed_by: row.get::<_, String>(3).map_err(MoatError::Database)?,
            status: AuthorizationStatus::from_str(&status_str)
                .unwrap_or(AuthorizationStatus::Pending),
            created_at: row.get::<_, i64>(5).map_err(MoatError::Database)?,
            contact_name: row.get::<_, Option<String>>(6).map_err(MoatError::Database)?,
            contact_phone: row.get::<_, Option<String>>(7).map_err(MoatError::Database)?,
            booking_title: row.get::<_, Option<String>>(8).map_err(MoatError::Database)?,
            booking_start_time: row.get::<_, Option<i64>>(9).map_err(MoatError::Database)?,
            booking_end_time: row.get::<_, Option<i64>>(10).map_err(MoatError::Database)?,
            decision_reason: row.get::<_, Option<String>>(11).map_err(MoatError::Database)?,
            decision_by: row.get::<_, Option<String>>(12).map_err(MoatError::Database)?,
            decision_at: row.get::<_, Option<i64>>(13).map_err(MoatError::Database)?,
        });
    }
    Ok(details)
}

fn get_encryption_key() -> String {
    std::env::var("MOAT_SECRET_KEY").unwrap_or_else(|_| "MoatAgeexSuperSecretKey123".to_string())
}

pub fn save_calendar_credentials(
    conn: &Connection,
    business_id: &str,
    provider: &str,
    access_token: &str,
    refresh_token: &str,
    expires_at: i64,
) -> MoatResult<()> {
    let key = get_encryption_key();
    let enc_access = crate::security::encryption::encrypt_credential(access_token, &key);
    let enc_refresh = crate::security::encryption::encrypt_credential(refresh_token, &key);
    let now = chrono::Utc::now().timestamp();

    conn.execute(
        "INSERT INTO calendar_credentials (business_id, provider, access_token, refresh_token, expires_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(business_id, provider) DO UPDATE SET
             access_token = excluded.access_token,
             refresh_token = excluded.refresh_token,
             expires_at = excluded.expires_at,
             updated_at = excluded.updated_at",
        params![business_id, provider, enc_access, enc_refresh, expires_at, now],
    ).map_err(MoatError::Database)?;
    Ok(())
}

pub fn load_calendar_credentials(
    conn: &Connection,
    business_id: &str,
    provider: &str,
) -> MoatResult<Option<(String, String, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT access_token, refresh_token, expires_at FROM calendar_credentials WHERE business_id = ?1 AND provider = ?2"
    ).map_err(MoatError::Database)?;

    let mut rows = stmt
        .query(params![business_id, provider])
        .map_err(MoatError::Database)?;
    if let Some(row) = rows.next().map_err(MoatError::Database)? {
        let enc_access: String = row.get(0).map_err(MoatError::Database)?;
        let enc_refresh: String = row.get(1).map_err(MoatError::Database)?;
        let expires_at: i64 = row.get(2).map_err(MoatError::Database)?;

        let key = get_encryption_key();
        let access =
            crate::security::encryption::decrypt_credential(&enc_access, &key).map_err(|e| {
                MoatError::SecurityViolation(format!("Failed to decrypt access token: {}", e))
            })?;
        let refresh =
            crate::security::encryption::decrypt_credential(&enc_refresh, &key).map_err(|e| {
                MoatError::SecurityViolation(format!("Failed to decrypt refresh token: {}", e))
            })?;

        Ok(Some((access, refresh, expires_at)))
    } else {
        Ok(None)
    }
}

pub fn is_whatsapp_event_processed(conn: &Connection, event_id: &str) -> MoatResult<bool> {
    let mut stmt = conn
        .prepare("SELECT 1 FROM processed_whatsapp_events WHERE event_id = ?1")
        .map_err(MoatError::Database)?;
    let exists = stmt
        .exists(params![event_id])
        .map_err(MoatError::Database)?;
    Ok(exists)
}

pub fn mark_whatsapp_event_processed(conn: &Connection, event_id: &str) -> MoatResult<()> {
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT OR IGNORE INTO processed_whatsapp_events (event_id, processed_at) VALUES (?1, ?2)",
        params![event_id, now],
    )
    .map_err(MoatError::Database)?;
    Ok(())
}

// Contacts CRUD
pub fn insert_contact(conn: &Connection, contact: &ExternalContact) -> MoatResult<()> {
    conn.execute(
        "INSERT INTO business_external_contacts (id, name, email, phone, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            contact.id,
            contact.name,
            contact.email,
            contact.phone,
            contact.created_at,
            contact.updated_at
        ],
    )
    .map_err(MoatError::Database)?;
    Ok(())
}

pub fn get_contact(conn: &Connection, id: &str) -> MoatResult<Option<ExternalContact>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, email, phone, created_at, updated_at FROM business_external_contacts WHERE id = ?1",
    ).map_err(MoatError::Database)?;

    let mut rows = stmt.query(params![id]).map_err(MoatError::Database)?;
    if let Some(row) = rows.next().map_err(MoatError::Database)? {
        Ok(Some(ExternalContact {
            id: row.get::<_, String>(0).map_err(MoatError::Database)?,
            name: row
                .get::<_, Option<String>>(1)
                .map_err(MoatError::Database)?,
            email: row
                .get::<_, Option<String>>(2)
                .map_err(MoatError::Database)?,
            phone: row
                .get::<_, Option<String>>(3)
                .map_err(MoatError::Database)?,
            created_at: row.get::<_, i64>(4).map_err(MoatError::Database)?,
            updated_at: row.get::<_, i64>(5).map_err(MoatError::Database)?,
        }))
    } else {
        Ok(None)
    }
}

// Identity Mappings CRUD
pub fn insert_identity_mapping(
    conn: &Connection,
    mapping: &ContactIdentityMapping,
) -> MoatResult<()> {
    conn.execute(
        "INSERT INTO business_identity_mappings (id, contact_id, channel, external_id)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            mapping.id,
            mapping.contact_id,
            mapping.channel,
            mapping.external_id
        ],
    )
    .map_err(MoatError::Database)?;
    Ok(())
}

pub fn get_contact_by_identity(
    conn: &Connection,
    channel: &str,
    external_id: &str,
) -> MoatResult<Option<ExternalContact>> {
    let mut stmt = conn
        .prepare(
            "SELECT c.id, c.name, c.email, c.phone, c.created_at, c.updated_at 
         FROM business_external_contacts c
         JOIN business_identity_mappings m ON c.id = m.contact_id
         WHERE m.channel = ?1 AND m.external_id = ?2",
        )
        .map_err(MoatError::Database)?;

    let mut rows = stmt
        .query(params![channel, external_id])
        .map_err(MoatError::Database)?;
    if let Some(row) = rows.next().map_err(MoatError::Database)? {
        Ok(Some(ExternalContact {
            id: row.get::<_, String>(0).map_err(MoatError::Database)?,
            name: row
                .get::<_, Option<String>>(1)
                .map_err(MoatError::Database)?,
            email: row
                .get::<_, Option<String>>(2)
                .map_err(MoatError::Database)?,
            phone: row
                .get::<_, Option<String>>(3)
                .map_err(MoatError::Database)?,
            created_at: row.get::<_, i64>(4).map_err(MoatError::Database)?,
            updated_at: row.get::<_, i64>(5).map_err(MoatError::Database)?,
        }))
    } else {
        Ok(None)
    }
}

// Interaction Threads CRUD
pub fn insert_thread(conn: &Connection, thread: &InteractionThread) -> MoatResult<()> {
    conn.execute(
        "INSERT INTO business_interaction_threads (id, contact_id, status, opportunity_status, created_at, updated_at, control_mode, assigned_operator, is_paused)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            thread.id,
            thread.contact_id,
            thread.status.as_str(),
            thread.opportunity_status.as_str(),
            thread.created_at,
            thread.updated_at,
            thread.control_mode.as_str(),
            thread.assigned_operator,
            if thread.is_paused { 1 } else { 0 }
        ],
    ).map_err(MoatError::Database)?;
    Ok(())
}

pub fn get_thread(conn: &Connection, id: &str) -> MoatResult<Option<InteractionThread>> {
    let mut stmt = conn.prepare(
        "SELECT id, contact_id, status, opportunity_status, created_at, updated_at, control_mode, assigned_operator, is_paused FROM business_interaction_threads WHERE id = ?1",
    ).map_err(MoatError::Database)?;

    let mut rows = stmt.query(params![id]).map_err(MoatError::Database)?;
    if let Some(row) = rows.next().map_err(MoatError::Database)? {
        let status_str: String = row.get::<_, String>(2).map_err(MoatError::Database)?;
        let opp_status_str: String = row.get::<_, String>(3).map_err(MoatError::Database)?;
        let control_mode_str: String = row.get::<_, String>(6).map_err(MoatError::Database)?;
        let assigned_operator: Option<String> = row
            .get::<_, Option<String>>(7)
            .map_err(MoatError::Database)?;
        let is_paused_val: i32 = row.get::<_, i32>(8).map_err(MoatError::Database)?;

        Ok(Some(InteractionThread {
            id: row.get::<_, String>(0).map_err(MoatError::Database)?,
            contact_id: row.get::<_, String>(1).map_err(MoatError::Database)?,
            status: ThreadStatus::from_str(&status_str).unwrap_or(ThreadStatus::Active),
            opportunity_status: OpportunityStatus::from_str(&opp_status_str)
                .unwrap_or(OpportunityStatus::Discovered),
            created_at: row.get::<_, i64>(4).map_err(MoatError::Database)?,
            updated_at: row.get::<_, i64>(5).map_err(MoatError::Database)?,
            control_mode: ThreadControlMode::from_str(&control_mode_str)
                .unwrap_or(ThreadControlMode::Ai),
            assigned_operator,
            is_paused: is_paused_val != 0,
        }))
    } else {
        Ok(None)
    }
}

pub fn update_thread_status(conn: &Connection, id: &str, status: ThreadStatus) -> MoatResult<()> {
    conn.execute(
        "UPDATE business_interaction_threads SET status = ?1, updated_at = unixepoch() WHERE id = ?2",
        params![status.as_str(), id],
    ).map_err(MoatError::Database)?;
    Ok(())
}

pub fn update_thread_opportunity_status(
    conn: &Connection,
    id: &str,
    status: OpportunityStatus,
) -> MoatResult<()> {
    conn.execute(
        "UPDATE business_interaction_threads SET opportunity_status = ?1, updated_at = unixepoch() WHERE id = ?2",
        params![status.as_str(), id],
    ).map_err(MoatError::Database)?;
    Ok(())
}

pub fn get_active_thread_by_contact(
    conn: &Connection,
    contact_id: &str,
) -> MoatResult<Option<InteractionThread>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, contact_id, status, opportunity_status, created_at, updated_at, control_mode, assigned_operator, is_paused 
         FROM business_interaction_threads 
         WHERE contact_id = ?1 AND status != 'closed' 
         ORDER BY updated_at DESC LIMIT 1",
        )
        .map_err(MoatError::Database)?;

    let mut rows = stmt
        .query(params![contact_id])
        .map_err(MoatError::Database)?;
    if let Some(row) = rows.next().map_err(MoatError::Database)? {
        let status_str: String = row.get::<_, String>(2).map_err(MoatError::Database)?;
        let opp_status_str: String = row.get::<_, String>(3).map_err(MoatError::Database)?;
        let control_mode_str: String = row.get::<_, String>(6).map_err(MoatError::Database)?;
        let assigned_operator: Option<String> = row
            .get::<_, Option<String>>(7)
            .map_err(MoatError::Database)?;
        let is_paused_val: i32 = row.get::<_, i32>(8).map_err(MoatError::Database)?;

        Ok(Some(InteractionThread {
            id: row.get::<_, String>(0).map_err(MoatError::Database)?,
            contact_id: row.get::<_, String>(1).map_err(MoatError::Database)?,
            status: ThreadStatus::from_str(&status_str).unwrap_or(ThreadStatus::Active),
            opportunity_status: OpportunityStatus::from_str(&opp_status_str)
                .unwrap_or(OpportunityStatus::Discovered),
            created_at: row.get::<_, i64>(4).map_err(MoatError::Database)?,
            updated_at: row.get::<_, i64>(5).map_err(MoatError::Database)?,
            control_mode: ThreadControlMode::from_str(&control_mode_str)
                .unwrap_or(ThreadControlMode::Ai),
            assigned_operator,
            is_paused: is_paused_val != 0,
        }))
    } else {
        Ok(None)
    }
}

// Message Records CRUD
pub fn insert_message(conn: &Connection, msg: &MessageRecord) -> MoatResult<()> {
    conn.execute(
        "INSERT INTO business_message_records (id, thread_id, sender, content, timestamp)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            msg.id,
            msg.thread_id,
            msg.sender,
            msg.content,
            msg.timestamp
        ],
    )
    .map_err(MoatError::Database)?;
    Ok(())
}

pub fn list_messages(conn: &Connection, thread_id: &str) -> MoatResult<Vec<MessageRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, thread_id, sender, content, timestamp FROM business_message_records WHERE thread_id = ?1 ORDER BY timestamp ASC",
    ).map_err(MoatError::Database)?;

    let rows = stmt
        .query_map(params![thread_id], |row| {
            Ok(MessageRecord {
                id: row.get::<_, String>(0)?,
                thread_id: row.get::<_, String>(1)?,
                sender: row.get::<_, String>(2)?,
                content: row.get::<_, String>(3)?,
                timestamp: row.get::<_, i64>(4)?,
            })
        })
        .map_err(MoatError::Database)?;

    let mut msgs = Vec::new();
    for r in rows {
        msgs.push(r.map_err(MoatError::Database)?);
    }
    Ok(msgs)
}

// Opportunities CRUD
pub fn insert_opportunity(conn: &Connection, opp: &Opportunity) -> MoatResult<()> {
    conn.execute(
        "INSERT INTO business_opportunities (id, contact_id, thread_id, status, value, notes, created_at, updated_at, pipeline_stage)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            opp.id,
            opp.contact_id,
            opp.thread_id,
            opp.status.as_str(),
            opp.value,
            opp.notes,
            opp.created_at,
            opp.updated_at,
            opp.pipeline_stage.as_str()
        ],
    ).map_err(MoatError::Database)?;
    Ok(())
}

pub fn get_opportunity_by_thread(
    conn: &Connection,
    thread_id: &str,
) -> MoatResult<Option<Opportunity>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, contact_id, thread_id, status, value, notes, created_at, updated_at, pipeline_stage 
         FROM business_opportunities WHERE thread_id = ?1",
        )
        .map_err(MoatError::Database)?;

    let mut rows = stmt
        .query(params![thread_id])
        .map_err(MoatError::Database)?;
    if let Some(row) = rows.next().map_err(MoatError::Database)? {
        let status_str: String = row.get::<_, String>(3).map_err(MoatError::Database)?;
        let pipeline_stage_str: String = row.get::<_, String>(8).map_err(MoatError::Database)?;
        Ok(Some(Opportunity {
            id: row.get::<_, String>(0).map_err(MoatError::Database)?,
            contact_id: row.get::<_, String>(1).map_err(MoatError::Database)?,
            thread_id: row.get::<_, String>(2).map_err(MoatError::Database)?,
            status: OpportunityStatus::from_str(&status_str)
                .unwrap_or(OpportunityStatus::Discovered),
            value: row.get::<_, Option<f64>>(4).map_err(MoatError::Database)?,
            notes: row
                .get::<_, Option<String>>(5)
                .map_err(MoatError::Database)?,
            created_at: row.get::<_, i64>(6).map_err(MoatError::Database)?,
            updated_at: row.get::<_, i64>(7).map_err(MoatError::Database)?,
            pipeline_stage: PipelineStage::from_str(&pipeline_stage_str)
                .unwrap_or(PipelineStage::New),
        }))
    } else {
        Ok(None)
    }
}

pub fn update_opportunity_status(
    conn: &Connection,
    thread_id: &str,
    status: OpportunityStatus,
) -> MoatResult<()> {
    conn.execute(
        "UPDATE business_opportunities SET status = ?1, updated_at = unixepoch() WHERE thread_id = ?2",
        params![status.as_str(), thread_id],
    ).map_err(MoatError::Database)?;
    Ok(())
}

// Scheduled Engagements CRUD
pub fn insert_engagement(conn: &Connection, eng: &ScheduledEngagement) -> MoatResult<()> {
    conn.execute(
        "INSERT INTO business_scheduled_engagements (id, contact_id, thread_id, title, status, start_time, end_time, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            eng.id,
            eng.contact_id,
            eng.thread_id,
            eng.title,
            eng.status.as_str(),
            eng.start_time,
            eng.end_time,
            eng.created_at,
            eng.updated_at
        ],
    ).map_err(MoatError::Database)?;
    Ok(())
}

pub fn get_engagement(conn: &Connection, id: &str) -> MoatResult<Option<ScheduledEngagement>> {
    let mut stmt = conn.prepare(
        "SELECT id, contact_id, thread_id, title, status, start_time, end_time, created_at, updated_at 
         FROM business_scheduled_engagements WHERE id = ?1",
    ).map_err(MoatError::Database)?;

    let mut rows = stmt.query(params![id]).map_err(MoatError::Database)?;
    if let Some(row) = rows.next().map_err(MoatError::Database)? {
        let status_str: String = row.get::<_, String>(4).map_err(MoatError::Database)?;
        Ok(Some(ScheduledEngagement {
            id: row.get::<_, String>(0).map_err(MoatError::Database)?,
            contact_id: row.get::<_, String>(1).map_err(MoatError::Database)?,
            thread_id: row.get::<_, String>(2).map_err(MoatError::Database)?,
            title: row.get::<_, String>(3).map_err(MoatError::Database)?,
            status: EngagementStatus::from_str(&status_str).unwrap_or(EngagementStatus::Proposed),
            start_time: row.get::<_, i64>(5).map_err(MoatError::Database)?,
            end_time: row.get::<_, i64>(6).map_err(MoatError::Database)?,
            created_at: row.get::<_, i64>(7).map_err(MoatError::Database)?,
            updated_at: row.get::<_, i64>(8).map_err(MoatError::Database)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn update_engagement_status(
    conn: &Connection,
    id: &str,
    status: EngagementStatus,
) -> MoatResult<()> {
    conn.execute(
        "UPDATE business_scheduled_engagements SET status = ?1, updated_at = unixepoch() WHERE id = ?2",
        params![status.as_str(), id],
    ).map_err(MoatError::Database)?;
    Ok(())
}

// Operator Escalations CRUD
pub fn insert_escalation(conn: &Connection, esc: &OperatorEscalation) -> MoatResult<()> {
    conn.execute(
        "INSERT INTO business_operator_escalations (id, thread_id, reason, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![esc.id, esc.thread_id, esc.reason, esc.status, esc.created_at, esc.updated_at],
    ).map_err(MoatError::Database)?;
    Ok(())
}

pub fn get_escalation(conn: &Connection, id: &str) -> MoatResult<Option<OperatorEscalation>> {
    let mut stmt = conn.prepare(
        "SELECT id, thread_id, reason, status, created_at, updated_at FROM business_operator_escalations WHERE id = ?1",
    ).map_err(MoatError::Database)?;

    let mut rows = stmt.query(params![id]).map_err(MoatError::Database)?;
    if let Some(row) = rows.next().map_err(MoatError::Database)? {
        Ok(Some(OperatorEscalation {
            id: row.get::<_, String>(0).map_err(MoatError::Database)?,
            thread_id: row.get::<_, String>(1).map_err(MoatError::Database)?,
            reason: row.get::<_, String>(2).map_err(MoatError::Database)?,
            status: row.get::<_, String>(3).map_err(MoatError::Database)?,
            created_at: row.get::<_, i64>(4).map_err(MoatError::Database)?,
            updated_at: row.get::<_, i64>(5).map_err(MoatError::Database)?,
        }))
    } else {
        Ok(None)
    }
}

// Business Integrations CRUD
pub fn insert_integration(conn: &Connection, integration: &BusinessIntegration) -> MoatResult<()> {
    conn.execute(
        "INSERT INTO business_integrations (id, provider, config, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            integration.id,
            integration.provider,
            integration.config,
            integration.created_at,
            integration.updated_at
        ],
    )
    .map_err(MoatError::Database)?;
    Ok(())
}

pub fn get_integration(
    conn: &Connection,
    provider: &str,
) -> MoatResult<Option<BusinessIntegration>> {
    let mut stmt = conn.prepare(
        "SELECT id, provider, config, created_at, updated_at FROM business_integrations WHERE provider = ?1",
    ).map_err(MoatError::Database)?;

    let mut rows = stmt.query(params![provider]).map_err(MoatError::Database)?;
    if let Some(row) = rows.next().map_err(MoatError::Database)? {
        Ok(Some(BusinessIntegration {
            id: row.get::<_, String>(0).map_err(MoatError::Database)?,
            provider: row.get::<_, String>(1).map_err(MoatError::Database)?,
            config: row.get::<_, String>(2).map_err(MoatError::Database)?,
            created_at: row.get::<_, i64>(3).map_err(MoatError::Database)?,
            updated_at: row.get::<_, i64>(4).map_err(MoatError::Database)?,
        }))
    } else {
        Ok(None)
    }
}

// Knowledge Documents CRUD
pub fn insert_knowledge_document(conn: &Connection, doc: &KnowledgeDocument) -> MoatResult<()> {
    conn.execute(
        "INSERT INTO business_knowledge_documents (id, title, content, category, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![doc.id, doc.title, doc.content, doc.category, doc.updated_at],
    )
    .map_err(MoatError::Database)?;
    Ok(())
}

pub fn search_knowledge_documents(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> MoatResult<Vec<KnowledgeDocument>> {
    let mut stmt = conn
        .prepare(
            "SELECT d.id, d.title, d.content, d.category, d.updated_at 
         FROM business_knowledge_documents d
         JOIN business_knowledge_documents_fts f ON d.rowid = f.rowid
         WHERE business_knowledge_documents_fts MATCH ?1 
         ORDER BY rank LIMIT ?2",
        )
        .map_err(MoatError::Database)?;

    let rows = stmt
        .query_map(params![query, limit as i64], |row| {
            Ok(KnowledgeDocument {
                id: row.get::<_, String>(0)?,
                title: row.get::<_, String>(1)?,
                content: row.get::<_, String>(2)?,
                category: row.get::<_, String>(3)?,
                updated_at: row.get::<_, i64>(4)?,
            })
        })
        .map_err(MoatError::Database)?;

    let mut docs = Vec::new();
    for r in rows {
        docs.push(r.map_err(MoatError::Database)?);
    }
    Ok(docs)
}

// Authorization Requests CRUD
pub fn insert_authorization_request(
    conn: &Connection,
    req: &AuthorizationRequest,
) -> MoatResult<()> {
    conn.execute(
        "INSERT INTO business_authorization_requests (id, thread_id, engagement_id, proposed_by, status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            req.id,
            req.thread_id,
            req.engagement_id,
            req.proposed_by,
            req.status.as_str(),
            req.created_at
        ],
    ).map_err(MoatError::Database)?;
    Ok(())
}

pub fn get_authorization_request(
    conn: &Connection,
    id: &str,
) -> MoatResult<Option<AuthorizationRequest>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, thread_id, engagement_id, proposed_by, status, created_at 
         FROM business_authorization_requests WHERE id = ?1",
        )
        .map_err(MoatError::Database)?;

    let mut rows = stmt.query(params![id]).map_err(MoatError::Database)?;
    if let Some(row) = rows.next().map_err(MoatError::Database)? {
        let status_str: String = row.get::<_, String>(4).map_err(MoatError::Database)?;
        Ok(Some(AuthorizationRequest {
            id: row.get::<_, String>(0).map_err(MoatError::Database)?,
            thread_id: row.get::<_, String>(1).map_err(MoatError::Database)?,
            engagement_id: row.get::<_, String>(2).map_err(MoatError::Database)?,
            proposed_by: row.get::<_, String>(3).map_err(MoatError::Database)?,
            status: AuthorizationStatus::from_str(&status_str)
                .unwrap_or(AuthorizationStatus::Pending),
            created_at: row.get::<_, i64>(5).map_err(MoatError::Database)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn update_authorization_status(
    conn: &Connection,
    id: &str,
    status: AuthorizationStatus,
) -> MoatResult<()> {
    conn.execute(
        "UPDATE business_authorization_requests SET status = ?1 WHERE id = ?2",
        params![status.as_str(), id],
    )
    .map_err(MoatError::Database)?;
    Ok(())
}

pub fn get_pending_authorization_by_engagement(
    conn: &Connection,
    engagement_id: &str,
) -> MoatResult<Option<AuthorizationRequest>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, thread_id, engagement_id, proposed_by, status, created_at 
         FROM business_authorization_requests 
         WHERE engagement_id = ?1 AND status = 'pending' 
         ORDER BY created_at DESC LIMIT 1",
        )
        .map_err(MoatError::Database)?;

    let mut rows = stmt
        .query(params![engagement_id])
        .map_err(MoatError::Database)?;
    if let Some(row) = rows.next().map_err(MoatError::Database)? {
        let status_str: String = row.get::<_, String>(4).map_err(MoatError::Database)?;
        Ok(Some(AuthorizationRequest {
            id: row.get::<_, String>(0).map_err(MoatError::Database)?,
            thread_id: row.get::<_, String>(1).map_err(MoatError::Database)?,
            engagement_id: row.get::<_, String>(2).map_err(MoatError::Database)?,
            proposed_by: row.get::<_, String>(3).map_err(MoatError::Database)?,
            status: AuthorizationStatus::from_str(&status_str)
                .unwrap_or(AuthorizationStatus::Pending),
            created_at: row.get::<_, i64>(5).map_err(MoatError::Database)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn search_knowledge_documents_ranked(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> MoatResult<Vec<(KnowledgeDocument, f64)>> {
    let mut stmt = conn
        .prepare(
            "SELECT d.id, d.title, d.content, d.category, d.updated_at, rank 
         FROM business_knowledge_documents d
         JOIN business_knowledge_documents_fts f ON d.rowid = f.rowid
         WHERE business_knowledge_documents_fts MATCH ?1 
         ORDER BY rank LIMIT ?2",
        )
        .map_err(MoatError::Database)?;

    let rows = stmt
        .query_map(params![query, limit as i64], |row| {
            let doc = KnowledgeDocument {
                id: row.get::<_, String>(0)?,
                title: row.get::<_, String>(1)?,
                content: row.get::<_, String>(2)?,
                category: row.get::<_, String>(3)?,
                updated_at: row.get::<_, i64>(4)?,
            };
            let rank: f64 = row.get(5)?;
            Ok((doc, rank))
        })
        .map_err(MoatError::Database)?;

    let mut docs = Vec::new();
    for r in rows {
        docs.push(r.map_err(MoatError::Database)?);
    }
    Ok(docs)
}

pub fn insert_or_update_opportunity_score(
    conn: &Connection,
    score: &super::models::OpportunityScore,
) -> MoatResult<()> {
    conn.execute(
        "INSERT INTO business_opportunity_scores (opportunity_id, intent_score, urgency_score, budget_score, qualification_score, total_score, rating, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(opportunity_id) DO UPDATE SET
           intent_score=excluded.intent_score,
           urgency_score=excluded.urgency_score,
           budget_score=excluded.budget_score,
           qualification_score=excluded.qualification_score,
           total_score=excluded.total_score,
           rating=excluded.rating,
           updated_at=excluded.updated_at",
        params![
            score.opportunity_id,
            score.intent_score,
            score.urgency_score,
            score.budget_score,
            score.qualification_score,
            score.total_score,
            score.rating.as_str(),
            score.updated_at
        ],
    ).map_err(MoatError::Database)?;
    Ok(())
}

pub fn get_opportunity_score(
    conn: &Connection,
    opp_id: &str,
) -> MoatResult<Option<super::models::OpportunityScore>> {
    let mut stmt = conn.prepare(
        "SELECT opportunity_id, intent_score, urgency_score, budget_score, qualification_score, total_score, rating, updated_at
         FROM business_opportunity_scores WHERE opportunity_id = ?1"
    ).map_err(MoatError::Database)?;
    let mut rows = stmt.query(params![opp_id]).map_err(MoatError::Database)?;
    if let Some(row) = rows.next().map_err(MoatError::Database)? {
        let rating_str: String = row.get(6).map_err(MoatError::Database)?;
        let rating = super::models::OpportunityRating::from_str(&rating_str).ok_or_else(|| {
            MoatError::Orchestrator(format!("Invalid rating string: {}", rating_str))
        })?;
        Ok(Some(super::models::OpportunityScore {
            opportunity_id: row.get(0).map_err(MoatError::Database)?,
            intent_score: row.get(1).map_err(MoatError::Database)?,
            urgency_score: row.get(2).map_err(MoatError::Database)?,
            budget_score: row.get(3).map_err(MoatError::Database)?,
            qualification_score: row.get(4).map_err(MoatError::Database)?,
            total_score: row.get(5).map_err(MoatError::Database)?,
            rating,
            updated_at: row.get(7).map_err(MoatError::Database)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn set_thread_objective(
    conn: &Connection,
    record: &super::models::ThreadObjectiveRecord,
) -> MoatResult<()> {
    conn.execute(
        "INSERT INTO business_thread_objectives (thread_id, objective, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(thread_id) DO UPDATE SET
           objective=excluded.objective,
           status=excluded.status,
           updated_at=excluded.updated_at",
        params![
            record.thread_id,
            record.objective.as_str(),
            record.status,
            record.created_at,
            record.updated_at
        ],
    ).map_err(MoatError::Database)?;
    Ok(())
}

pub fn get_thread_objective(
    conn: &Connection,
    thread_id: &str,
) -> MoatResult<Option<super::models::ThreadObjectiveRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT thread_id, objective, status, created_at, updated_at
         FROM business_thread_objectives WHERE thread_id = ?1",
        )
        .map_err(MoatError::Database)?;
    let mut rows = stmt
        .query(params![thread_id])
        .map_err(MoatError::Database)?;
    if let Some(row) = rows.next().map_err(MoatError::Database)? {
        let obj_str: String = row.get(1).map_err(MoatError::Database)?;
        let objective = super::models::ThreadObjective::from_str(&obj_str)
            .ok_or_else(|| MoatError::Orchestrator(format!("Invalid objective: {}", obj_str)))?;
        Ok(Some(super::models::ThreadObjectiveRecord {
            thread_id: row.get(0).map_err(MoatError::Database)?,
            objective,
            status: row.get(2).map_err(MoatError::Database)?,
            created_at: row.get(3).map_err(MoatError::Database)?,
            updated_at: row.get(4).map_err(MoatError::Database)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn add_to_operator_queue(
    conn: &Connection,
    item: &super::models::OperatorQueueItem,
) -> MoatResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO business_operator_queue (thread_id, customer_id, reason, priority, created_at, customer_summary, thread_summary)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            item.thread_id,
            item.customer_id,
            item.reason,
            item.priority,
            item.created_at,
            item.customer_summary,
            item.thread_summary
        ],
    ).map_err(MoatError::Database)?;
    Ok(())
}

pub fn remove_from_operator_queue(conn: &Connection, thread_id: &str) -> MoatResult<()> {
    conn.execute(
        "DELETE FROM business_operator_queue WHERE thread_id = ?1",
        params![thread_id],
    )
    .map_err(MoatError::Database)?;
    Ok(())
}

pub fn list_operator_queue(conn: &Connection) -> MoatResult<Vec<super::models::OperatorQueueItem>> {
    let mut stmt = conn.prepare(
        "SELECT thread_id, customer_id, reason, priority, created_at, customer_summary, thread_summary FROM business_operator_queue ORDER BY created_at DESC"
    ).map_err(MoatError::Database)?;
    let mut rows = stmt.query([]).map_err(MoatError::Database)?;
    let mut items = Vec::new();
    while let Some(row) = rows.next().map_err(MoatError::Database)? {
        items.push(super::models::OperatorQueueItem {
            thread_id: row.get(0).map_err(MoatError::Database)?,
            customer_id: row.get(1).map_err(MoatError::Database)?,
            reason: row.get(2).map_err(MoatError::Database)?,
            priority: row.get(3).map_err(MoatError::Database)?,
            created_at: row.get(4).map_err(MoatError::Database)?,
            customer_summary: row.get(5).map_err(MoatError::Database)?,
            thread_summary: row.get(6).map_err(MoatError::Database)?,
        });
    }
    Ok(items)
}

pub fn insert_owner_audit_record(
    conn: &Connection,
    record: &super::models::OwnerAuditRecord,
) -> MoatResult<()> {
    conn.execute(
        "INSERT INTO business_owner_audit_trail (id, action, target_id, performed_by, performed_at, reason)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            record.id,
            record.action,
            record.target_id,
            record.performed_by,
            record.performed_at,
            record.reason
        ],
    ).map_err(MoatError::Database)?;
    Ok(())
}

pub fn list_owner_audit_records(
    conn: &Connection,
) -> MoatResult<Vec<super::models::OwnerAuditRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, action, target_id, performed_by, performed_at, reason FROM business_owner_audit_trail ORDER BY performed_at DESC"
    ).map_err(MoatError::Database)?;
    let mut rows = stmt.query([]).map_err(MoatError::Database)?;
    let mut records = Vec::new();
    while let Some(row) = rows.next().map_err(MoatError::Database)? {
        records.push(super::models::OwnerAuditRecord {
            id: row.get(0).map_err(MoatError::Database)?,
            action: row.get(1).map_err(MoatError::Database)?,
            target_id: row.get(2).map_err(MoatError::Database)?,
            performed_by: row.get(3).map_err(MoatError::Database)?,
            performed_at: row.get(4).map_err(MoatError::Database)?,
            reason: row.get(5).map_err(MoatError::Database)?,
        });
    }
    Ok(records)
}

pub fn update_escalation_status(conn: &Connection, id: &str, status: &str) -> MoatResult<()> {
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "UPDATE business_operator_escalations SET status = ?1, updated_at = ?2 WHERE id = ?3",
        params![status, now, id],
    )
    .map_err(MoatError::Database)?;
    Ok(())
}

pub fn list_escalations(conn: &Connection) -> MoatResult<Vec<super::models::OperatorEscalation>> {
    let mut stmt = conn.prepare(
        "SELECT id, thread_id, reason, status, created_at, updated_at FROM business_operator_escalations ORDER BY created_at DESC"
    ).map_err(MoatError::Database)?;
    let mut rows = stmt.query([]).map_err(MoatError::Database)?;
    let mut items = Vec::new();
    while let Some(row) = rows.next().map_err(MoatError::Database)? {
        items.push(super::models::OperatorEscalation {
            id: row.get(0).map_err(MoatError::Database)?,
            thread_id: row.get(1).map_err(MoatError::Database)?,
            reason: row.get(2).map_err(MoatError::Database)?,
            status: row.get(3).map_err(MoatError::Database)?,
            created_at: row.get(4).map_err(MoatError::Database)?,
            updated_at: row.get(5).map_err(MoatError::Database)?,
        });
    }
    Ok(items)
}

pub fn list_recent_bookings(
    conn: &Connection,
) -> MoatResult<Vec<super::models::ScheduledEngagement>> {
    let mut stmt = conn.prepare(
        "SELECT id, contact_id, thread_id, title, status, start_time, end_time, created_at, updated_at 
         FROM business_scheduled_engagements ORDER BY created_at DESC"
    ).map_err(MoatError::Database)?;
    let mut rows = stmt.query([]).map_err(MoatError::Database)?;
    let mut items = Vec::new();
    while let Some(row) = rows.next().map_err(MoatError::Database)? {
        let status_str: String = row.get(4).map_err(MoatError::Database)?;
        let status = super::models::EngagementStatus::from_str(&status_str)
            .unwrap_or(super::models::EngagementStatus::Proposed);
        items.push(super::models::ScheduledEngagement {
            id: row.get(0).map_err(MoatError::Database)?,
            contact_id: row.get(1).map_err(MoatError::Database)?,
            thread_id: row.get(2).map_err(MoatError::Database)?,
            title: row.get(3).map_err(MoatError::Database)?,
            status,
            start_time: row.get(5).map_err(MoatError::Database)?,
            end_time: row.get(6).map_err(MoatError::Database)?,
            created_at: row.get(7).map_err(MoatError::Database)?,
            updated_at: row.get(8).map_err(MoatError::Database)?,
        });
    }
    Ok(items)
}

// Response Suggestions CRUD
pub fn insert_response_suggestion(
    conn: &Connection,
    suggestion: &ResponseSuggestion,
) -> MoatResult<()> {
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT INTO business_response_suggestions (thread_id, suggested_reply, confidence, reasoning_summary, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(thread_id) DO UPDATE SET
             suggested_reply = excluded.suggested_reply,
             confidence = excluded.confidence,
             reasoning_summary = excluded.reasoning_summary,
             created_at = excluded.created_at",
        params![
            suggestion.thread_id,
            suggestion.suggested_reply,
            suggestion.confidence,
            suggestion.reasoning_summary,
            now
        ],
    ).map_err(MoatError::Database)?;
    Ok(())
}

pub fn get_response_suggestion(
    conn: &Connection,
    thread_id: &str,
) -> MoatResult<Option<ResponseSuggestion>> {
    let mut stmt = conn.prepare(
        "SELECT thread_id, suggested_reply, confidence, reasoning_summary FROM business_response_suggestions WHERE thread_id = ?1",
    ).map_err(MoatError::Database)?;

    let mut rows = stmt
        .query(params![thread_id])
        .map_err(MoatError::Database)?;
    if let Some(row) = rows.next().map_err(MoatError::Database)? {
        Ok(Some(ResponseSuggestion {
            thread_id: row.get(0).map_err(MoatError::Database)?,
            suggested_reply: row.get(1).map_err(MoatError::Database)?,
            confidence: row.get(2).map_err(MoatError::Database)?,
            reasoning_summary: row.get(3).map_err(MoatError::Database)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn delete_response_suggestion(conn: &Connection, thread_id: &str) -> MoatResult<()> {
    conn.execute(
        "DELETE FROM business_response_suggestions WHERE thread_id = ?1",
        params![thread_id],
    )
    .map_err(MoatError::Database)?;
    Ok(())
}

// Opportunity Pipeline Helpers
pub fn update_opportunity_pipeline_stage(
    conn: &Connection,
    opportunity_id: &str,
    stage: PipelineStage,
) -> MoatResult<()> {
    conn.execute(
        "UPDATE business_opportunities SET pipeline_stage = ?1, updated_at = unixepoch() WHERE id = ?2",
        params![stage.as_str(), opportunity_id],
    ).map_err(MoatError::Database)?;
    Ok(())
}

pub fn insert_pipeline_history_record(
    conn: &Connection,
    record: &OpportunityPipelineHistory,
) -> MoatResult<()> {
    conn.execute(
        "INSERT INTO business_opportunity_pipeline_history (id, opportunity_id, from_stage, to_stage, changed_by, changed_at, reason)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            record.id,
            record.opportunity_id,
            record.from_stage.as_str(),
            record.to_stage.as_str(),
            record.changed_by,
            record.changed_at,
            record.reason
        ],
    ).map_err(MoatError::Database)?;
    Ok(())
}

pub fn list_pipeline_history(
    conn: &Connection,
    opportunity_id: &str,
) -> MoatResult<Vec<OpportunityPipelineHistory>> {
    let mut stmt = conn.prepare(
        "SELECT id, opportunity_id, from_stage, to_stage, changed_by, changed_at, reason 
         FROM business_opportunity_pipeline_history WHERE opportunity_id = ?1 ORDER BY changed_at ASC",
    ).map_err(MoatError::Database)?;

    let mut rows = stmt
        .query(params![opportunity_id])
        .map_err(MoatError::Database)?;
    let mut history = Vec::new();
    while let Some(row) = rows.next().map_err(MoatError::Database)? {
        let from_str: String = row.get(2).map_err(MoatError::Database)?;
        let to_str: String = row.get(3).map_err(MoatError::Database)?;
        history.push(OpportunityPipelineHistory {
            id: row.get(0).map_err(MoatError::Database)?,
            opportunity_id: row.get(1).map_err(MoatError::Database)?,
            from_stage: PipelineStage::from_str(&from_str).unwrap_or(PipelineStage::New),
            to_stage: PipelineStage::from_str(&to_str).unwrap_or(PipelineStage::New),
            changed_by: row.get(4).map_err(MoatError::Database)?,
            changed_at: row.get(5).map_err(MoatError::Database)?,
            reason: row.get(6).map_err(MoatError::Database)?,
        });
    }
    Ok(history)
}

// Conversation Intervention updates
pub fn update_thread_control_mode(
    conn: &Connection,
    thread_id: &str,
    mode: ThreadControlMode,
) -> MoatResult<()> {
    conn.execute(
        "UPDATE business_interaction_threads SET control_mode = ?1, updated_at = unixepoch() WHERE id = ?2",
        params![mode.as_str(), thread_id],
    ).map_err(MoatError::Database)?;
    Ok(())
}

pub fn update_thread_operator(
    conn: &Connection,
    thread_id: &str,
    operator: Option<&str>,
) -> MoatResult<()> {
    conn.execute(
        "UPDATE business_interaction_threads SET assigned_operator = ?1, updated_at = unixepoch() WHERE id = ?2",
        params![operator, thread_id],
    ).map_err(MoatError::Database)?;
    Ok(())
}

pub fn update_thread_paused_status(
    conn: &Connection,
    thread_id: &str,
    is_paused: bool,
) -> MoatResult<()> {
    conn.execute(
        "UPDATE business_interaction_threads SET is_paused = ?1, updated_at = unixepoch() WHERE id = ?2",
        params![if is_paused { 1 } else { 0 }, thread_id],
    ).map_err(MoatError::Database)?;
    Ok(())
}

// Analytics logging
pub fn insert_analytics_event(
    conn: &Connection,
    event_type: &str,
    thread_id: Option<&str>,
    value: f64,
) -> MoatResult<()> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT INTO business_analytics_events (id, event_type, thread_id, value, timestamp)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, event_type, thread_id, value, now],
    )
    .map_err(MoatError::Database)?;
    Ok(())
}

pub fn get_avg_analytics_metric(conn: &Connection, metric: &str, start: i64, end: i64) -> MoatResult<f64> {
    let mut sql = "SELECT AVG(value) FROM business_analytics_events WHERE event_type = ?1".to_string();
    let mut p: Vec<rusqlite::types::Value> = vec![metric.to_string().into()];
    if start > 0 {
        sql.push_str(" AND timestamp >= ?2");
        p.push(start.into());
    }
    if end > 0 {
        sql.push_str(&format!(" AND timestamp <= ?{}", p.len() + 1));
        p.push(end.into());
    }
    let avg: Option<f64> = conn.query_row(&sql, rusqlite::params_from_iter(p), |r| r.get(0)).map_err(MoatError::Database)?;
    Ok(avg.unwrap_or(0.0))
}

// Search queries
pub fn search_threads_db(conn: &Connection, query: &str) -> MoatResult<Vec<InteractionThread>> {
    let like_query = format!("%{}%", query);
    let mut stmt = conn.prepare(
        "SELECT t.id, t.contact_id, t.status, t.opportunity_status, t.created_at, t.updated_at, t.control_mode, t.assigned_operator, t.is_paused 
         FROM business_interaction_threads t
         LEFT JOIN business_external_contacts c ON t.contact_id = c.id
         WHERE t.id LIKE ?1 
            OR t.status LIKE ?1 
            OR t.opportunity_status LIKE ?1 
            OR t.control_mode LIKE ?1 
            OR c.name LIKE ?1 
            OR c.email LIKE ?1 
            OR c.phone LIKE ?1
         ORDER BY t.updated_at DESC",
    ).map_err(MoatError::Database)?;

    let mut rows = stmt
        .query(params![like_query])
        .map_err(MoatError::Database)?;
    let mut items = Vec::new();
    while let Some(row) = rows.next().map_err(MoatError::Database)? {
        let status_str: String = row.get::<_, String>(2).map_err(MoatError::Database)?;
        let opp_status_str: String = row.get::<_, String>(3).map_err(MoatError::Database)?;
        let control_mode_str: String = row.get::<_, String>(6).map_err(MoatError::Database)?;
        let assigned_operator: Option<String> = row
            .get::<_, Option<String>>(7)
            .map_err(MoatError::Database)?;
        let is_paused_val: i32 = row.get::<_, i32>(8).map_err(MoatError::Database)?;

        items.push(InteractionThread {
            id: row.get::<_, String>(0).map_err(MoatError::Database)?,
            contact_id: row.get::<_, String>(1).map_err(MoatError::Database)?,
            status: ThreadStatus::from_str(&status_str).unwrap_or(ThreadStatus::Active),
            opportunity_status: OpportunityStatus::from_str(&opp_status_str)
                .unwrap_or(OpportunityStatus::Discovered),
            created_at: row.get::<_, i64>(4).map_err(MoatError::Database)?,
            updated_at: row.get::<_, i64>(5).map_err(MoatError::Database)?,
            control_mode: ThreadControlMode::from_str(&control_mode_str)
                .unwrap_or(ThreadControlMode::Ai),
            assigned_operator,
            is_paused: is_paused_val != 0,
        });
    }
    Ok(items)
}

pub fn search_customers_db(conn: &Connection, query: &str) -> MoatResult<Vec<ExternalContact>> {
    let like_query = format!("%{}%", query);
    let mut stmt = conn
        .prepare(
            "SELECT id, name, email, phone, created_at, updated_at 
         FROM business_external_contacts
         WHERE id LIKE ?1 OR name LIKE ?1 OR email LIKE ?1 OR phone LIKE ?1
         ORDER BY name ASC",
        )
        .map_err(MoatError::Database)?;

    let mut rows = stmt
        .query(params![like_query])
        .map_err(MoatError::Database)?;
    let mut items = Vec::new();
    while let Some(row) = rows.next().map_err(MoatError::Database)? {
        items.push(ExternalContact {
            id: row.get::<_, String>(0).map_err(MoatError::Database)?,
            name: row
                .get::<_, Option<String>>(1)
                .map_err(MoatError::Database)?,
            email: row
                .get::<_, Option<String>>(2)
                .map_err(MoatError::Database)?,
            phone: row
                .get::<_, Option<String>>(3)
                .map_err(MoatError::Database)?,
            created_at: row.get::<_, i64>(4).map_err(MoatError::Database)?,
            updated_at: row.get::<_, i64>(5).map_err(MoatError::Database)?,
        });
    }
    Ok(items)
}

pub fn search_bookings_db(conn: &Connection, query: &str) -> MoatResult<Vec<ScheduledEngagement>> {
    let like_query = format!("%{}%", query);
    let mut stmt = conn.prepare(
        "SELECT id, contact_id, thread_id, title, status, start_time, end_time, created_at, updated_at 
         FROM business_scheduled_engagements
         WHERE id LIKE ?1 OR title LIKE ?1 OR status LIKE ?1
         ORDER BY created_at DESC",
    ).map_err(MoatError::Database)?;

    let mut rows = stmt
        .query(params![like_query])
        .map_err(MoatError::Database)?;
    let mut items = Vec::new();
    while let Some(row) = rows.next().map_err(MoatError::Database)? {
        let status_str: String = row.get(4).map_err(MoatError::Database)?;
        let status = super::models::EngagementStatus::from_str(&status_str)
            .unwrap_or(super::models::EngagementStatus::Proposed);
        items.push(ScheduledEngagement {
            id: row.get(0).map_err(MoatError::Database)?,
            contact_id: row.get(1).map_err(MoatError::Database)?,
            thread_id: row.get(2).map_err(MoatError::Database)?,
            title: row.get(3).map_err(MoatError::Database)?,
            status,
            start_time: row.get(5).map_err(MoatError::Database)?,
            end_time: row.get(6).map_err(MoatError::Database)?,
            created_at: row.get(7).map_err(MoatError::Database)?,
            updated_at: row.get(8).map_err(MoatError::Database)?,
        });
    }
    Ok(items)
}

pub fn search_authorizations_db(
    conn: &Connection,
    query: &str,
) -> MoatResult<Vec<AuthorizationRequest>> {
    let like_query = format!("%{}%", query);
    let mut stmt = conn
        .prepare(
            "SELECT id, thread_id, engagement_id, proposed_by, status, created_at 
         FROM business_authorization_requests
         WHERE id LIKE ?1 OR thread_id LIKE ?1 OR proposed_by LIKE ?1 OR status LIKE ?1
         ORDER BY created_at DESC",
        )
        .map_err(MoatError::Database)?;

    let mut rows = stmt
        .query(params![like_query])
        .map_err(MoatError::Database)?;
    let mut items = Vec::new();
    while let Some(row) = rows.next().map_err(MoatError::Database)? {
        let status_str: String = row.get::<_, String>(4).map_err(MoatError::Database)?;
        items.push(AuthorizationRequest {
            id: row.get::<_, String>(0).map_err(MoatError::Database)?,
            thread_id: row.get::<_, String>(1).map_err(MoatError::Database)?,
            engagement_id: row.get::<_, String>(2).map_err(MoatError::Database)?,
            proposed_by: row.get::<_, String>(3).map_err(MoatError::Database)?,
            status: AuthorizationStatus::from_str(&status_str)
                .unwrap_or(AuthorizationStatus::Pending),
            created_at: row.get::<_, i64>(5).map_err(MoatError::Database)?,
        });
    }
    Ok(items)
}

pub fn search_audit_events_db(conn: &Connection, query: &str) -> MoatResult<Vec<OwnerAuditRecord>> {
    let like_query = format!("%{}%", query);
    let mut stmt = conn.prepare(
        "SELECT id, action, target_id, performed_by, performed_at, reason 
         FROM business_owner_audit_trail
         WHERE id LIKE ?1 OR action LIKE ?1 OR target_id LIKE ?1 OR performed_by LIKE ?1 OR reason LIKE ?1
         ORDER BY performed_at DESC",
    ).map_err(MoatError::Database)?;

    let mut rows = stmt
        .query(params![like_query])
        .map_err(MoatError::Database)?;
    let mut items = Vec::new();
    while let Some(row) = rows.next().map_err(MoatError::Database)? {
        items.push(OwnerAuditRecord {
            id: row.get(0).map_err(MoatError::Database)?,
            action: row.get(1).map_err(MoatError::Database)?,
            target_id: row.get(2).map_err(MoatError::Database)?,
            performed_by: row.get(3).map_err(MoatError::Database)?,
            performed_at: row.get(4).map_err(MoatError::Database)?,
            reason: row.get(5).map_err(MoatError::Database)?,
        });
    }
    Ok(items)
}

pub fn list_opportunities(conn: &Connection) -> MoatResult<Vec<Opportunity>> {
    let mut stmt = conn.prepare(
        "SELECT id, contact_id, thread_id, status, value, notes, created_at, updated_at, pipeline_stage 
         FROM business_opportunities ORDER BY created_at DESC"
    ).map_err(MoatError::Database)?;
    let mut rows = stmt.query([]).map_err(MoatError::Database)?;
    let mut items = Vec::new();
    while let Some(row) = rows.next().map_err(MoatError::Database)? {
        let status_str: String = row.get::<_, String>(3).map_err(MoatError::Database)?;
        let pipeline_stage_str: String = row.get::<_, String>(8).map_err(MoatError::Database)?;
        items.push(Opportunity {
            id: row.get::<_, String>(0).map_err(MoatError::Database)?,
            contact_id: row.get::<_, String>(1).map_err(MoatError::Database)?,
            thread_id: row.get::<_, String>(2).map_err(MoatError::Database)?,
            status: OpportunityStatus::from_str(&status_str)
                .unwrap_or(OpportunityStatus::Discovered),
            value: row.get::<_, Option<f64>>(4).map_err(MoatError::Database)?,
            notes: row
                .get::<_, Option<String>>(5)
                .map_err(MoatError::Database)?,
            created_at: row.get::<_, i64>(6).map_err(MoatError::Database)?,
            updated_at: row.get::<_, i64>(7).map_err(MoatError::Database)?,
            pipeline_stage: PipelineStage::from_str(&pipeline_stage_str)
                .unwrap_or(PipelineStage::New),
        });
    }
    Ok(items)
}

pub fn list_active_threads(conn: &Connection) -> MoatResult<Vec<InteractionThread>> {
    let mut stmt = conn.prepare(
        "SELECT id, contact_id, status, opportunity_status, created_at, updated_at, control_mode, assigned_operator, is_paused 
         FROM business_interaction_threads 
         WHERE status IN ('active', 'awaiting_authorization', 'escalated') 
         ORDER BY updated_at DESC"
    ).map_err(MoatError::Database)?;
    let mut rows = stmt.query([]).map_err(MoatError::Database)?;
    let mut items = Vec::new();
    while let Some(row) = rows.next().map_err(MoatError::Database)? {
        let status_str: String = row.get::<_, String>(2).map_err(MoatError::Database)?;
        let opp_status_str: String = row.get::<_, String>(3).map_err(MoatError::Database)?;
        let control_mode_str: String = row.get::<_, String>(6).map_err(MoatError::Database)?;
        let assigned_operator: Option<String> = row
            .get::<_, Option<String>>(7)
            .map_err(MoatError::Database)?;
        let is_paused_val: i32 = row.get::<_, i32>(8).map_err(MoatError::Database)?;

        items.push(InteractionThread {
            id: row.get::<_, String>(0).map_err(MoatError::Database)?,
            contact_id: row.get::<_, String>(1).map_err(MoatError::Database)?,
            status: ThreadStatus::from_str(&status_str).unwrap_or(ThreadStatus::Active),
            opportunity_status: OpportunityStatus::from_str(&opp_status_str)
                .unwrap_or(OpportunityStatus::Discovered),
            created_at: row.get::<_, i64>(4).map_err(MoatError::Database)?,
            updated_at: row.get::<_, i64>(5).map_err(MoatError::Database)?,
            control_mode: ThreadControlMode::from_str(&control_mode_str)
                .unwrap_or(ThreadControlMode::Ai),
            assigned_operator,
            is_paused: is_paused_val != 0,
        });
    }
    Ok(items)
}

pub fn transition_opportunity_stage(
    conn: &Connection,
    notifier: &dyn super::owner::NotificationProvider,
    opportunity_id: &str,
    to_stage: PipelineStage,
    changed_by: &str,
    reason: Option<&str>,
) -> MoatResult<()> {
    // 1. Get current opportunity to check current stage
    let mut stmt = conn
        .prepare("SELECT pipeline_stage FROM business_opportunities WHERE id = ?1")
        .map_err(MoatError::Database)?;
    let mut rows = stmt
        .query(params![opportunity_id])
        .map_err(MoatError::Database)?;
    let from_stage = if let Some(row) = rows.next().map_err(MoatError::Database)? {
        let stage_str: String = row.get(0).map_err(MoatError::Database)?;
        PipelineStage::from_str(&stage_str).unwrap_or(PipelineStage::New)
    } else {
        PipelineStage::New
    };

    if from_stage == to_stage {
        return Ok(());
    }

    // 2. Update pipeline_stage on opportunity
    conn.execute(
        "UPDATE business_opportunities SET pipeline_stage = ?1, updated_at = unixepoch() WHERE id = ?2",
        params![to_stage.as_str(), opportunity_id],
    ).map_err(MoatError::Database)?;

    // 3. Log history
    let record = OpportunityPipelineHistory {
        id: uuid::Uuid::new_v4().to_string(),
        opportunity_id: opportunity_id.to_string(),
        from_stage,
        to_stage,
        changed_by: changed_by.to_string(),
        changed_at: chrono::Utc::now().timestamp(),
        reason: reason.map(|r| r.to_string()),
    };
    insert_pipeline_history_record(conn, &record)?;

    // 4. Notify
    let _ = notifier.notify_pipeline_stage_change(
        opportunity_id,
        from_stage.as_str(),
        to_stage.as_str(),
    );

    // 5. Record event in event store
    let event_payload = serde_json::json!({
        "opportunity_id": opportunity_id,
        "from_stage": from_stage.as_str(),
        "to_stage": to_stage.as_str(),
        "changed_by": changed_by,
        "reason": reason
    });
    let _ = crate::business::event_store::record_event(
        conn,
        "pipeline_transitioned",
        opportunity_id,
        &event_payload,
    );

    Ok(())
}



pub fn save_oauth_state(
    conn: &rusqlite::Connection,
    state: &str,
    nonce: &str,
    business_id: &str,
    expires_at: i64,
) -> crate::MoatResult<()> {
    conn.execute(
        "INSERT INTO connection_oauth_states (state, nonce, business_id, expires_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![state, nonce, business_id, expires_at],
    )?;
    Ok(())
}

pub fn consume_oauth_state(
    conn: &rusqlite::Connection,
    state: &str,
) -> crate::MoatResult<Option<(String, String)>> {
    let mut stmt = conn
        .prepare(
            "SELECT nonce, business_id, expires_at FROM connection_oauth_states WHERE state = ?1",
        )?;
    let mut rows = stmt.query(rusqlite::params![state])?;
    if let Some(row) = rows.next()? {
        let nonce: String = row.get(0)?;
        let business_id: String = row.get(1)?;
        let expires_at: i64 = row.get(2)?;
        let now = chrono::Utc::now().timestamp();
        if expires_at < now {
            return Ok(None);
        }
        return Ok(Some((nonce, business_id)));
    }
    Ok(None)
}

pub fn purge_expired_oauth_states(conn: &rusqlite::Connection) -> crate::MoatResult<usize> {
    let now = chrono::Utc::now().timestamp();
    let count = conn.execute("DELETE FROM connection_oauth_states WHERE expires_at < ?1", rusqlite::params![now])?;
    Ok(count)
}

pub fn update_calendar_credentials_identity(
    conn: &rusqlite::Connection,
    business_id: &str,
    provider: &str,
    account_email: &str,
    google_user_id: &str,
) -> crate::MoatResult<()> {
    conn.execute(
        "UPDATE calendar_credentials SET account_email = ?1, google_user_id = ?2 WHERE business_id = ?3 AND provider = ?4",
        rusqlite::params![account_email, google_user_id, business_id, provider],
    )?;
    Ok(())
}

pub fn get_calendar_credentials_identity(
    conn: &rusqlite::Connection,
    business_id: &str,
    provider: &str,
) -> crate::MoatResult<Option<(String, String)>> {
    let mut stmt = conn.prepare("SELECT account_email, google_user_id FROM calendar_credentials WHERE business_id = ?1 AND provider = ?2")?;
    let mut rows = stmt.query(rusqlite::params![business_id, provider])?;
    if let Some(row) = rows.next()? {
        let email: Option<String> = row.get(0)?;
        let user_id: Option<String> = row.get(1)?;
        if let (Some(e), Some(u)) = (email, user_id) {
            return Ok(Some((e, u)));
        }
    }
    Ok(None)
}

pub fn delete_calendar_credentials(
    conn: &rusqlite::Connection,
    business_id: &str,
    provider: &str,
) -> crate::MoatResult<()> {
    conn.execute("DELETE FROM calendar_credentials WHERE business_id = ?1 AND provider = ?2", rusqlite::params![business_id, provider])?;
    Ok(())
}

#[derive(serde::Deserialize, serde::Serialize, Default)]
pub struct GoogleWorkspaceConfig {
    pub client_id: Option<String>,
    pub selected_spreadsheet_id: Option<String>,
}

pub fn read_google_config(
    conn: &rusqlite::Connection,
    business_id: &str,
) -> crate::MoatResult<GoogleWorkspaceConfig> {
    let mut stmt = conn.prepare("SELECT configuration FROM connection_instances WHERE business_id = ? AND provider_id = 'google'")?;
    let mut rows = stmt.query([business_id])?;
    if let Some(row) = rows.next()? {
        let config_str: String = row.get(0)?;
        let key = get_encryption_key();
        
        // Attempt decryption
        let decrypted_str = crate::security::encryption::decrypt_credential(&config_str, &key);
        
        let parse_target = match decrypted_str {
            Ok(ref decrypted) => decrypted,
            Err(_) => &config_str,
        };
        
        match serde_json::from_str(parse_target) {
            Ok(config) => return Ok(config),
            Err(e) => {
                if parse_target != &config_str {
                    if let Ok(config) = serde_json::from_str(&config_str) {
                        return Ok(config);
                    }
                }
                println!("JSON error in read_google_config: {}", e);
            }
        }
    } else {
        println!("No row found in connection_instances for biz {}", business_id);
    }
    Ok(GoogleWorkspaceConfig::default())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OperationalSupervisorTask {
    pub id: String,
    pub business_id: String,
    pub title: String,
    pub affected_component: String,
    pub root_cause: String,
    pub severity: String,
    pub category: String,
    pub summary: String,
    pub recommended_action: String,
    pub status: String,
}

pub fn get_pending_operational_supervisor_tasks(
    conn: &rusqlite::Connection,
    limit: i64,
) -> crate::MoatResult<Vec<OperationalSupervisorTask>> {
    let mut stmt = conn.prepare("SELECT id, business_id, title, affected_component, root_cause, severity, category, summary, recommended_action, status FROM business_operational_supervisor_tasks WHERE status = 'pending' LIMIT ?")?;
    let rows = stmt.query_map([limit], |row| {
        Ok(OperationalSupervisorTask {
            id: row.get(0)?,
            business_id: row.get(1)?,
            title: row.get(2)?,
            affected_component: row.get(3)?,
            root_cause: row.get(4)?,
            severity: row.get(5)?,
            category: row.get(6)?,
            summary: row.get(7)?,
            recommended_action: row.get(8)?,
            status: row.get(9)?,
        })
    })?;
    let mut tasks = Vec::new();
    for t in rows.flatten() {
        tasks.push(t);
    }
    Ok(tasks)
}

pub fn get_pending_operational_supervisor_tasks_by_business(
    conn: &rusqlite::Connection,
    business_id: &str,
    limit: i64,
) -> crate::MoatResult<Vec<OperationalSupervisorTask>> {
    let mut stmt = conn.prepare("SELECT id, business_id, title, affected_component, root_cause, severity, category, summary, recommended_action, status FROM business_operational_supervisor_tasks WHERE business_id = ?1 AND status = 'pending' LIMIT ?2")?;
    let rows = stmt.query_map(rusqlite::params![business_id, limit], |row| {
        Ok(OperationalSupervisorTask {
            id: row.get(0)?,
            business_id: row.get(1)?,
            title: row.get(2)?,
            affected_component: row.get(3)?,
            root_cause: row.get(4)?,
            severity: row.get(5)?,
            category: row.get(6)?,
            summary: row.get(7)?,
            recommended_action: row.get(8)?,
            status: row.get(9)?,
        })
    })?;
    let mut tasks = Vec::new();
    for t in rows.flatten() {
        tasks.push(t);
    }
    Ok(tasks)
}

pub fn update_operational_supervisor_task_status(
    conn: &rusqlite::Connection,
    task_id: &str,
    status: &str,
) -> crate::MoatResult<()> {
    conn.execute(
        "UPDATE business_operational_supervisor_tasks SET status = ?1 WHERE id = ?2",
        rusqlite::params![status, task_id],
    )?;
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SupervisorChatMessage {
    pub id: String,
    pub business_id: String,
    pub role: String,
    pub content: String,
    pub created_at: i64,
}

pub fn get_supervisor_chat_messages(
    conn: &rusqlite::Connection,
    business_id: &str,
) -> crate::MoatResult<Vec<SupervisorChatMessage>> {
    let mut stmt = conn.prepare("SELECT id, business_id, role, content, created_at FROM supervisor_chat_messages WHERE business_id = ?1 ORDER BY created_at ASC")?;
    let rows = stmt.query_map(rusqlite::params![business_id], |row| {
        Ok(SupervisorChatMessage {
            id: row.get(0)?,
            business_id: row.get(1)?,
            role: row.get(2)?,
            content: row.get(3)?,
            created_at: row.get(4)?,
        })
    })?;
    let mut msgs = Vec::new();
    for m in rows.flatten() {
        msgs.push(m);
    }
    Ok(msgs)
}

pub fn insert_supervisor_chat_message(
    conn: &rusqlite::Connection,
    msg: &SupervisorChatMessage,
) -> crate::MoatResult<()> {
    conn.execute(
        "INSERT INTO supervisor_chat_messages (id, business_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![msg.id, msg.business_id, msg.role, msg.content, msg.created_at],
    )?;
    Ok(())
}

