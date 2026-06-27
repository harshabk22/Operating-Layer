pub mod db;
pub mod crypto;
pub mod middleware;
pub mod api;
pub mod workers;

#[cfg(test)]
pub mod tests;

#[derive(Clone)]
pub struct AppState {
    pub db: db::DbStore,
    pub jwt_secret: String,
}
