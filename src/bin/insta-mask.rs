use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use tracing::{info, warn, debug};

use insta_keyframes::image::{DecodedImage, encode::write_mask_png, resize::resize_mask};
use insta_keyframes::insta_mask_manifest::{KeyframesManifest, MaskManifest, ProcessingMeta, MaskFrame, LensMask};
use insta_keyframes::mask::{combine::combine_masks, morphology::dilate, static_mask::StaticMask};
use insta_keyframes::people::vision::{PersonSegmenter, CpuPersonSegmenter, VisionPersonSegmenter, VisionQuality};
use insta_keyframes::shadows::detect::{ShadowConfig, detect_shadows};
use insta_keyframes::temporal::{PropagationConfig, validate::validate_propagation, propagate::propagate_mask};
use insta_keyframes::objects::Detector;
use insta_keyframes::objects::prompt::PromptableSegmenter;

#[derive(Debug, Parser)]
#[command(name = "insta-mask", version, about = "High-throughput mask generation for Insta360 keyframes")]
struct Cli {
    /// Input directory (output of insta-keyframes) containing manifest.json
    #[arg(long, value_name = "DIR")]
    input: PathBuf,

    /// Output directory for cleaned/masks
    #[arg(long, short = 'o', value_name = "DIR")]
    output: PathBuf,

    /// Removal targets: people, operator, shadows, tripod, backpack, chair, vehicle, etc. Repeatable.
    #[arg(long, value_name = "ITEM", action = clap::ArgAction::Append)]
    remove: Vec<String>,

    /// Arbitrary text prompt for fallback segmentation
    #[arg(long, value_name = "PROMPT")]
    prompt: Option<String>,

    /// Manual rect mask x,y,w,h (pixels, per lens). Repeatable.
    #[arg(long, value_name = "RECT", action = clap::ArgAction::Append)]
    rect: Vec<String>,

    /// Polygon JSON file
    #[arg(long, value_name = "FILE")]
    polygon: Option<PathBuf>,

    /// Precomputed mask image
    #[arg(long, value_name = "FILE")]
    mask: Option<PathBuf>,

    /// Preset: insta360-operator, people-only, people-and-shadows, photogrammetry-clean, aggressive-clean
    #[arg(long, value_name = "PRESET")]
    preset: Option<String>,

    /// Also write cleaned images (masked)
    #[arg(long)]
    write_clean_images: bool,

    /// Clean mode: transparent, solid, blur, inpaint (default solid)
    #[arg(long, value_name = "MODE", default_value = "solid")]
    clean_mode: String,

    /// Shadow mode: fast (heuristic) or ml (opt-in neural)
    #[arg(long, value_name = "MODE", default_value = "fast")]
    shadow_mode: String,

    /// People backend quality
    #[arg(long, value_name = "QUALITY", default_value = "fast")]
    people_quality: String,

    /// Inference max dimension (downsample before segmentation)
    #[arg(long, value_name = "N", default_value_t = 768)]
    inference_max_dimension: u32,

    /// Dilation radius for final mask (0=none, 6≈ conservative for reconstruction)
    #[arg(long, value_name = "N", default_value_t = 0)]
    dilate: u32,

    /// Disable temporal propagation
    #[arg(long)]
    no_temporal: bool,

    /// Nadir static mask (operator/tripod cap)
    #[arg(long)]
    static_nadir: bool,

    #[arg(long)]
    no_static_nadir: bool,

    /// Review HTML generation
    #[arg(long)]
    review: bool,

    /// Disable COLMAP-compatible masks/ folder (by default colmap masks are written)
    #[arg(long)]
    no_colmap: bool,

    /// Do not invert masks for COLMAP (by default masks are inverted 255↔0 for COLMAP's 0=masked)
    #[arg(long)]
    no_colmap_invert: bool,

