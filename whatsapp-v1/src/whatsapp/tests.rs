use crate::whatsapp::provider::WhatsAppConnector;
use crate::whatsapp::webhook::WhatsAppWebhookServer;
use moat_core::business::db;
use moat_core::business::runtime::CustomerInteractionRuntime;
use moat_core::communication::connector::ChannelConnector;
use moat_core::communication::types::{ChannelType, InteractionMessage, MessageDirection};
use moat_core::communication::ConnectorRegistry;
use moat_core::agent::llm_types::{LlmMessage, LlmProvider, LlmResponse};
use moat_core::agent::execution::journal_chain::hmac_sha256;
use moat_core::business::calendar::{CalendarEvent, CalendarProvider};
use moat_core::business::spreadsheet::SpreadsheetProvider;
use moat_core::business::owner::NotificationProvider;
use moat_core::memory::meta_brain::MetaBrain;
use moat_core::MoatResult;
use rusqlite::Connection;
use std::sync::Arc;
use tokio::sync::RwLock as AsyncRwLock;
use reqwest::Client;

type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

struct StubProvider {
    response: Arc<std::sync::Mutex<Option<(String, u32)>>>,
}

impl StubProvider {
    fn new(xml: &str) -> Self {
        Self {
            response: Arc::new(std::sync::Mutex::new(Some((xml.to_string(), 42)))),
        }
    }
}

impl LlmProvider for StubProvider {
    fn chat_complete<'a>(
        &'a self,
        _model: &'a str,
        _messages: Vec<LlmMessage>,
        _tools: Option<Vec<moat_core::agent::llm_types::LlmTool>>,
        _max_tokens: usize,
        _temperature: f32,
        _response_format: Option<serde_json::Value>,
    ) -> BoxFuture<'a, MoatResult<LlmResponse>> {
        Box::pin(async move {
            let (text, tokens) = {
                let lock = self.response.lock().expect("lock stub response");
                if let Some(ref val) = *lock {
                    val.clone()
                } else {
                    ("<response>done</response>".to_string(), 42)
                }
            };
            Ok(LlmResponse {
                content: Some(text),
                tool_calls: None,
                tokens,
                structured: None,
            })
        })
    }
}

// Local mock implementations since moat_core's are cfg(test) only in external crates
struct MockCalendarProvider {
    pub should_fail: std::sync::atomic::AtomicBool,
    events: std::sync::Mutex<std::collections::HashMap<String, CalendarEvent>>,
}

