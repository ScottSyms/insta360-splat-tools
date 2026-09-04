use crate::insta360::telemetry::ImuSample;

#[derive(Debug, Clone)]
pub struct FilterConfig {
    pub low_pass_alpha: Option<f64>,
    pub remove_duplicates: bool,
    pub reject_invalid: bool,
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            low_pass_alpha: Some(0.3),
            remove_duplicates: true,
            reject_invalid: true,
        }
    }
}

pub fn preprocess(mut samples: Vec<ImuSample>, cfg: &FilterConfig) -> Vec<ImuSample> {
    if cfg.reject_invalid {
        samples.retain(|s| {
            s.gyro_rad_s.iter().all(|v| v.is_finite())
                && s.accel_m_s2.iter().all(|v| v.is_finite())
        });
    }

    // sort by timestamp
    samples.sort_by_key(|s| s.timestamp_us);

    if cfg.remove_duplicates {
        samples.dedup_by(|a, b| a.timestamp_us == b.timestamp_us);
    }

    // low-pass filter on gyro
    if let Some(alpha) = cfg.low_pass_alpha {
        if samples.len() > 1 {
            let mut filtered = samples[0].clone();
            let mut out = Vec::with_capacity(samples.len());
            out.push(filtered.clone());
            for s in samples.iter().skip(1) {
                let mut nf = s.clone();
                for i in 0..3 {
                    nf.gyro_rad_s[i] = alpha * s.gyro_rad_s[i] + (1.0 - alpha) * filtered.gyro_rad_s[i];
                    nf.accel_m_s2[i] = alpha * s.accel_m_s2[i] + (1.0 - alpha) * filtered.accel_m_s2[i];
                }
                filtered = nf.clone();
                out.push(nf);
            }
            return out;
        }
    }

    samples
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insta360::telemetry::ImuSample;

    #[test]
    fn test_dedup_and_sort() {
        let samples = vec![
            ImuSample { timestamp_us: 2000, gyro_rad_s: [0.0,0.0,0.0], accel_m_s2: [0.0,0.0,0.0]},
            ImuSample { timestamp_us: 1000, gyro_rad_s: [1.0,0.0,0.0], accel_m_s2: [0.0,0.0,0.0]},
            ImuSample { timestamp_us: 1000, gyro_rad_s: [1.0,0.0,0.0], accel_m_s2: [0.0,0.0,0.0]},
        ];
        let out = preprocess(samples, &FilterConfig{ low_pass_alpha: None, ..Default::default()});
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].timestamp_us, 1000);
        assert_eq!(out[1].timestamp_us, 2000);
    }
}
