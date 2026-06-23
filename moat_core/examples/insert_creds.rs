fn main() {
    let home = dirs::home_dir().unwrap();
    let db_path = home.join(".ageex").join("business.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let access_token = "ya29.a0AT3oNZ9DBz_4HLfVHt2FdAvupmsnp5z9VD1veRHmEdZVj3ce1nr1KwJCHDeSsBzGCUEHD_urvvjQJGLSoA94GmEXWaavz0Crqe79sAfSlB7kZjT-FW3MoBzQXYIt97QoOKaAPFSwe2WwJF_Bk1kE8KFQyrFmUB3nEbTzyQ8aO_VIOR7k2oDN1P9pfrUl0w3ALPZ-d_EaCgYKAQQSARASFQHGX2MigvST6NCDE24lYRABw2KtpA0206";
    let refresh_token = "1//0gBSNduCa2oBBCgYIARAAGBASNwF-L9IrnXrmzekUcvrgqHvLqMG611u5jMMzGCwaS9OlJAXxlrvJkMvl0-o36IWB3rw_Yt_WQTU";
    let expires_at = chrono::Utc::now().timestamp() + 3599;

    moat_core::business::db::save_calendar_credentials(
        &conn,
        "default_business",
        "google",
        access_token,
        refresh_token,
        expires_at,
    )
    .unwrap();

    println!("Inserted successfully!");
}
