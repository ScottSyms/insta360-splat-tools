use crate::imu::motion::{angular_distance_deg, angular_velocity_deg_s};
use crate::imu::orientation::OrientationState;
use crate::insta360::telemetry::{ImuSample, TimestampUs};
use crate::selection::policy::SelectionPolicy;
use nalgebra::UnitQuaternion;

#[derive(Debug, Clone)]
pub struct Candidate {
    pub timestamp_us: TimestampUs,
    pub elapsed_us: i64,
    pub rotation_delta_deg: f64,
    pub angular_velocity_deg_s: f64,
    pub acceleration_score: f64,
    pub reason: String,
    /// Absolute IMU-integrated rig orientation at this candidate's timestamp (world_from_rig, §8.2)
    pub world_from_rig: UnitQuaternion<f64>,
}

pub fn select_candidates(
    samples: &[ImuSample],
    orientations: &OrientationState,
    policy: &SelectionPolicy,
    duration_us: i64,
) -> Vec<Candidate> {
    if samples.is_empty() || orientations.orientations.is_empty() {
        return Vec::new();
    }

    let mut candidates = Vec::new();
    let mut last_keyframe_ts = samples.first().map(|s| s.timestamp_us).unwrap_or(0);
    // Always include first frame as keyframe?
    // We will emit it as first candidate at t=0
    let first_orientation = orientations.orientation_at(last_keyframe_ts).unwrap();

    // We will iterate over samples chronologically and check thresholds
    // To avoid O(N^2) we use pointer to last_keyframe orientation
    let mut last_orientation = first_orientation;

    // Always select t=0
    candidates.push(Candidate {
        timestamp_us: last_keyframe_ts,
        elapsed_us: 0,
        rotation_delta_deg: 0.0,
        angular_velocity_deg_s: 0.0,
        acceleration_score: 0.0,
        reason: "initial".to_string(),
        world_from_rig: first_orientation,
    });

    for sample in samples.iter() {
        let ts = sample.timestamp_us;
        if ts <= last_keyframe_ts {
            continue;
        }
        if ts > duration_us {
            break;
        }
        let elapsed = ts - last_keyframe_ts;
        // Enforce minimum interval unless maximum forces
        if elapsed < policy.minimum_interval_us {
            continue;
        }

        let cur_orientation = match orientations.orientation_at(ts) {
            Some(o) => o,
            None => continue,
        };
        let rotation_delta = angular_distance_deg(&last_orientation, &cur_orientation);
        let ang_vel = angular_velocity_deg_s(&sample.gyro_rad_s);
        let accel_score = (sample.accel_m_s2[0].powi(2) + sample.accel_m_s2[1].powi(2) + sample.accel_m_s2[2].powi(2)).sqrt() / 9.81;

        let mut triggered = false;
        let mut reason = String::new();

        if rotation_delta >= policy.rotation_threshold_deg {
            triggered = true;
            reason = format!("rotation {rotation_delta:.2}deg >= {}", policy.rotation_threshold_deg);
        }
        if elapsed >= policy.maximum_interval_us {
            triggered = true;
            if reason.is_empty() {
                reason = format!("max_interval {}ms", elapsed / 1000);
            } else {
                reason.push_str(&" + max_interval".to_string());
            }
        }

        if !triggered {
            continue;
        }

        // Angular velocity check: delay if too fast
        if let Some(thresh) = policy.angular_velocity_threshold_deg_s {
            if ang_vel > thresh {
                tracing::debug!("delay candidate at {}us: angular velocity {ang_vel:.1} > {thresh}", ts);
                continue;
            }
        }

        // Visual validation hook would go here (MVP: disabled or check flow placeholder)
        // For now, accept.

        candidates.push(Candidate {
            timestamp_us: ts,
            elapsed_us: elapsed,
            rotation_delta_deg: rotation_delta,
            angular_velocity_deg_s: ang_vel,
            acceleration_score: accel_score,
            reason,
            world_from_rig: cur_orientation,
        });
        last_keyframe_ts = ts;
        last_orientation = cur_orientation;
    }

    // Ensure we have at least one more if duration not covered? Already handled by max_interval loop.

    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imu::orientation::OrientationState;
    use crate::insta360::telemetry::ImuSample;

    fn sample(ts: i64, gyro: [f64;3]) -> ImuSample {
        ImuSample { timestamp_us: ts, gyro_rad_s: gyro, accel_m_s2: [0.0,0.0,9.81] }
    }

    #[test]
    fn selects_on_rotation() {
        // constant rotation 10 deg/s around Z => 0.174 rad/s
        let gyro = [0.0,0.0, 0.1745329]; // 10 deg/s
        let samples: Vec<_> = (0..500).map(|i| sample(i*10_000, gyro)).collect(); // 5 sec, 10ms steps
        let orientations = OrientationState::integrate(&samples);
        let policy = SelectionPolicy {
            rotation_threshold_deg: 5.0,
            minimum_interval_us: 300_000,
            maximum_interval_us: 2_500_000,
            angular_velocity_threshold_deg_s: None,
            visual_enabled: false,
            minimum_flow_score: 0.1,
        };
        let cands = select_candidates(&samples, &orientations, &policy, 5_000_000);
        // first at 0, then every ~500ms (5deg at 10deg/s) but min 300ms => ~500ms
        assert!(cands.len() >= 5, "got {} candidates", cands.len());
        assert_eq!(cands[0].timestamp_us, 0);
    }

    #[test]
    fn selects_on_max_interval_when_stationary() {
        let samples: Vec<_> = (0..500).map(|i| sample(i*10_000, [0.0,0.0,0.0])).collect();
        let orientations = OrientationState::integrate(&samples);
        let policy = SelectionPolicy {
            rotation_threshold_deg: 5.0,
            minimum_interval_us: 300_000,
            maximum_interval_us: 1_000_000,
            angular_velocity_threshold_deg_s: None,
            visual_enabled: false,
            minimum_flow_score: 0.1,
        };
        let cands = select_candidates(&samples, &orientations, &policy, 5_000_000);
        // should select roughly every 1s
        assert!(cands.len() >= 4, "got {} {:?}", cands.len(), cands);
    }
}