impl MockCalendarProvider {
    fn new() -> Self {
        Self {
            should_fail: std::sync::atomic::AtomicBool::new(false),
            events: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl CalendarProvider for MockCalendarProvider {
    fn check_availability(&self, _start_time: i64, _end_time: i64) -> anyhow::Result<bool> {
        if self.should_fail.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(anyhow::anyhow!("Simulated calendar availability check failure"));
        }
        Ok(true)
    }

    fn create_event(&self, title: &str, start_time: i64, end_time: i64) -> anyhow::Result<CalendarEvent> {
        if self.should_fail.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(anyhow::anyhow!("Simulated calendar event creation failure"));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let event = CalendarEvent {
            id: id.clone(),
            title: title.to_string(),
            start_time,
            end_time,
        };
        let mut evts = self.events.lock().unwrap();
        evts.insert(id, event.clone());
        Ok(event)
    }

    fn update_event(&self, event_id: &str, title: &str, start_time: i64, end_time: i64) -> anyhow::Result<CalendarEvent> {
        let mut evts = self.events.lock().unwrap();
        if let Some(event) = evts.get_mut(event_id) {
            event.title = title.to_string();
            event.start_time = start_time;
            event.end_time = end_time;
            Ok(event.clone())
        } else {
            Err(anyhow::anyhow!("Calendar event not found"))
        }
    }

    fn cancel_event(&self, event_id: &str) -> anyhow::Result<()> {
        let mut evts = self.events.lock().unwrap();
        if evts.remove(event_id).is_some() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Calendar event not found"))
        }
    }
}

struct MockSpreadsheetProvider {
    sheets: std::sync::Mutex<std::collections::HashMap<String, Vec<Vec<String>>>>,
}

impl MockSpreadsheetProvider {
    fn new() -> Self {
        Self {
            sheets: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl SpreadsheetProvider for MockSpreadsheetProvider {
    fn append_row(&self, sheet_name: &str, values: Vec<String>) -> anyhow::Result<()> {
        let mut sheets = self.sheets.lock().unwrap();
        sheets
            .entry(sheet_name.to_string())
            .or_default()
            .push(values);
        Ok(())
    }

    fn update_row(&self, sheet_name: &str, row_index: usize, values: Vec<String>) -> anyhow::Result<()> {
        let mut sheets = self.sheets.lock().unwrap();
        if let Some(rows) = sheets.get_mut(sheet_name) {
            if row_index < rows.len() {
                rows[row_index] = values;
                Ok(())
            } else {
                Err(anyhow::anyhow!("Row index out of bounds"))
            }
        } else {
            Err(anyhow::anyhow!("Sheet not found"))
        }
    }

    fn lookup_row(&self, sheet_name: &str, query: &str) -> anyhow::Result<Option<(usize, Vec<String>)>> {
        let sheets = self.sheets.lock().unwrap();
        if let Some(rows) = sheets.get(sheet_name) {
            for (index, row) in rows.iter().enumerate() {
                for cell in row {
                    if cell.contains(query) {
                        return Ok(Some((index, row.clone())));
                    }
                }
            }
        }
        Ok(None)
    }
}

struct MockNotificationProvider;

impl MockNotificationProvider {
    fn new() -> Self {
        Self
    }
}

impl NotificationProvider for MockNotificationProvider {
    fn notify_new_authorization(&self, _auth_id: &str, _thread_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn notify_new_escalation(&self, _escalation_id: &str, _thread_id: &str, _reason: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn notify_booking_completed(&self, _booking_id: &str, _thread_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn notify_new_opportunity(&self, _opportunity_id: &str, _thread_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn notify_pipeline_stage_change(&self, _opportunity_id: &str, _from_stage: &str, _to_stage: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

fn run_mock_whatsapp_api_server() -> String {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut buffer = [0; 2048];
            let _ = stream.read(&mut buffer);
            let request = String::from_utf8_lossy(&buffer);

            let (response_status, response_body) = if request.contains("/messages") {
                (
                    "200 OK",
                    r#"{"messaging_product":"whatsapp","contacts":[{"input":"1234567890","wa_id":"1234567890"}],"messages":[{"id":"wa_msg_id_123"}]}"#,
                )
            } else {
                ("404 Not Found", "")
            };

            let response = format!(
                "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                response_status, response_body.len(), response_body
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    format!("http://127.0.0.1:{}", port)
}

#[tokio::test]
async fn test_whatsapp_connector_send() {
    let conn = Connection::open_in_memory().unwrap();
    db::prepare_tables(&conn).unwrap();
    let conn_arc = Arc::new(tokio::sync::Mutex::new(conn));

    let mock_url = run_mock_whatsapp_api_server();

    let connector = WhatsAppConnector::with_url(
        conn_arc.clone(),
        "phone_id_123".to_string(),
        mock_url,
        Some("valid_token".to_string()),
    );

    let msg = InteractionMessage {
        id: "msg_123".to_string(),
        business_id: "biz1".to_string(),
        thread_id: "t1".to_string(),
        customer_id: "whatsapp:+1234567890".to_string(),
        channel: ChannelType::Whatsapp,
        direction: MessageDirection::Outbound,
        body: "Hello from core!".to_string(),
        timestamp: chrono::Utc::now().timestamp(),
    };

    connector.send_message(msg).await.unwrap();
}

#[tokio::test]
async fn test_whatsapp_webhook_server() {
    let db_name = format!(
        "file:test_webhook_{}?mode=memory&cache=shared",
        uuid::Uuid::new_v4()
    );
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
        | rusqlite::OpenFlags::SQLITE_OPEN_CREATE
        | rusqlite::OpenFlags::SQLITE_OPEN_URI;

    // Setup DB and prepare tables
    let conn = Connection::open_with_flags(&db_name, flags).unwrap();
    db::prepare_tables(&conn).unwrap();

    let conn_webhook_for_provider = Connection::open_with_flags(&db_name, flags).unwrap();
    let conn_webhook_arc = Arc::new(tokio::sync::Mutex::new(conn_webhook_for_provider));

    let conn_rt_for_provider = Connection::open_with_flags(&db_name, flags).unwrap();
    let conn_rt_arc = Arc::new(tokio::sync::Mutex::new(conn_rt_for_provider));

    // Create registry and stub orchestrator
    let registry = Arc::new(ConnectorRegistry::new());
    let provider = Arc::new(StubProvider::new("<response>done</response>"));
    let meta_brain = Arc::new(AsyncRwLock::new(MetaBrain::new()));

    let ai_agent = Arc::new(moat_core::agent::ai_agent::AIAgent::new(
        provider.clone(),
        Arc::clone(&conn_rt_arc),
        Arc::clone(&meta_brain),
        None,
    ).unwrap());

    let cal = Arc::new(MockCalendarProvider::new());
    let sheet = Arc::new(MockSpreadsheetProvider::new());
    let notifier = Arc::new(MockNotificationProvider::new());

    let runtime = Arc::new(CustomerInteractionRuntime::with_providers(
        ai_agent,
        conn_rt_arc.clone(),
        registry,
        cal,
        sheet,
        notifier,
    ));

    let verify_token = "my_secret_verify_token".to_string();
    let client_secret = "my_app_secret".to_string();

    let server = Arc::new(WhatsAppWebhookServer::new(
        runtime.clone(),
        conn_webhook_arc.clone(),
        verify_token.clone(),
        client_secret.clone(),
    ));

    // Bind to arbitrary port
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let addr = format!("127.0.0.1:{}", port);

    let handle = WhatsAppWebhookServer::start(server, addr.clone());

    // Give server a tiny bit to spin up
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let client = Client::new();

    // 1. GET verification success
    let res = client
        .get(format!(
            "http://{}/webhook?hub.mode=subscribe&hub.verify_token={}&hub.challenge=hello_world",
            addr, verify_token
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.text().await.unwrap(), "hello_world");

    // 2. GET verification failure
    let res = client.get(format!("http://{}/webhook?hub.mode=subscribe&hub.verify_token=bad_token&hub.challenge=hello_world", addr))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);

    // 3. POST message signature validation failure
    let payload = serde_json::json!({
        "entry": [
            {
                "changes": [
                    {
                        "value": {
                            "messaging_product": "whatsapp",
                            "metadata": {
                                "display_phone_number": "16505551111",
                                "phone_number_id": "123456789"
                            },
                            "contacts": [
                                {
                                    "profile": {
                                        "name": "Bob Test"
                                    },
                                    "wa_id": "1234567890"
                                }
                            ],
                            "messages": [
                                {
                                    "from": "1234567890",
                                    "id": "wa_msg_1",
                                    "timestamp": "1672531199",
                                    "text": {
                                        "body": "Hello world"
                                    },
                                    "type": "text"
                                }
                            ]
                        },
                        "field": "messages"
                    }
                ]
            }
        ]
    });

    let payload_bytes = serde_json::to_vec(&payload).unwrap();
    let res = client
        .post(format!("http://{}/webhook", addr))
        .header(
            "x-hub-signature-256",
            "sha256=invalid_signature_hex_value_12345678",
        )
        .body(payload_bytes.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // 4. POST message success
    let signature = hmac_sha256(client_secret.as_bytes(), &payload_bytes);
    let signature_hex: String = signature.iter().map(|b| format!("{:02x}", b)).collect();
    let signature_header = format!("sha256={}", signature_hex);

    let res = client
        .post(format!("http://{}/webhook", addr))
        .header("x-hub-signature-256", signature_header.clone())
        .body(payload_bytes.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // Verify customer automatically created and metric incremented
    {
        let contact =
            db::get_contact_by_identity(&conn, "whatsapp", "whatsapp:+1234567890").unwrap();
        assert!(contact.is_some());
        let contact = contact.unwrap();
        assert_eq!(contact.name.unwrap(), "Bob Test");
        assert_eq!(contact.phone.unwrap(), "+1234567890");

        let metric = db::get_kpi_metric(&conn, "messages_received").unwrap();
        assert_eq!(metric, 1);
    }

    // 5. POST replay check (sending same message ID should be ignored, return 200 but not insert/duplicate)
    let res = client
        .post(format!("http://{}/webhook", addr))
        .header("x-hub-signature-256", signature_header)
        .body(payload_bytes)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // Verify metric wasn't incremented again
    {
        let metric = db::get_kpi_metric(&conn, "messages_received").unwrap();
        assert_eq!(metric, 1); // remains 1, proving replay protection worked!
    }

    // Clean up server thread
    handle.abort();
}
