#[derive(Debug, Clone)]
pub struct PairScores {
    pub temporal: f32,
    pub overlap: f32,
    pub rotation: f32,
    pub cross_lens: f32,
    pub loop_score: f32,
    pub safety: f32,
    pub total: f32,
}

impl PairScores {
    pub fn new(temporal: f32, overlap: f32, rotation: f32, cross_lens: f32, loop_score: f32, safety: f32) -> Self {
        let total = temporal*0.2 + overlap*0.3 + rotation*0.2 + cross_lens*0.1 + loop_score*0.1 + safety*0.1;
        Self { temporal, overlap, rotation, cross_lens, loop_score, safety, total }
    }
}
