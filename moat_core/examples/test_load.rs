fn main() {
    let home = dirs::home_dir().unwrap();
    let db_path = home.join(".ageex").join("business.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let res =
        moat_core::business::db::load_calendar_credentials(&conn, "default_business", "google");
    println!("Load Result: {:?}", res.unwrap().is_some());
}
