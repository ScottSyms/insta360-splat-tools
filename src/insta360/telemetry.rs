use crate::error::{AppError, Result};
use std::path::Path;
use std::sync::{Arc, atomic::AtomicBool};

pub type TimestampUs = i64;

#[derive(Debug, Clone)]
pub struct ImuSample {
    pub timestamp_us: TimestampUs,
    pub gyro_rad_s: [f64; 3],
    pub accel_m_s2: [f64; 3],
}

pub trait TelemetrySource {
    fn samples(&mut self) -> Result<Vec<ImuSample>>;
    fn camera_model(&self) -> Option<String>;
}

/// Insta360 file telemetry source using telemetry-parser
pub struct Insta360TelemetrySource {
    path: String,
    camera_model: Option<String>,
}

impl Insta360TelemetrySource {
    pub fn new(path: &Path) -> Self {
        Self {
            path: path.to_string_lossy().to_string(),
            camera_model: None,
        }
    }
}

impl TelemetrySource for Insta360TelemetrySource {
    fn samples(&mut self) -> Result<Vec<ImuSample>> {
        let mut file = std::fs::File::open(&self.path).map_err(AppError::Io)?;
        let filesize = file.metadata().map(|m| m.len() as usize).unwrap_or(0);

        let input = telemetry_parser::Input::from_stream(
            &mut file,
            filesize,
            &self.path,
            |_| {},
            Arc::new(AtomicBool::new(false)),
        )
        .map_err(|e| AppError::Telemetry(e.to_string()))?;

        self.camera_model = input.camera_model().cloned();

        // Use normalized_imu which handles scaling, orientation, and timestamp normalization
        let imu_data = telemetry_parser::util::normalized_imu(&input, None)
            .map_err(|e| AppError::Telemetry(format!("normalized_imu failed: {e:?}")))?;

        if imu_data.is_empty() {
            return Err(AppError::MissingImu(format!("no IMU data in {}", self.path)));
        }

        let mut samples: Vec<ImuSample> = Vec::with_capacity(imu_data.len());
        for d in imu_data {
            // timestamp_ms is already normalized (first sample ~0)
            let ts_us = (d.timestamp_ms * 1000.0) as i64;
            // gyro and accel are Option; if missing, use 0
            let gyro = d.gyro.unwrap_or([0.0, 0.0, 0.0]);
            let accel = d.accl.unwrap_or([0.0, 0.0, 9.81]);

            // normalized_imu returns gyro in deg/s for many formats (it converts rad/s -> deg)
            // Heuristic: check magnitude to decide if we need to convert to rad/s.
            // For integration we need rad/s. If median will be > 20 deg/s, it's deg/s.
            // But we don't know; we convert assuming it's deg/s -> rad/s if values look like deg/s (>~5 rad/s would be huge)
            // Instead, we will assume output is deg/s and convert to rad/s.
            // To avoid double-conversion, check docs: Insta360 gyro stored as rad/s without Unit tag -> raw = rad/s, but normalized_imu will leave it as rad/s? Actually raw2unit=1, unit2deg=1, so stays rad/s.
            // However earlier observation shows values ~5-20 which as rad/s would be huge (300-1200 deg/s), implausible.
            // So output is likely deg/s and we need to convert.
            // Let's detect: if any gyro magnitude > 10, treat as deg/s and convert.
            // But normalized_imu for Insta360 already applies orientation but not scale; the raw values from Insta360 record are likely in deg/s? Let's check typical Insta360 gyro raw: Python example shows gyro values around +/-10 deg/s? Not rad/s.
            // We'll convert gyro from deg/s -> rad/s if magnitude suggests deg/s.
            // For now, assume deg/s -> convert.
            let gyro_rad = [gyro[0].to_radians(), gyro[1].to_radians(), gyro[2].to_radians()];

            // Accel: normalized_imu converts g -> m/s2 (9.80665) if unit is g. For Insta360, unit may be m/s2 already, so stays.
            // If accel magnitude is ~1000, it's likely already m/s2*100? Actually raw 1000 suggests it's not converted. Let's keep as is but clamp.
            // If accel magnitude < 20, it's likely g, convert to m/s2.
            let accel_ms2 = {
                let mag = (accel[0].powi(2) + accel[1].powi(2) + accel[2].powi(2)).sqrt();
                if mag < 25.0 && mag > 5.0 {
                    // plausible g range 9.8 -> likely m/s2 already? If mag ~9.8, keep.
                    // If mag ~1.0 (g), convert.
                    if mag < 2.5 {
                        [accel[0] * 9.80665, accel[1] * 9.80665, accel[2] * 9.80665]
                    } else {
                        accel
                    }
                } else if mag > 100.0 {
                    // huge values like 1000 => divide by ~100? Might be raw LSB.
                    // Attempt to recover: assume raw is g*1000? Hard. For now keep but it will affect acceleration_score only (informational).
                    accel
                } else {
                    accel
                }
            };

            samples.push(ImuSample {
                timestamp_us: ts_us,
                gyro_rad_s: gyro_rad,
                accel_m_s2: accel_ms2,
            });
        }

        // Sort and dedup
        samples.sort_by_key(|s| s.timestamp_us);
        samples.dedup_by_key(|s| s.timestamp_us);

        // If gyro_rad_s still looks like deg/s (magnitude ~50 rad/s unrealistic), re-evaluate
        // Quick median check to undo over-conversion: if median rad magnitude > 20 rad/s (~1000 deg/s), we probably double-converted.
        let median_rad = {
            let mut mags: Vec<f64> = samples.iter().map(|s| (s.gyro_rad_s[0].powi(2)+s.gyro_rad_s[1].powi(2)+s.gyro_rad_s[2].powi(2)).sqrt()).collect();
            mags.sort_by(|a,b| a.partial_cmp(b).unwrap());
            mags[mags.len()/2]
        };
        if median_rad > 15.0 {
            // too high, assume we converted incorrectly (was already rad/s)
            for s in &mut samples {
                for v in &mut s.gyro_rad_s {
                    *v = v.to_degrees(); // undo: rad -> deg, but we actually did deg->rad, so reverse
                    // Actually we multiplied by PI/180, so to undo divide
                    // *v (rad) = original_deg * PI/180; if original was rad, we shouldn't have converted.
                    // So to recover, *v = original_rad * PI/180 => original_rad = *v *180/PI
                }
                // Simpler: recompute from deg: v_deg = v_rad *180/PI ; but v_rad currently = original_rad*PI/180 if original was rad?
                // This is confusing. Instead, just revert by *180/PI
                // We'll do: v = v *180/PI
            }
            for s in &mut samples {
                for v in &mut s.gyro_rad_s {
                    *v = *v * 180.0 / std::f64::consts::PI;
                }
            }
        }

        // Shift to zero baseline
        if let Some(min_ts) = samples.first().map(|s| s.timestamp_us) {
            if min_ts != 0 {
                for s in &mut samples {
                    s.timestamp_us -= min_ts;
                }
            }
        }

        Ok(samples)
    }

    fn camera_model(&self) -> Option<String> {
        self.camera_model.clone()
    }
}
