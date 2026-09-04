#[derive(Debug, Clone)]
pub struct DecodedFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>, // RGB24
    pub timestamp_us: i64,
}

impl DecodedFrame {
    pub fn to_gray(&self) -> Vec<u8> {
        let mut gray = Vec::with_capacity((self.width * self.height) as usize);
        for chunk in self.data.chunks_exact(3) {
            let r = chunk[0] as f32;
            let g = chunk[1] as f32;
            let b = chunk[2] as f32;
            let lum = (0.299 * r + 0.587 * g + 0.114 * b) as u8;
            gray.push(lum);
        }
        gray
    }
}