    /// COLMAP masks subdirectory name (default "masks")
    #[arg(long, value_name = "DIR", default_value = "masks")]
    colmap_masks_dir: String,

    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Preset {
    #[value(name = "insta360-operator")]
    Insta360Operator,
    #[value(name = "people-only")]
    PeopleOnly,
    #[value(name = "people-and-shadows")]
    PeopleAndShadows,
    #[value(name = "photogrammetry-clean")]
    PhotogrammetryClean,
    #[value(name = "aggressive-clean")]
    AggressiveClean,
}

fn parse_preset(s: &str) -> Option<Preset> {
    match s {
        "insta360-operator" => Some(Preset::Insta360Operator),
        "people-only" => Some(Preset::PeopleOnly),
        "people-and-shadows" => Some(Preset::PeopleAndShadows),
        "photogrammetry-clean" => Some(Preset::PhotogrammetryClean),
        "aggressive-clean" => Some(Preset::AggressiveClean),
        _ => None,
    }
}

struct ResolvedConfig {
    remove_people: bool,
    remove_operator: bool,
    remove_shadows: bool,
    remove_objects: Vec<String>,
    static_nadir: bool,
    temporal: bool,
    shadow_mode: String,
    people_quality: VisionQuality,
    dilate: u32,
    inference_max_dimension: u32,
    preset_name: Option<String>,
    colmap: bool,
    colmap_invert: bool,
    colmap_masks_dir: String,
}

fn resolve_config(cli: &Cli) -> ResolvedConfig {
    let mut cfg = ResolvedConfig {
        remove_people: false,
        remove_operator: false,
        remove_shadows: false,
        remove_objects: Vec::new(),
        static_nadir: false,
        temporal: !cli.no_temporal,
        shadow_mode: cli.shadow_mode.clone(),
        people_quality: match cli.people_quality.as_str() {
            "accurate" => VisionQuality::Accurate,
            "balanced" => VisionQuality::Balanced,
            _ => VisionQuality::Fast,
        },
        dilate: cli.dilate,
        inference_max_dimension: cli.inference_max_dimension,
        preset_name: cli.preset.clone(),
        colmap: !cli.no_colmap,
        colmap_invert: !cli.no_colmap_invert,
        colmap_masks_dir: cli.colmap_masks_dir.clone(),
    };

    // preset defaults
    if let Some(p) = cli.preset.as_deref().and_then(parse_preset) {
        match p {
            Preset::Insta360Operator => {
                cfg.remove_people = true;
                cfg.remove_shadows = true;
                cfg.static_nadir = true;
                cfg.temporal = true;
            }
            Preset::PeopleOnly => {
                cfg.remove_people = true;
            }
            Preset::PeopleAndShadows => {
                cfg.remove_people = true;
                cfg.remove_shadows = true;
            }
            Preset::PhotogrammetryClean => {
                cfg.remove_people = true;
                cfg.remove_shadows = true;
                cfg.static_nadir = true;
                cfg.temporal = true;
            }
            Preset::AggressiveClean => {
                cfg.remove_people = true;
                cfg.remove_operator = true;
                cfg.remove_shadows = true;
                cfg.static_nadir = true;
                cfg.temporal = true;
                cfg.dilate = 12;
            }
        }
    }

    // explicit --remove overrides/adds
    for r in &cli.remove {
        match r.to_lowercase().as_str() {
            "people" | "person" => cfg.remove_people = true,
            "operator" => cfg.remove_operator = true,
            "shadows" | "shadow" => cfg.remove_shadows = true,
            other => cfg.remove_objects.push(other.to_string()),
        }
    }

    if cli.static_nadir { cfg.static_nadir = true; }
    if cli.no_static_nadir { cfg.static_nadir = false; }

    // If no explicit removes and no preset, default to people (most common)
    if !cfg.remove_people && !cfg.remove_operator && !cfg.remove_shadows && cfg.remove_objects.is_empty() && cli.remove.is_empty() && cli.preset.is_none() {
        cfg.remove_people = true;
    }

    cfg
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let filter = match cli.verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)))
        .init();

    let cfg = resolve_config(&cli);
    info!("insta-mask preset={:?} people={} operator={} shadows={} nadir={} temporal={} objects={:?}", cfg.preset_name, cfg.remove_people, cfg.remove_operator, cfg.remove_shadows, cfg.static_nadir, cfg.temporal, cfg.remove_objects);

    let input_manifest_path = cli.input.join("manifest.json");
    let manifest_str = std::fs::read_to_string(&input_manifest_path)
        .with_context(|| format!("read input manifest {}", input_manifest_path.display()))?;
    let kf_manifest: KeyframesManifest = serde_json::from_str(&manifest_str)
        .with_context(|| format!("parse {}", input_manifest_path.display()))?;

    info!("loaded {} frames from {}", kf_manifest.frames.len(), input_manifest_path.display());

    std::fs::create_dir_all(&cli.output).context("create output dir")?;

    // Choose segmenter
    let segmenter: Box<dyn PersonSegmenter> = if cfg.remove_people || cfg.remove_operator {
        // Prefer Vision on macOS
        Box::new(VisionPersonSegmenter::new(cfg.people_quality))
    } else {
        Box::new(CpuPersonSegmenter::new())
    };
    let segmenter = &segmenter;

    let shadow_cfg = ShadowConfig {
        enabled: cfg.remove_shadows,
        expand_px: 12,
        darken_threshold: 18,
        dilate: 3,
    };

    let prop_cfg = PropagationConfig {
        enabled: cfg.temporal,
        max_rotation_deg: 8.0,
        max_translation_px: 24,
    };

    // Prepare per-frame processing
    let start = std::time::Instant::now();
    let mut results: Vec<(usize, PathBuf, PathBuf, f64, f64)> = Vec::new(); // id, mask_a, mask_b, frac_a, frac_b

    // For temporal propagation we need state per lens across frames sorted by id
    let mut prev_masks_a: Option<Vec<u8>> = None;
    let mut prev_masks_b: Option<Vec<u8>> = None;
    let mut propagated_count = 0usize;
    let mut full_seg_count = 0usize;

    // We need sequential for propagation, but parallel for decode. For MVP we do sequential + rayon for lens A/B within frame.
    // For higher throughput, group frames and propagate only when valid.

    let mut mask_frames = Vec::new();

    for (idx, entry) in kf_manifest.frames.iter().enumerate() {
        let t_frame = std::time::Instant::now();
        // Resolve source paths relative to input dir
        let src_a = cli.input.join(&entry.lens_a);
        let src_b = cli.input.join(&entry.lens_b);

        // Check files exist
        if !src_a.exists() {
            warn!("missing lens_a {} for frame {}", src_a.display(), entry.id);
            continue;
        }
        if !src_b.exists() {
            warn!("missing lens_b {} for frame {}", src_b.display(), entry.id);
            continue;
        }

        // Load images in parallel
        let (img_a_res, img_b_res) = rayon::join(
            || DecodedImage::from_path(&src_a),
            || DecodedImage::from_path(&src_b),
        );
        let img_a = img_a_res?;
        let img_b = img_b_res?;

        // Try temporal propagation: if previous mask exists and rotation small, reuse shifted mask
        let can_propagate_a = prev_masks_a.is_some() && validate_propagation(entry.motion.rotation_delta_deg, &prop_cfg);
        let can_propagate_b = prev_masks_b.is_some() && validate_propagation(entry.motion.rotation_delta_deg, &prop_cfg);

        let (mask_a, frac_a) = process_lens(
            &img_a,
            entry.motion.rotation_delta_deg,
            &cfg,
            segmenter.as_ref(),
            &shadow_cfg,
            can_propagate_a.then(|| prev_masks_a.as_deref()).flatten(),
            &cli,
        )?;

        let (mask_b, frac_b) = process_lens(
            &img_b,
            entry.motion.rotation_delta_deg,
            &cfg,
            segmenter.as_ref(),
            &shadow_cfg,
            can_propagate_b.then(|| prev_masks_b.as_deref()).flatten(),
            &cli,
        )?;

        if can_propagate_a { propagated_count += 1; } else { full_seg_count += 1; }

        // Write masks (primary: frames/000001/lens_a.mask.png, 0=retain 255=exclude)
        let out_a_mask = cli.output.join(format!("frames/{:06}/lens_a.mask.png", entry.id));
        let out_b_mask = cli.output.join(format!("frames/{:06}/lens_b.mask.png", entry.id));
        write_mask_png(&mask_a, img_a.width, img_a.height, &out_a_mask)?;
        write_mask_png(&mask_b, img_b.width, img_b.height, &out_b_mask)?;

        // COLMAP-compatible masks by default: masks/000001_lens_a.png with same basename, inverted (COLMAP: 0=masked)
        if cfg.colmap {
            let colmap_a = cli.output.join(format!("{}/{:06}_lens_a.png", cfg.colmap_masks_dir, entry.id));
            let colmap_b = cli.output.join(format!("{}/{:06}_lens_b.png", cfg.colmap_masks_dir, entry.id));
            let colmap_mask_a = if cfg.colmap_invert { insta_keyframes::mask::invert(&mask_a) } else { mask_a.clone() };
            let colmap_mask_b = if cfg.colmap_invert { insta_keyframes::mask::invert(&mask_b) } else { mask_b.clone() };
            write_mask_png(&colmap_mask_a, img_a.width, img_a.height, &colmap_a)?;
            write_mask_png(&colmap_mask_b, img_b.width, img_b.height, &colmap_b)?;
        }

        // Copy source frames? Default per spec: cleaned/frames/000001/lens_a.jpg etc? Spec says cleaned/frames/.. lens_a.jpg + mask. For MVP, copy source JPEG to output frames dir if not already.
        let out_a_jpg = cli.output.join(format!("frames/{:06}/lens_a.jpg", entry.id));
        let out_b_jpg = cli.output.join(format!("frames/{:06}/lens_b.jpg", entry.id));
        // Only copy if not exists or --write-clean-images handling; but spec default says cleaned/ contains both jpg and mask.
        // We'll copy.
        if !out_a_jpg.exists() {
            if let Some(parent) = out_a_jpg.parent() { std::fs::create_dir_all(parent)?; }
            std::fs::copy(&src_a, &out_a_jpg).with_context(|| format!("copy {} -> {}", src_a.display(), out_a_jpg.display()))?;
        }
        if !out_b_jpg.exists() {
            if let Some(parent) = out_b_jpg.parent() { std::fs::create_dir_all(parent)?; }
            std::fs::copy(&src_b, &out_b_jpg).with_context(|| format!("copy {} -> {}", src_b.display(), out_b_jpg.display()))?;
        }

        if cli.write_clean_images {
            let clean_a = cli.output.join(format!("frames/{:06}/lens_a.clean.jpg", entry.id));
            let clean_b = cli.output.join(format!("frames/{:06}/lens_b.clean.jpg", entry.id));
            insta_keyframes::image::encode::write_clean_image(&img_a, Some(&mask_a), &clean_a, &cli.clean_mode)?;
            insta_keyframes::image::encode::write_clean_image(&img_b, Some(&mask_b), &clean_b, &cli.clean_mode)?;
        }

        // Update propagation state with *final* masks
        prev_masks_a = Some(mask_a.clone());
        prev_masks_b = Some(mask_b.clone());

        let rel_a = out_a_mask.strip_prefix(&cli.output).unwrap_or(&out_a_mask).to_string_lossy().to_string();
        let rel_b = out_b_mask.strip_prefix(&cli.output).unwrap_or(&out_b_mask).to_string_lossy().to_string();
        let src_rel_a = path_relative(&cli.input, &src_a);
        let src_rel_b = path_relative(&cli.input, &src_b);

        mask_frames.push(MaskFrame {
            id: entry.id,
            lens_a: LensMask { source: src_rel_a, mask: rel_a, removed_fraction: frac_a },
            lens_b: LensMask { source: src_rel_b, mask: rel_b, removed_fraction: frac_b },
        });

        results.push((entry.id, out_a_mask, out_b_mask, frac_a, frac_b));
        let dt = t_frame.elapsed();
        let total = kf_manifest.frames.len();
        info!("[{}/{}] {:06} lens_a {:.1}% lens_b {:.1}%  {:>4}ms{}",
            idx + 1, total, entry.id, frac_a * 100.0, frac_b * 100.0, dt.as_millis(),
            if can_propagate_a || can_propagate_b { " [propagated]" } else { "" }
        );
        debug!("frame {:06} a_frac {:.3} b_frac {:.3} propagated_a={}", entry.id, frac_a, frac_b, can_propagate_a);
    }

    let elapsed = start.elapsed();
    let total_images = mask_frames.len() * 2;
    let mps = if elapsed.as_secs_f64() > 0.0 { total_images as f64 / elapsed.as_secs_f64() } else { 0.0 };

    // Write output manifest
    let out_manifest = MaskManifest {
        schema_version: "1.0".to_string(),
        source_manifest: input_manifest_path.strip_prefix(&cli.output).unwrap_or(&input_manifest_path).to_string_lossy().to_string(),
        processing: ProcessingMeta {
            people_backend: segmenter.name().to_string(),
            people_quality: format!("{:?}", cfg.people_quality).to_lowercase(),
            shadow_mode: cfg.shadow_mode.clone(),
            temporal_propagation: cfg.temporal,
            preset: cfg.preset_name.clone(),
            colmap_masks: if cfg.colmap { Some(cfg.colmap_masks_dir.clone()) } else { None },
            colmap_invert: cfg.colmap_invert,
        },
        frames: mask_frames,
    };
    let out_manifest_path = cli.output.join("manifest.json");
    std::fs::write(&out_manifest_path, serde_json::to_string_pretty(&out_manifest)?)?;
    info!("wrote {}", out_manifest_path.display());

    // Metrics
    info!("Processed: {} frames ({} lens images) in {:.1}s", results.len(), total_images, elapsed.as_secs_f64());
    info!("Full segmentation: {}  Propagated: {}  Throughput: {:.1} images/s", full_seg_count, propagated_count, mps);
    if cfg.temporal {
        let pct = if total_images > 0 { propagated_count as f64 * 100.0 / results.len() as f64 } else { 0.0 };
        info!("Propagation rate: {:.1}%", pct);
    }
    if cfg.colmap {
        info!("COLMAP masks: {}/ (inverted={}) — use: colmap feature_extractor --image_path {}/frames --ImageReader.masks {}/{}", cfg.colmap_masks_dir, cfg.colmap_invert, cli.output.display(), cli.output.display(), cfg.colmap_masks_dir);
    }

    // Review HTML
    if cli.review {
        write_review_html(&cli.output, &out_manifest)?;
        info!("wrote review.html");
    }

    // Optional: also copy input manifest? Already referenced.

    Ok(())
}

