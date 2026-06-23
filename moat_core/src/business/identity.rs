use super::db;
use super::models::{ContactIdentityMapping, ExternalContact};
use crate::MoatError;
use crate::MoatResult;
use rusqlite::Connection;
use uuid::Uuid;

pub fn resolve_contact(
    conn: &Connection,
    channel: &str,
    external_id: &str,
) -> MoatResult<ExternalContact> {
    // Check if identity mapping exists
    if let Some(contact) = db::get_contact_by_identity(conn, channel, external_id)? {
        return Ok(contact);
    }

    // Identity doesn't exist, create a new contact
    let now = chrono::Utc::now().timestamp();
    let contact_id = Uuid::new_v4().to_string();
    let contact = ExternalContact {
        id: contact_id.clone(),
        name: Some(format!("Contact_{}", &contact_id[..8])),
        email: None,
        phone: if channel == "whatsapp" {
            Some(external_id.to_string())
        } else {
            None
        },
        created_at: now,
        updated_at: now,
    };

    db::insert_contact(conn, &contact)?;

    // Insert identity mapping
    let mapping = ContactIdentityMapping {
        id: Uuid::new_v4().to_string(),
        contact_id,
        channel: channel.to_string(),
        external_id: external_id.to_string(),
    };

    db::insert_identity_mapping(conn, &mapping)?;

    Ok(contact)
}

pub fn link_identity(
    conn: &Connection,
    contact_id: &str,
    channel: &str,
    external_id: &str,
) -> MoatResult<()> {
    // Ensure contact exists
    let contact_opt = db::get_contact(conn, contact_id)?;
    if contact_opt.is_none() {
        return Err(MoatError::Orchestrator(format!(
            "Contact ID {} does not exist",
            contact_id
        )));
    }

    // Insert new mapping
    let mapping = ContactIdentityMapping {
        id: Uuid::new_v4().to_string(),
        contact_id: contact_id.to_string(),
        channel: channel.to_string(),
        external_id: external_id.to_string(),
    };

    db::insert_identity_mapping(conn, &mapping)?;
    Ok(())
}
