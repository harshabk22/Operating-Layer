use std::time::SystemTime;

pub fn calculate_retry_delay(retries: u32, base_delay_secs: u64) -> u64 {
    let base = base_delay_secs * 2u64.pow(retries);

    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(123456789);

    let jitter_percent = 50 + (nanos % 101); // 50 to 150 %
    let delay_millis = (base as f64 * 1000.0 * (jitter_percent as f64 / 100.0)) as u64;

    delay_millis.div_ceil(1000)
}