fn process_lens(
    img: &DecodedImage,
    rotation_delta: f64,
    cfg: &ResolvedConfig,
    segmenter: &dyn PersonSegmenter,
    shadow_cfg: &ShadowConfig,
    prev_mask: Option<&[u8]>,
    cli: &Cli,
) -> Result<(Vec<u8>, f64)> {
    let (w, h) = (img.width, img.height);
    let n = (w * h) as usize;
    let mut base_masks: Vec<Vec<u8>> = Vec::new();

    // Tier 0: deterministic static masks
    if cfg.static_nadir {
        let nadir = StaticMask::Nadir { radius_ratio: 0.12 }.generate(w, h);
        base_masks.push(nadir);
    }
    if cfg.remove_operator {
        // operator often overlaps nadir but add slightly larger
        let op = StaticMask::Nadir { radius_ratio: 0.15 }.generate(w, h);
        base_masks.push(op);
    }
    for rect_str in &cli.rect {
        if let Some(mask) = parse_rect_mask(rect_str, w, h) {
            base_masks.push(mask);
        }
    }
    if let Some(poly_path) = &cli.polygon {
        if let Ok(poly) = load_polygon(poly_path) {
            let m = StaticMask::Polygon { points: poly }.generate(w, h);
            base_masks.push(m);
        }
    }
    if let Some(mask_path) = &cli.mask {
        if let Ok(m) = StaticMask::from_image_path(mask_path) {
            base_masks.push(m.generate(w, h));
        }
    }

    // Tier 1: person segmentation (with inference resize)
    let mut person_mask = vec![0u8; n];
    let need_person = cfg.remove_people || cfg.remove_operator || cfg.remove_shadows;
    if need_person {
        if let Some(prev) = prev_mask {
            // Validate propagation: if rotation small, propagate
            let prop_cfg = PropagationConfig { enabled: cfg.temporal, max_rotation_deg: 8.0, max_translation_px: 24 };
            if validate_propagation(rotation_delta, &prop_cfg) {
                // Simple propagate (zero shift for fisheye; MVP)
                person_mask = propagate_mask(prev, w, h, 0, 0);
            } else {
                person_mask = run_person_segmentation(img, cfg, segmenter)?;
            }
        } else {
            person_mask = run_person_segmentation(img, cfg, segmenter)?;
        }
        base_masks.push(person_mask.clone());
    }

    // Tier 2: known object detection + ROI segmentation (stub)
    if !cfg.remove_objects.is_empty() {
        // DummyDetector returns none; keep placeholder
        let dummy = insta_keyframes::objects::detector::DummyDetector;
        let _dets = dummy.detect(img, &cfg.remove_objects)?;
        // ROI segmenter would be called per bbox; skipped for MVP
    }

    // Tier 3: promptable (stub)
    if let Some(prompt) = &cli.prompt {
        let ps = insta_keyframes::objects::prompt::DummyPromptSegmenter;
        let pm = ps.segment_prompt(img, prompt)?;
        base_masks.push(pm);
    }

    // Tier 1b: shadows heuristic
    let mut shadow_mask = vec![0u8; n];
    if cfg.remove_shadows && !person_mask.is_empty() {
        shadow_mask = detect_shadows(&person_mask, img, shadow_cfg);
        base_masks.push(shadow_mask.clone());
    }

    // Combine
    let combined = combine_masks(&base_masks, &[], w, h);

    // Morphology
    let mut final_mask = combined;
    if cfg.dilate > 0 {
        final_mask = dilate(&final_mask, w, h, cfg.dilate);
    }

    let removed = final_mask.iter().filter(|&&v| v > 127).count() as f64 / n as f64;

    Ok((final_mask, removed))
}

