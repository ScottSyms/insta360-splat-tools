use crate::pairs::graph::{CandidatePair, PairSource};

/// Visual retrieval stub: in v1, no automatic loop closure; user supplies via --loop-closure
pub fn loop_closure_edges(candidates: Vec<CandidatePair>) -> Vec<CandidatePair> {
    candidates.into_iter().map(|mut p| { p.source |= PairSource::LOOP_CLOSURE; p.loop_score = 0.8; p }).collect()
}
