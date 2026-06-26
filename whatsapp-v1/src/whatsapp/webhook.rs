use moat_core::agent::execution::journal_chain::hmac_sha256;
use moat_core::business::runtime::CustomerInteractionRuntime;
use rusqlite::Connection;
use serde_json::Value;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

pub struct WhatsAppWebhookServer {
    runtime: Arc<CustomerInteractionRuntime>,
    db: Arc<Mutex<Connection>>,
    verify_token: String,
    client_secret: String,
}

impl WhatsAppWebhookServer {
    pub fn new(
        runtime: Arc<CustomerInteractionRuntime>,
        db: Arc<Mutex<Connection>>,
        verify_token: String,
        client_secret: String,
    ) -> Self {
        Self {
            runtime,
            db,
            verify_token,
            client_secret,
        }
    }

    pub fn start(self: Arc<Self>, addr: String) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let listener = match TcpListener::bind(&addr).await {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("[WhatsAppWebhookServer] Failed to bind to {}: {}", addr, e);
                    return;
                }
            };

            loop {
                let (mut stream, _) = match listener.accept().await {
                    Ok(val) => val,
                    Err(_) => continue,
                };

                let this = self.clone();
                tokio::spawn(async move {
                    if let Err(e) = this.handle_connection(&mut stream).await {
                        eprintln!("[WhatsAppWebhookServer] Error handling connection: {}", e);
                    }
                });
            }
        })
    }

    async fn handle_connection(&self, stream: &mut tokio::net::TcpStream) -> Result<(), String> {
        let req = parse_http_request(stream).await?;

        if req.path.contains("/webhook") {
            if req.method == "GET" {
                let params = parse_query_params(&req.query);
                let mode = params.get("hub.mode").map(|s| s.as_str()).unwrap_or("");
                let token = params
                    .get("hub.verify_token")
                    .map(|s| s.as_str())
                    .unwrap_or("");
                let challenge = params
                    .get("hub.challenge")
                    .map(|s| s.as_str())
                    .unwrap_or("");

                if mode == "subscribe" && token == self.verify_token {
                    let response_body = challenge;
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                        response_body.len(),
                        response_body
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                    return Ok(());
                } else {
                    let response = "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n";
                    let _ = stream.write_all(response.as_bytes()).await;
                    return Ok(());
                }
            } else if req.method == "POST" {
                let sig_header = req
                    .headers
                    .get("x-hub-signature-256")
                    .cloned()
                    .unwrap_or_default();
                if !validate_signature(&req.body, &sig_header, &self.client_secret) {
                    let response = "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n";
                    let _ = stream.write_all(response.as_bytes()).await;
                    return Ok(());
                }

                let payload_str = String::from_utf8_lossy(&req.body);
                let payload: Value = match serde_json::from_str(&payload_str) {
                    Ok(p) => p,
                    Err(_) => {
                        let response = "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n";
                        let _ = stream.write_all(response.as_bytes()).await;
                        return Ok(());
                    }
                };

                if let Some(entry_arr) = payload.get("entry").and_then(|e| e.as_array()) {
                    for entry in entry_arr {
                        if let Some(changes) = entry.get("changes").and_then(|c| c.as_array()) {
                            for change in changes {
                                if let Some(val) = change.get("value") {
                                    if let Some(messages) =
                                        val.get("messages").and_then(|m| m.as_array())
                                    {
                                        for msg in messages {
                                            let msg_id = msg
                                                .get("id")
                                                .and_then(|i| i.as_str())
                                                .unwrap_or_default()
                                                .to_string();
                                            let from = msg
                                                .get("from")
                                                .and_then(|f| f.as_str())
                                                .unwrap_or_default()
                                                .to_string();
                                            let body = msg
                                                .get("text")
                                                .and_then(|t| t.get("body"))
                                                .and_then(|b| b.as_str())
                                                .unwrap_or_default()
                                                .to_string();

                                            if msg_id.is_empty() || from.is_empty() {
                                                continue;
                                            }

                                            let idempotency_key = format!("whatsapp:{}", msg_id);
                                            let status = {
                                                let conn = self.db.lock().await;
                                                moat_core::business::reliability::idempotency::try_acquire(&conn, &idempotency_key)
                                                    .unwrap_or(moat_core::business::reliability::idempotency::IdempotencyStatus::InFlight)
                                            };
                                            if let moat_core::business::reliability::idempotency::IdempotencyStatus::Completed { response } = &status {
                                                let http_resp = format!(
                                                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                                                    response.len(),
                                                    response
                                                );
                                                let _ = stream.write_all(http_resp.as_bytes()).await;
                                                return Ok(());
                                            }
                                            if status == moat_core::business::reliability::idempotency::IdempotencyStatus::InFlight {
                                                let response = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
                                                let _ = stream.write_all(response.as_bytes()).await;
                                                return Ok(());
                                            }

                                            let mut profile_name = "WhatsApp User".to_string();
                                            if let Some(contacts) =
                                                val.get("contacts").and_then(|c| c.as_array())
                                            {
                                                if let Some(contact) = contacts.first() {
                                                    if let Some(name) = contact
                                                        .get("profile")
                                                        .and_then(|p| p.get("name"))
                                                        .and_then(|n| n.as_str())
                                                    {
                                                        profile_name = name.to_string();
                                                    }
                                                }
                                            }

                                            let external_id = format!("whatsapp:+{}", from);

                                            {
                                                let conn = self.db.lock().await;
                                                let existing =
                                                    moat_core::business::db::get_contact_by_identity(
                                                        &conn,
                                                        "whatsapp",
                                                        &external_id,
                                                    )
                                                    .unwrap_or(None);
                                                if existing.is_none() {
                                                    let new_id = uuid::Uuid::new_v4().to_string();
                                                    let contact =
                                                        moat_core::business::models::ExternalContact {
                                                            id: new_id.clone(),
                                                            name: Some(profile_name),
                                                            email: None,
                                                            phone: Some(format!("+{}", from)),
                                                            created_at: chrono::Utc::now()
                                                                .timestamp(),
                                                            updated_at: chrono::Utc::now()
                                                                .timestamp(),
                                                        };
                                                    let _ = moat_core::business::db::insert_contact(
                                                        &conn, &contact,
                                                    );

                                                    let mapping = moat_core::business::models::ContactIdentityMapping {
                                                        id: uuid::Uuid::new_v4().to_string(),
                                                        contact_id: new_id.clone(),
                                                        channel: "whatsapp".to_string(),
                                                        external_id: external_id.clone(),
                                                    };
                                                    let _ = moat_core::business::db::insert_identity_mapping(&conn, &mapping);
                                                }
                                            };

                                            {
                                                let conn = self.db.lock().await;
                                                let _ = moat_core::business::db::increment_kpi_metric(
                                                    &conn,
                                                    "messages_received",
                                                );
                                            }

                                            let reply_res = self
                                                .runtime
                                                .handle_interaction("whatsapp", &external_id, &body)
                                                .await;

                                            {
                                                let conn = self.db.lock().await;
                                                match &reply_res {
                                                    Ok(reply_body) => {
                                                        let _ = moat_core::business::reliability::idempotency::complete(&conn, &idempotency_key, reply_body);
                                                    }
                                                    Err(e) => {
                                                        eprintln!("[WhatsApp Webhook] Error handling interaction: {:?}", e);
                                                        let _ = moat_core::business::reliability::idempotency::fail(&conn, &idempotency_key);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                let response = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
                let _ = stream.write_all(response.as_bytes()).await;
                return Ok(());
            }
        }

        let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
        let _ = stream.write_all(response.as_bytes()).await;
        Ok(())
    }
}

struct HttpRequest {
    method: String,
    path: String,
    query: String,
    headers: std::collections::HashMap<String, String>,
    body: Vec<u8>,
}

async fn parse_http_request(stream: &mut tokio::net::TcpStream) -> Result<HttpRequest, String> {
    let mut buffer = Vec::new();
    let mut temp = [0u8; 1024];
    let mut header_end = None;

    loop {
        let n = stream.read(&mut temp).await.map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        buffer.extend_from_slice(&temp[..n]);
        if let Some(pos) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            header_end = Some(pos);
            break;
        }
        if buffer.len() > 8192 {
            return Err("Headers too large".to_string());
        }
    }

    let pos = header_end.ok_or_else(|| "No HTTP header end found".to_string())?;
    let header_part = String::from_utf8_lossy(&buffer[..pos]);
    let mut lines = header_part.lines();

    let req_line = lines
        .next()
        .ok_or_else(|| "Empty request line".to_string())?;
    let mut req_tokens = req_line.split_whitespace();
    let method = req_tokens
        .next()
        .ok_or_else(|| "Missing method".to_string())?
        .to_string();
    let full_path = req_tokens
        .next()
        .ok_or_else(|| "Missing path".to_string())?
        .to_string();

    let (path, query) = if let Some(idx) = full_path.find('?') {
        (
            full_path[..idx].to_string(),
            full_path[idx + 1..].to_string(),
        )
    } else {
        (full_path, "".to_string())
    };

    let mut headers = std::collections::HashMap::new();
    for line in lines {
        if let Some(idx) = line.find(':') {
            let key = line[..idx].trim().to_lowercase();
            let val = line[idx + 1..].trim().to_string();
            headers.insert(key, val);
        }
    }

    let mut body = buffer[pos + 4..].to_vec();
    if let Some(len_str) = headers.get("content-length") {
        let content_len: usize = len_str
            .parse()
            .map_err(|_| "Invalid Content-Length".to_string())?;
        while body.len() < content_len {
            let mut read_buf = vec![0u8; content_len - body.len()];
            let n = stream
                .read(&mut read_buf)
                .await
                .map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&read_buf[..n]);
        }
        if body.len() > content_len {
            body.truncate(content_len);
        }
    }

    Ok(HttpRequest {
        method,
        path,
        query,
        headers,
        body,
    })
}

fn parse_query_params(query: &str) -> std::collections::HashMap<String, String> {
    let mut params = std::collections::HashMap::new();
    for part in query.split('&') {
        if let Some(idx) = part.find('=') {
            let key = part[..idx].to_string();
            let val = part[idx + 1..].to_string();
            let decoded_val = val
                .replace("%20", " ")
                .replace("%2B", "+")
                .replace("%2b", "+")
                .replace("%2F", "/")
                .replace("%2f", "/");
            params.insert(key, decoded_val);
        }
    }
    params
}

fn validate_signature(body: &[u8], signature_header: &str, client_secret: &str) -> bool {
    if !signature_header.starts_with("sha256=") {
        return false;
    }
    let expected_hex = &signature_header[7..];
    let expected_sig = match decode_hex(expected_hex) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let computed_sig = hmac_sha256(client_secret.as_bytes(), body);
    computed_sig == expected_sig.as_slice()
}

fn decode_hex(s: &str) -> Result<Vec<u8>, &'static str> {
    if !s.len().is_multiple_of(2) {
        return Err("Odd length");
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| "Invalid hex char"))
        .collect()
}