fn run_person_segmentation(img: &DecodedImage, cfg: &ResolvedConfig, segmenter: &dyn PersonSegmenter) -> Result<Vec<u8>> {
    // Inference resize per spec §12
    let max_dim = cfg.inference_max_dimension;
    let (data, rw, rh, scale) = if img.width > max_dim || img.height > max_dim {
        let (d, nw, nh, s) = insta_keyframes::image::resize::resize_image(&img.data, img.width, img.height, max_dim);
        (d, nw, nh, s)
    } else {
        (img.data.clone(), img.width, img.height, 1.0)
    };

    let small_img = DecodedImage { width: rw, height: rh, data };
    let small_mask = segmenter.segment(&small_img)?;
    if (rw, rh) == (img.width, img.height) {
        Ok(small_mask)
    } else {
        // upsample
        Ok(resize_mask(&small_mask, rw, rh, img.width, img.height))
    }
}

fn parse_rect_mask(s: &str, w: u32, h: u32) -> Option<Vec<u8>> {
    // format x,y,w,h (absolute or 0..1 normalized if <1?)
    let parts: Vec<f32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    if parts.len() != 4 { return None; }
    let (x, y, bw, bh) = (parts[0], parts[1], parts[2], parts[3]);
    let (x, y, bw, bh) = if x < 1.0 && y < 1.0 && bw <= 1.0 && bh <= 1.0 {
        ((x * w as f32) as u32, (y * h as f32) as u32, (bw * w as f32) as u32, (bh * h as f32) as u32)
    } else {
        (x as u32, y as u32, bw as u32, bh as u32)
    };
    let m = StaticMask::Rect { x, y, w: bw, h: bh }.generate(w, h);
    Some(m)
}

