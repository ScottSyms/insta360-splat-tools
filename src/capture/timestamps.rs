/// Timestamp utilities: i64 nanoseconds as authoritative (§7)
pub type TimestampNs = i64;

pub fn us_to_ns(us: i64) -> TimestampNs { us * 1000 }
pub fn ns_to_us(ns: TimestampNs) -> i64 { ns / 1000 }
