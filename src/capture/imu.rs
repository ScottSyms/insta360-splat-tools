/// Minimal IMU sample for capture layer (avoids circular dependency on binary crate)
#[derive(Debug, Clone)]
pub struct ImuSample {
    pub timestamp_us: i64,
    pub gyro_rad_s: [f64; 3],
    pub accel_m_s2: [f64; 3],
}
pub type TimestampUs = i64;
