use crate::insta360::telemetry::{ImuSample, TimestampUs};
use nalgebra::{UnitQuaternion, Vector3};

#[derive(Debug, Clone)]
pub struct IntegratedOrientation {
    pub timestamp_us: TimestampUs,
    pub orientation: UnitQuaternion<f64>,
}

#[derive(Debug, Clone)]
pub struct OrientationState {
    pub orientations: Vec<IntegratedOrientation>,
}

impl OrientationState {
    pub fn integrate(samples: &[ImuSample]) -> Self {
        if samples.is_empty() {
            return Self { orientations: Vec::new() };
        }
        let mut orientations = Vec::with_capacity(samples.len());
        let mut q = UnitQuaternion::identity();
        orientations.push(IntegratedOrientation {
            timestamp_us: samples[0].timestamp_us,
            orientation: q,
        });

        for window in samples.windows(2) {
            let prev = &window[0];
            let cur = &window[1];
            let dt_s = (cur.timestamp_us - prev.timestamp_us) as f64 / 1_000_000.0;
            if dt_s <= 0.0 || dt_s > 1.0 {
                // skip large gaps (e.g., pause)
                orientations.push(IntegratedOrientation {
                    timestamp_us: cur.timestamp_us,
                    orientation: q,
                });
                continue;
            }
            let gyro = Vector3::new(cur.gyro_rad_s[0], cur.gyro_rad_s[1], cur.gyro_rad_s[2]);
            let angle = gyro.norm() * dt_s;
            if angle > 1e-9 {
                let axis = gyro.normalize();
                let delta = UnitQuaternion::from_axis_angle(&nalgebra::Unit::new_normalize(axis), angle);
                q = q * delta;
            }
            orientations.push(IntegratedOrientation {
                timestamp_us: cur.timestamp_us,
                orientation: q,
            });
        }
        Self { orientations }
    }

    /// Find orientation at or before timestamp via binary search
    pub fn orientation_at(&self, ts: TimestampUs) -> Option<UnitQuaternion<f64>> {
        if self.orientations.is_empty() {
            return None;
        }
        // binary search by timestamp
        let idx = match self.orientations.binary_search_by_key(&ts, |o| o.timestamp_us) {
            Ok(i) => i,
            Err(i) => {
                if i == 0 {
                    0
                } else {
                    i - 1
                }
            }
        };
        Some(self.orientations[idx].orientation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    fn sample(ts: i64, gyro: [f64; 3]) -> ImuSample {
        ImuSample { timestamp_us: ts, gyro_rad_s: gyro, accel_m_s2: [0.0,0.0,9.81] }
    }

    #[test]
    fn integrate_identity_no_motion() {
        let samples = vec![sample(0, [0.0,0.0,0.0]), sample(1_000_000, [0.0,0.0,0.0])];
        let state = OrientationState::integrate(&samples);
        assert_eq!(state.orientations.len(), 2);
        let angle = state.orientations[0].orientation.angle_to(&state.orientations[1].orientation);
        assert_abs_diff_eq!(angle, 0.0, epsilon=1e-9);
    }

    #[test]
    fn integrate_90deg_yaw() {
        // Rotate around Z at ~pi/2 rad/s for 1 sec => 90 degrees
        let gyro_z = std::f64::consts::FRAC_PI_2;
        let samples: Vec<_> = (0..=100).map(|i| sample(i*10_000, [0.0,0.0,gyro_z])).collect();
        let state = OrientationState::integrate(&samples);
        let first = state.orientations.first().unwrap().orientation;
        let last = state.orientations.last().unwrap().orientation;
        let angle_deg = first.angle_to(&last).to_degrees();
        // Should be ~90 deg (integration with 10ms steps)
        assert!((angle_deg - 90.0).abs() < 2.0, "angle was {angle_deg}");
    }
}
