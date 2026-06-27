use rusqlite::{params, Connection, Result};
use std::sync::{Arc, Mutex};
use crate::crypto::hash_sha256;

#[derive(Clone)]
pub struct DbStore {
    pub conn: Arc<Mutex<Connection>>,
}

impl DbStore {
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.initialize_schema()?;
        store.seed_default_admin()?;
        Ok(store)
    }

    fn initialize_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        
        // 1. admin_users table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS admin_users (
                id TEXT PRIMARY KEY,
                username TEXT UNIQUE NOT NULL,
                email TEXT UNIQUE NOT NULL,
                password_hash TEXT NOT NULL,
                role TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );",
            [],
        )?;

        // 2. customers table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS customers (
                id TEXT PRIMARY KEY,
                organization_name TEXT NOT NULL,
                organization_id TEXT UNIQUE NOT NULL,
                status TEXT NOT NULL,
                tier TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );",
            [],
        )?;

        // 3. customer_runtimes table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS customer_runtimes (
                id TEXT PRIMARY KEY,
                customer_id TEXT NOT NULL REFERENCES customers(id) ON DELETE CASCADE,
                environment TEXT NOT NULL,
                status TEXT NOT NULL,
                api_key_hash TEXT UNIQUE NOT NULL,
                runtime_api_url TEXT NOT NULL,
                last_heartbeat_at TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE(customer_id, environment)
            );",
            [],
        )?;

        // 4. packages table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS packages (
                id TEXT PRIMARY KEY,
                name TEXT UNIQUE NOT NULL,
                description TEXT,
                latest_version TEXT,
                status TEXT NOT NULL,
                developer_id TEXT REFERENCES admin_users(id),
                signature_pubkey TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );",
            [],
        )?;

        // 5. package_versions table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS package_versions (
                id TEXT PRIMARY KEY,
                package_id TEXT NOT NULL REFERENCES packages(id) ON DELETE CASCADE,
                version TEXT NOT NULL,
                checksum_sha256 TEXT NOT NULL,
                signature TEXT NOT NULL,
                binary_url TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                UNIQUE(package_id, version)
            );",
            [],
        )?;

        // 6. deployments table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS deployments (
                id TEXT PRIMARY KEY,
                runtime_id TEXT NOT NULL REFERENCES customer_runtimes(id) ON DELETE CASCADE,
                package_id TEXT NOT NULL REFERENCES packages(id) ON DELETE CASCADE,
                version TEXT NOT NULL,
                status TEXT NOT NULL,
                error_message TEXT,
                installed_at TEXT,
                created_at TEXT NOT NULL
            );",
            [],
        )?;

        // 7. subscriptions table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS subscriptions (
                id TEXT PRIMARY KEY,
                customer_id TEXT NOT NULL REFERENCES customers(id) ON DELETE CASCADE,
                plan_id TEXT NOT NULL,
                status TEXT NOT NULL,
                payment_status TEXT NOT NULL,
                current_period_start TEXT NOT NULL,
                current_period_end TEXT NOT NULL,
                amount INTEGER NOT NULL,
                created_at TEXT NOT NULL
            );",
            [],
        )?;

        // 8. incidents table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS incidents (
                id TEXT PRIMARY KEY,
                runtime_id TEXT NOT NULL REFERENCES customer_runtimes(id) ON DELETE CASCADE,
                incident_type TEXT NOT NULL,
                severity TEXT NOT NULL,
                summary TEXT NOT NULL,
                details TEXT,
                status TEXT NOT NULL,
                reported_at TEXT NOT NULL,
                resolved_at TEXT
            );",
            [],
        )?;

        // 9. audit_logs table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS audit_logs (
                id TEXT PRIMARY KEY,
                user_id TEXT REFERENCES admin_users(id) ON DELETE SET NULL,
                username TEXT NOT NULL,
                action TEXT NOT NULL,
                target_type TEXT,
                target_id TEXT,
                details TEXT,
                ip_address TEXT,
                created_at TEXT NOT NULL
            );",
            [],
        )?;

        // 10. approvals table (retained for custom workflows)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS approvals (
                id TEXT PRIMARY KEY,
                action_type TEXT NOT NULL,
                requested_by TEXT NOT NULL,
                payload TEXT NOT NULL,
                status TEXT NOT NULL,
                approved_by TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );",
            [],
        )?;

        // 11. rollbacks table (retained for rollback history)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS rollbacks (
                id TEXT PRIMARY KEY,
                deployment_id TEXT NOT NULL REFERENCES deployments(id) ON DELETE CASCADE,
                from_version TEXT NOT NULL,
                to_version TEXT NOT NULL,
                reason TEXT NOT NULL,
                rolled_back_at TEXT NOT NULL
            );",
            [],
        )?;

        // Indexes
        conn.execute("CREATE INDEX IF NOT EXISTS idx_admin_users_username ON admin_users(username);", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_customers_org_id ON customers(organization_id);", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_customer_runtimes_customer ON customer_runtimes(customer_id);", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_packages_name ON packages(name);", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_package_versions_package ON package_versions(package_id);", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_deployments_runtime ON deployments(runtime_id);", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_subscriptions_customer ON subscriptions(customer_id);", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_incidents_runtime ON incidents(runtime_id);", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_audit_logs_created ON audit_logs(created_at);", [])?;

        Ok(())
    }

    fn seed_default_admin(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM admin_users WHERE username = 'admin'",
            [],
            |row| row.get(0),
        )?;
        
        if count == 0 {
            let pass_hash = hash_sha256("admin"); // Default password "admin"
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO admin_users (id, username, email, password_hash, role, created_at, updated_at) 
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![
                    "admin-uuid-12345",
                    "admin",
                    "admin@moatageex.com",
                    pass_hash,
                    "Super Admin",
                    now,
                    now
                ],
            )?;
        }
        Ok(())
    }
}
