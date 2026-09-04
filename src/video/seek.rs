/// Helper for timestamp conversion
pub fn us_to_pts(timestamp_us: i64, time_base_num: i32, time_base_den: i32) -> i64 {
    // pts = timestamp_us * time_base_den / (1e6 * time_base_num)
    // time_base = num/den  => duration per tick = num/den seconds
    // so pts = timestamp_us / 1e6 / (num/den) = timestamp_us * den / (1e6 * num)
    if time_base_num == 0 {
        return 0;
    }
    (timestamp_us as f64 * time_base_den as f64 / (1_000_000.0 * time_base_num as f64)) as i64
}
