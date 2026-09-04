use nalgebra::UnitQuaternion;

pub fn angular_distance_deg(a: &UnitQuaternion<f64>, b: &UnitQuaternion<f64>) -> f64 {
    a.angle_to(b).to_degrees()
}

pub fn angular_velocity_deg_s(gyro_rad_s: &[f64; 3]) -> f64 {
    let mag = (gyro_rad_s[0].powi(2) + gyro_rad_s[1].powi(2) + gyro_rad_s[2].powi(2)).sqrt();
    mag.to_degrees()
}

pub fn acceleration_magnitude(accel: &[f64; 3]) -> f64 {
    (accel[0].powi(2) + accel[1].powi(2) + accel[2].powi(2)).sqrt()
}
