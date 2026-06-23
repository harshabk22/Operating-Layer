use moat_core::business::migrations;
use rusqlite::Connection;
use std::fs;

fn main() {
    println!("Testing migrations engine...");

    // Remove old test db
    let db_path = "test_migrations.db";
    let _ = fs::remove_file(db_path);

    let conn = Connection::open(db_path).unwrap();

    // Verify fresh install (upgrade to latest)
    println!("Step 1: Fresh install (upgrade)");
    migrations::upgrade(&conn).unwrap();
    let version = migrations::get_current_version(&conn).unwrap();
    assert_eq!(version, 2, "Version should be 2 after upgrade");
    println!("Fresh install success, version = {}", version);

    // Verify tables exist
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table'")
        .unwrap();
    let tables: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|x| x.unwrap())
        .collect();
    assert!(tables.contains(&"business_opportunities".to_string()));
    assert!(tables.contains(&"schema_migrations".to_string()));

    // Verify rollback
    println!("Step 2: Rollback (version 2 -> 1)");
    migrations::rollback(&conn).unwrap();
    let version_rollback = migrations::get_current_version(&conn).unwrap();
    assert_eq!(version_rollback, 1, "Version should be 1 after rollback");
    println!("Rollback success, version = {}", version_rollback);

    // Verify indexes are dropped
    let mut stmt2 = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='index' AND name='idx_proj_opp_status'")
        .unwrap();
    let has_index: bool = stmt2
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .next()
        .is_some();
    assert!(!has_index, "Index should have been dropped");

    // Upgrade back to 2
    println!("Step 3: Upgrade back to 2");
    migrations::upgrade(&conn).unwrap();
    let version_reupgrade = migrations::get_current_version(&conn).unwrap();
    assert_eq!(version_reupgrade, 2, "Version should be 2 after re-upgrade");

    // Verify index is back
    let mut stmt3 = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='index' AND name='idx_proj_opp_status'")
        .unwrap();
    let has_index2: bool = stmt3
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .next()
        .is_some();
    assert!(has_index2, "Index should have been recreated");
    println!("Re-upgrade success, version = {}", version_reupgrade);

    println!("All verification steps passed successfully!");
}
