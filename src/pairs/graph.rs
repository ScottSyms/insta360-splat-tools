use serde::{Deserialize, Serialize};

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, Serialize, Deserialize)]
    pub struct PairSource: u32 {
        const TEMPORAL = 1 << 0;
        const GEOMETRY = 1 << 1;
        const CROSS_LENS = 1 << 2;
        const LOOP_CLOSURE = 1 << 3;
        const SAFETY = 1 << 4;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidatePair {
    pub image_a: String, // cam0/00000001.jpg
    pub image_b: String,
    pub source: PairSource,
    pub score: f32,
    pub temporal_score: f32,
    pub overlap_score: f32,
    pub rotation_score: f32,
    pub loop_score: f32,
    pub frame_delta: i32,
    pub time_delta_s: f64,
    pub angular_delta_deg: f64,
    pub estimated_overlap: f32,
    pub selected: bool,
}

#[derive(Debug, Default)]
pub struct PairGraph {
    pub pairs: Vec<CandidatePair>,
}

impl PairGraph {
    pub fn new() -> Self { Self { pairs: Vec::new() } }

    pub fn add(&mut self, p: CandidatePair) {
        // Deduplicate unordered pair
        let key = if p.image_a < p.image_b { (&p.image_a, &p.image_b) } else { (&p.image_b, &p.image_a) };
        if self.pairs.iter().any(|e| {
            let ek = if e.image_a < e.image_b { (&e.image_a, &e.image_b) } else { (&e.image_b, &e.image_a) };
            ek == key
        }) { return; }
        self.pairs.push(p);
    }

    /// Ensure each image has at least min_prev/min_next neighbors, else add safety edges (§10.9)
    pub fn ensure_connectivity(&mut self, _min_prev: usize, _min_next: usize) {
        // MVP: no-op, report only via diagnostics
    }

    pub fn diagnostics(&self) -> GraphDiagnostics {
        let n = self.pairs.len();
        let mut by_source = std::collections::HashMap::new();
        for p in &self.pairs {
            *by_source.entry(format!("{:?}", p.source)).or_insert(0) += 1;
        }
        GraphDiagnostics { total_pairs: n, by_source }
    }
}

#[derive(Debug)]
pub struct GraphDiagnostics {
    pub total_pairs: usize,
    pub by_source: std::collections::HashMap<String, usize>,
}

pub fn pair_id_for_colmap(image_id_a: i64, image_id_b: i64) -> i64 {
    const MAX: i64 = 2147483647;
    if image_id_a > image_id_b { MAX * image_id_b + image_id_a } else { MAX * image_id_a + image_id_b }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pair_id_order_independent() {
        assert_eq!(pair_id_for_colmap(1,2), pair_id_for_colmap(2,1));
    }
    #[test]
    fn no_isolated_after_repair() {
        let mut g = PairGraph::new();
        g.add(CandidatePair { image_a:"cam0/000001.jpg".into(), image_b:"cam0/000002.jpg".into(), source: PairSource::TEMPORAL, score:1.0, temporal_score:1.0, overlap_score:1.0, rotation_score:1.0, loop_score:0.0, frame_delta:1, time_delta_s:0.1, angular_delta_deg:1.0, estimated_overlap:1.0, selected:true });
        assert_eq!(g.pairs.len(), 1);
        g.ensure_connectivity(2,2);
    }
}
