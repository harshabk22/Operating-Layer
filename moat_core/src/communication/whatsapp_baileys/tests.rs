use crate::communication::connector::ChannelConnector;
use crate::communication::db::{
    clear_session, delete_session_file, has_baileys_credentials, load_session_files,
    prepare_tables, save_session_file,
};
use crate::communication::types::{ChannelType, InteractionMessage, MessageDirection};
use crate::communication::whatsapp::WhatsAppConnector;
use crate::communication::whatsapp_baileys::connector::{
    WhatsAppBaileysConnector, WhatsAppMultiplexer,
};
use crate::communication::whatsapp_baileys::session::SessionState;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

#[test]
fn test_store_persistence_and_encryption() {
    let conn = Connection::open_in_memory().unwrap();
    prepare_tables(&conn).unwrap();

    let biz_id = "test_biz";
    let conn_type = "whatsapp_baileys";
    let file_name = "creds.json";
    let content = b"{\"paired\": true, \"secret\": \"12345\"}";

    // Set environment encryption key
    std::env::set_var("MOAT_SECRET_KEY", "TestSecretKey123");

    // Save
    save_session_file(&conn, biz_id, conn_type, file_name, content).unwrap();

    // Verify it is encrypted in the raw DB
    let encrypted_content: String = conn
        .query_row(
            "SELECT file_content FROM communication_connector_sessions WHERE business_id = ?1 AND file_name = ?2",
            [biz_id, file_name],
            |row| row.get(0),
        )
        .unwrap();

    // It should not match the plaintext content
    assert_ne!(encrypted_content.as_bytes(), content);

    // Check presence
    let has_creds = has_baileys_credentials(&conn, biz_id).unwrap();
    assert!(has_creds);

    // Load and verify decrypted content
    let files = load_session_files(&conn, biz_id, conn_type).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].0, file_name);
    assert_eq!(files[0].1, content);

    // Delete
    delete_session_file(&conn, biz_id, conn_type, file_name).unwrap();
    let has_creds_after = has_baileys_credentials(&conn, biz_id).unwrap();
    assert!(!has_creds_after);

    // Clear session
    save_session_file(&conn, biz_id, conn_type, "another.json", content).unwrap();
    clear_session(&conn, biz_id, conn_type).unwrap();
    let files_after_clear = load_session_files(&conn, biz_id, conn_type).unwrap();
    assert!(files_after_clear.is_empty());
}

#[tokio::test]
async fn test_multiplexer_routing() {
    let conn = Connection::open_in_memory().unwrap();
    prepare_tables(&conn).unwrap();
    let conn_arc = Arc::new(Mutex::new(conn));

    let business_connector = Arc::new(WhatsAppConnector::new(
        conn_arc.clone(),
        "default_number".to_string(),
    ));
    let baileys_connector = Arc::new(WhatsAppBaileysConnector::new(
        conn_arc.clone(),
        "default_business".to_string(),
    ));

    let multiplexer = WhatsAppMultiplexer::new(
        business_connector.clone(),
        baileys_connector.clone(),
        conn_arc.clone(),
    );

    // 1. Without credentials, should use Business connector (which will fail because it's not connected or stub config, but let's check it's routed correctly)
    let msg = InteractionMessage::new(
        "default_business".to_string(),
        "thread1".to_string(),
        "whatsapp:+123456789".to_string(),
        ChannelType::Whatsapp,
        MessageDirection::Outbound,
        "Test multiplexer message".to_string(),
    );

    let res = multiplexer.send_message(msg.clone()).await;
    // Business connector should return error because of invalid token or graph URL in test environment, but Baileys would fail with "not connected"
    // Let's verify we get the Business connector's response error (WhatsApp API returned error or similar, or reqwest error)
    // rather than Baileys "Cannot send message: Baileys is not connected"
    assert!(res.is_err());
    let err_str = res.unwrap_err().to_string();
    assert!(!err_str.contains("Baileys is not connected"));

    // 2. Insert Baileys credentials, should now use Baileys connector and return its specific "Baileys is not connected" error!
    {
        let conn_lock = conn_arc.lock().unwrap();
        save_session_file(
            &conn_lock,
            "default_business",
            "whatsapp_baileys",
            "creds.json",
            b"{}",
        )
        .unwrap();
    }

    let res_baileys = multiplexer.send_message(msg).await;
    assert!(res_baileys.is_err());
    let err_baileys_str = res_baileys.unwrap_err().to_string();
    assert!(err_baileys_str.contains("Baileys is not connected"));
}

#[test]
fn test_session_lifecycle_states() {
    let unpaired = SessionState::Unpaired;
    let pairing = SessionState::Pairing;
    let connected = SessionState::Connected;
    let reconnecting = SessionState::Reconnecting;
    let disconnected = SessionState::Disconnected;

    assert_eq!(unpaired.to_string(), "unpaired");
    assert_eq!(pairing.to_string(), "pairing");
    assert_eq!(connected.to_string(), "connected");
    assert_eq!(reconnecting.to_string(), "reconnecting");
    assert_eq!(disconnected.to_string(), "disconnected");
}

#[test]
fn test_jid_conversions() {
    use crate::communication::whatsapp_baileys::connector::{
        normalize_jid_to_external_id, external_id_to_jid,
    };

    // 1. Test normal phone number JID
    assert_eq!(
        normalize_jid_to_external_id("919482520149@s.whatsapp.net"),
        "whatsapp:+919482520149"
    );
    assert_eq!(
        external_id_to_jid("whatsapp:+919482520149"),
        "919482520149@s.whatsapp.net"
    );

    // 2. Test LID JID
    assert_eq!(
        normalize_jid_to_external_id("178769625133202@lid"),
        "whatsapp:178769625133202@lid"
    );
    assert_eq!(
        external_id_to_jid("whatsapp:178769625133202@lid"),
        "178769625133202@lid"
    );

    // 3. Test legacy/incorrectly normalized LID format mapping recovery
    assert_eq!(
        external_id_to_jid("whatsapp:+178769625133202"),
        "178769625133202@lid"
    );

    // 4. Test JID containing device suffix (e.g. colon notation)
    assert_eq!(
        normalize_jid_to_external_id("178769625133202:2@lid"),
        "whatsapp:178769625133202@lid"
    );
    assert_eq!(
        normalize_jid_to_external_id("919482520149:1@s.whatsapp.net"),
        "whatsapp:+919482520149"
    );
}
