use crate::MoatError;
use crate::MoatResult;
use rusqlite::{params, Connection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdempotencyStatus {
    Acquired,
    InFlight,
    Completed { response: String },
}

pub fn try_acquire(conn: &Connection, key: &str) -> MoatResult<IdempotencyStatus> {
    let now = chrono::Utc::now().timestamp();

    let mut stmt =
        conn.prepare("SELECT status, response FROM business_idempotency_keys WHERE key = ?1")?;
    let mut rows = stmt.query(params![key])?;
    if let Some(row) = rows.next()? {
        let status: String = row.get(0)?;
        let response: Option<String> = row.get(1)?;
        match status.as_str() {
            "processing" => Ok(IdempotencyStatus::InFlight),
            "completed" => Ok(IdempotencyStatus::Completed {
                response: response.unwrap_or_default(),
            }),
            "failed" => {
                conn.execute(
                    "UPDATE business_idempotency_keys SET status = 'processing', response = NULL WHERE key = ?1",
                    params![key],
                )?;
                Ok(IdempotencyStatus::Acquired)
            }
            _ => Err(MoatError::InvariantViolation(format!(
                "Unknown idempotency status: {}",
                status
            ))),
        }
    } else {
                conn.execute(
                    "INSERT INTO business_idempotency_keys (key, status, created_at) VALUES (?1, 'processing', ?2)",
                    params![key, now],
                )?;
        Ok(IdempotencyStatus::Acquired)
    }
}

pub fn complete(conn: &Connection, key: &str, response: &str) -> MoatResult<()> {
    conn.execute(
        "UPDATE business_idempotency_keys SET status = 'completed', response = ?1 WHERE key = ?2",
        params![response, key],
    )?;
    Ok(())
}

pub fn fail(conn: &Connection, key: &str) -> MoatResult<()> {
    conn.execute(
        "UPDATE business_idempotency_keys SET status = 'failed' WHERE key = ?1",
        params![key],
    )?;
    Ok(())
}
