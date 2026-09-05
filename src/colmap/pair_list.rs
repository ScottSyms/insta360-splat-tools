use crate::pairs::graph::CandidatePair;
use std::path::Path;

pub fn write_candidate_pairs_txt(pairs: &[CandidatePair], path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
    let mut s = String::new();
    for p in pairs {
        s.push_str(&format!("{} {}\n", p.image_a, p.image_b));
    }
    std::fs::write(path, s)?;
    Ok(())
}

pub fn write_candidate_pairs_json(pairs: &[CandidatePair], path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
    std::fs::write(path, serde_json::to_string_pretty(pairs)?)?;
    Ok(())
}