fn load_polygon(path: &Path) -> Result<Vec<(f32,f32)>> {
    let s = std::fs::read_to_string(path)?;
    let v: serde_json::Value = serde_json::from_str(&s)?;
    // expect [[x,y],...] normalized
    let mut out = Vec::new();
    if let Some(arr) = v.as_array() {
        for pt in arr {
            if let Some(pair) = pt.as_array() {
                if pair.len() >= 2 {
                    let x = pair[0].as_f64().unwrap_or(0.0) as f32;
                    let y = pair[1].as_f64().unwrap_or(0.0) as f32;
                    out.push((x, y));
                }
            }
        }
    }
    Ok(out)
}

fn path_relative(base: &Path, target: &Path) -> String {
    target.strip_prefix(base).unwrap_or(target).to_string_lossy().to_string()
}

fn write_review_html(output: &Path, manifest: &MaskManifest) -> Result<()> {
    let path = output.join("review.html");
    let mut html = String::new();
    html.push_str(r#"<!doctype html><meta charset="utf-8"><title>insta-mask review</title><style>body{font-family:sans-serif} .row{display:flex;gap:8px;margin:12px 0} img{max-width:280px;max-height:280px;border:1px solid #ccc} .meta{font-size:12px;color:#555}</style><h1>insta-mask review</h1>"#);
    for f in &manifest.frames {
        let a_src = f.lens_a.source.clone();
        let a_mask = f.lens_a.mask.clone();
        let b_src = f.lens_b.source.clone();
        let b_mask = f.lens_b.mask.clone();
        html.push_str(&format!(r#"<div class="row"><div><div class="meta">#{:06} lens_a {:.1}%</div><img src="{}"><img src="{}"></div><div><div class="meta">lens_b {:.1}%</div><img src="{}"><img src="{}"></div></div>"#, f.id, f.lens_a.removed_fraction*100.0, a_src, a_mask, f.lens_b.removed_fraction*100.0, b_src, b_mask));
    }
    std::fs::write(path, html)?;
    Ok(())
}
