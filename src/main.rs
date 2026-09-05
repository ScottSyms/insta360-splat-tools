mod cli;
mod config;
mod error;
mod imu;
mod insta360;
mod output;
mod selection;
mod video;
mod vision;

use anyhow::Context;
use clap::Parser;
use config::{Config, Preset};
use insta360::telemetry::{ImuSample, TelemetrySource};
use std::path::{Path, PathBuf};

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();

    let filter = match cli.verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .init();

    let command = cli.command.clone();
    if let Some(cmd) = command {
        match cmd {
            cli::Commands::Analyze { video_a, video_b, input_directory, output } => {
                let out = output.or_else(|| cli.output.clone()).ok_or_else(|| anyhow::anyhow!("--output required"))?;
                return run_analyze(cli, video_a, video_b, input_directory, out);
            }
            cli::Commands::Preview { video_a, video_b, input_directory, output } => {
                let out = output.or_else(|| cli.output.clone()).ok_or_else(|| anyhow::anyhow!("--output required"))?;
                return run_analyze(cli, video_a, video_b, input_directory, out);
            }
            cli::Commands::Build { front, rear, input_directory, project, calibration, camera } => {
                return handle_build(cli, front, rear, input_directory, project, calibration, camera);
            }
            cli::Commands::ColmapInit { project, database, calibration } => {
                return handle_colmap_init(project, database, calibration);
            }
            cli::Commands::ColmapPairs { project, database, strategy, temporal_before, temporal_after, min_neighbors, loop_closure, same_frame } => {
                return handle_colmap_pairs(project, database, strategy, temporal_before, temporal_after, min_neighbors, loop_closure, same_frame);
            }
            cli::Commands::ColmapPrepare { project, masks } => {
                return handle_colmap_prepare(project, masks);
            }
            cli::Commands::ColmapFeatures { project } => {
                return handle_colmap_features(project);
            }
            cli::Commands::ColmapMatch { project, rig_verification } => {
                return handle_colmap_match(project, rig_verification);
            }
            cli::Commands::ColmapMap { project, fix_rig } => {
                return handle_colmap_map(project, fix_rig);
            }
            cli::Commands::ColmapDiagnose { project } => {
                return handle_colmap_diagnose(project);
            }
            cli::Commands::Extract { project } => {
                return handle_extract(project);
            }
        }
    }

    // Legacy: no subcommand, use output
    let out = cli.output.clone().ok_or_else(|| anyhow::anyhow!("--output required (or use subcommand --project)"))?;
    let mut legacy_cli = cli;
    legacy_cli.output = Some(out.clone());
    run(legacy_cli, false)
}

fn resolve_pair(cli: &cli::Cli) -> anyhow::Result<(PathBuf, PathBuf)> {
    if let (Some(a), Some(b)) = (&cli.video_a, &cli.video_b) {
        return Ok((a.clone(), b.clone()));
    }
    if let Some(dir) = &cli.input_directory {
        let (a, b) = insta360::files::discover_pair(dir)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        return Ok((a, b));
    }
    Err(anyhow::anyhow!(
        "must provide --video-a/--video-b or --input-directory"
    ))
}

fn build_config(cli: &cli::Cli) -> Config {
    let mut cfg = Config::default();
    if let Some(preset_name) = &cli.preset {
        if let Some(preset) = Preset::parse(preset_name) {
            preset.apply(&mut cfg);
        } else {
            tracing::warn!("unknown preset: {}", preset_name);
        }
    }
    if let Some(v) = cli.rotation_threshold {
        cfg.selection.rotation_threshold_deg = v;
    }
    if let Some(s) = &cli.min_interval {
        if let Ok(ms) = cli::parse_duration_ms(s) {
            cfg.selection.minimum_interval_ms = ms;
        }
    }
    if let Some(s) = &cli.max_interval {
        if let Ok(ms) = cli::parse_duration_ms(s) {
            cfg.selection.maximum_interval_ms = ms;
        }
    }
    if cli.optical_flow {
        cfg.visual.enabled = true;
    }
    if cli.no_reject_blur {
        cfg.quality.blur_filter = false;
    }
    if cli.reject_blur {
        cfg.quality.blur_filter = true;
    }
    if let Some(fmt) = &cli.format {
        cfg.output.format = fmt.clone();
    }
    if let Some(q) = cli.jpeg_quality {
        cfg.output.jpeg_quality = q;
    }
    if cli.flat_naming {
        cfg.output.flat_naming = true;
    }
    cfg
}

fn run(cli: cli::Cli, dry_run: bool) -> anyhow::Result<()> {
    let (video_a, video_b) = resolve_pair(&cli)?;
    let cfg = build_config(&cli);
    let output_root = cli.output.as_ref().unwrap().clone();

    tracing::info!("validating pair: {} + {}", video_a.display(), video_b.display());
    let pair = insta360::files::validate_pair(&video_a, &video_b, cfg.sync.max_lens_skew_ms)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    tracing::info!(
        "streams: a {}x{} {:.2}fps {}ms, b {}x{} {:.2}fps {}ms",
        pair.info_a.width,
        pair.info_a.height,
        pair.info_a.frame_rate,
        pair.info_a.duration_ms,
        pair.info_b.width,
        pair.info_b.height,
        pair.info_b.frame_rate,
        pair.info_b.duration_ms
    );

    let sync = insta360::sync::synchronize(&pair, cfg.sync.max_lens_skew_ms)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let mut samples: Vec<ImuSample> = Vec::new();
    let mut camera_model = pair.camera_model.clone();

    if let Some(tel_path) = &pair.telemetry_file {
        tracing::info!("extracting telemetry from {}", tel_path.display());
        let mut source = insta360::telemetry::Insta360TelemetrySource::new(tel_path);
        match source.samples() {
            Ok(mut s) => {
                camera_model = source.camera_model().or(camera_model);
                tracing::info!("telemetry: {} samples, model {:?}", s.len(), camera_model);
                let filtered = imu::filters::preprocess(s.clone(), &imu::filters::FilterConfig::default());
                s = filtered;
                samples = s;
            }
            Err(e) => {
                tracing::warn!("telemetry extraction failed: {} — falling back to time-based selection", e);
            }
        }
    } else {
        tracing::warn!("no telemetry file — using time-based fallback");
    }

    if samples.is_empty() {
        tracing::info!("synthesizing IMU samples for fallback selection (no gyro)");
        let dur_us = if sync.duration_us > 0 { sync.duration_us } else { 30_000_000 };
        let step = 10_000;
        let mut ts = 0;
        while ts < dur_us {
            samples.push(ImuSample {
                timestamp_us: ts,
                gyro_rad_s: [0.0, 0.0, 0.0],
                accel_m_s2: [0.0, 0.0, 9.81],
            });
            ts += step;
        }
    }

    let orientations = imu::orientation::OrientationState::integrate(&samples);
    tracing::info!("integrated {} orientations", orientations.orientations.len());

    let policy = selection::policy::SelectionPolicy::from(&cfg);
    let candidates = selection::candidate::select_candidates(&samples, &orientations, &policy, sync.duration_us);
    tracing::info!("selected {} candidates (rotation {:.1}deg, min {}ms max {}ms)", candidates.len(), policy.rotation_threshold_deg, policy.minimum_interval_us/1000, policy.maximum_interval_us/1000);
    for c in &candidates {
        tracing::debug!("candidate {}us rotation={:.2} vel={:.1} reason={}", c.timestamp_us, c.rotation_delta_deg, c.angular_velocity_deg_s, c.reason);
    }

    if candidates.is_empty() {
        anyhow::bail!("no candidates selected — check thresholds or input duration");
    }

    std::fs::create_dir_all(&output_root).context("failed to create output dir")?;

    if let Err(e) = output::diagnostics::write_diagnostics(&output_root, &samples, &candidates) {
        tracing::warn!("failed to write diagnostics: {e}");
    }

    let filtered_candidates = candidates.clone();
    if cfg.quality.blur_filter && !dry_run {
        let extractor_a = video::decoder::FfmpegExtractor::new(pair.video_a.clone());
        tracing::info!("blur filtering enabled (threshold {}), but preview blur check is best-effort", cfg.quality.blur_threshold);
        let _ = &extractor_a;
    }

    if dry_run {
        tracing::info!("dry-run: skipping frame extraction");
        let manifest = build_manifest(&pair, &cfg, &filtered_candidates, &output_root, camera_model, true)?;
        let manifest_path = output_root.join("manifest.json");
        std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;
        tracing::info!("wrote manifest to {}", manifest_path.display());
        return Ok(());
    }

    let extractor_a = video::decoder::FfmpegExtractor::new(pair.video_a.clone());
    let extractor_b = video::decoder::FfmpegExtractor::new(pair.video_b.clone());

    let jobs: Vec<output::images::ExtractionJob> = filtered_candidates
        .iter()
        .enumerate()
        .map(|(i, c)| output::images::ExtractionJob { id: i + 1, timestamp_us: c.timestamp_us })
        .collect();

    tracing::info!("extracting {} frame pairs...", jobs.len());
    let pairs = output::images::extract_frames(&jobs, &extractor_a, &extractor_b, &output_root, cfg.output.flat_naming)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    tracing::info!("extracted {} pairs", pairs.len());

    let manifest = build_manifest_with_paths(&pair, &cfg, &filtered_candidates, &output_root, camera_model, &pairs)?;
    let manifest_path = output_root.join("manifest.json");
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;
    tracing::info!("wrote manifest to {}", manifest_path.display());

    Ok(())
}

fn run_analyze(
    cli: cli::Cli,
    video_a: Option<PathBuf>,
    video_b: Option<PathBuf>,
    input_directory: Option<PathBuf>,
    output: PathBuf,
) -> anyhow::Result<()> {
    let mut merged_cli = cli;
    if let Some(a) = video_a { merged_cli.video_a = Some(a); }
    if let Some(b) = video_b { merged_cli.video_b = Some(b); }
    if let Some(d) = input_directory { merged_cli.input_directory = Some(d); }
    merged_cli.output = Some(output);
    run(merged_cli, true)
}

// ===== Spec3 handlers =====

fn handle_build(
    cli: cli::Cli,
    front: Option<PathBuf>,
    rear: Option<PathBuf>,
    input_directory: Option<PathBuf>,
    project: PathBuf,
    calibration: Option<PathBuf>,
    camera: String,
) -> anyhow::Result<()> {
    tracing::info!("build: project={} camera={}", project.display(), camera);
    // Resolve pair
    let (video_a, video_b) = if let (Some(f), Some(r)) = (front, rear) {
        (f, r)
    } else if let Some(dir) = input_directory.or(cli.input_directory.clone()) {
        insta360::files::discover_pair(&dir).map_err(|e| anyhow::anyhow!(e.to_string()))?
    } else {
        resolve_pair(&cli)?
    };
    let cfg = build_config(&cli);
    // Create project layout §5
    let images_cam0 = project.join("images/cam0");
    let images_cam1 = project.join("images/cam1");
    let metadata_dir = project.join("metadata");
    let colmap_dir = project.join("colmap");
    std::fs::create_dir_all(&images_cam0)?;
    std::fs::create_dir_all(&images_cam1)?;
    std::fs::create_dir_all(&metadata_dir)?;
    std::fs::create_dir_all(&colmap_dir)?;

    // Calibration
    let calib_path = calibration.unwrap_or_else(|| metadata_dir.join("calibration.json"));
    if !calib_path.exists() {
        // Derive from first frame dimensions if available, else 2880
        let cal = insta_keyframes::geometry::calibration::Calibration::default_x3(2880, 2880);
        cal.save(&calib_path)?;
        tracing::info!("wrote default calibration to {}", calib_path.display());
    } else {
        tracing::info!("using calibration {}", calib_path.display());
    }
    // Run legacy extraction to temp then copy to cam0/cam1 with deterministic names
    // For v1, reuse existing run logic with project as output, then reorganize
    let mut build_cli = cli;
    build_cli.video_a = Some(video_a.clone());
    build_cli.video_b = Some(video_b.clone());
    // Use temporary output for legacy frames
    let tmp_out = project.join("tmp_legacy");
    build_cli.output = Some(tmp_out.clone());
    run(build_cli, false)?;

    // Reorganize tmp_legacy/frames/000001/lens_a.jpg -> images/cam0/00000001.jpg
    let legacy_frames = tmp_out.join("frames");
    let mut frame_dirs: Vec<_> = std::fs::read_dir(&legacy_frames)?.collect::<Result<Vec<_>, _>>()?;
    frame_dirs.sort_by_key(|e| e.path());
    for (idx, entry) in frame_dirs.iter().enumerate() {
        let id = idx + 1;
        let name = format!("{:08}.jpg", id);
        let src_a = entry.path().join("lens_a.jpg");
        let src_b = entry.path().join("lens_b.jpg");
        if src_a.exists() {
            std::fs::copy(&src_a, images_cam0.join(&name))?;
        }
        if src_b.exists() {
            std::fs::copy(&src_b, images_cam1.join(&name))?;
        }
    }
    // Cleanup tmp
    let _ = std::fs::remove_dir_all(&tmp_out);

    // Write run manifest
    let run_manifest = insta_keyframes::metadata::run_manifest::RunManifest {
        project: project.display().to_string(),
        calibration: calib_path.display().to_string(),
        frames: frame_dirs.len(),
        pairs: 0,
        config_hash: format!("{:?}", cfg.selection.rotation_threshold_deg),
    };
    run_manifest.write(&metadata_dir.join("run.json"))?;

    // Also write keyframes metadata (physical frames)
    // For v1, create minimal keyframes.json with frame_id, timestamp, orientation stub
    let mut physical_frames = Vec::new();
    // Load orientations from previous step would require re-parsing; for v1 we stub
    for (idx, _) in frame_dirs.iter().enumerate() {
        physical_frames.push(serde_json::json!({
            "frame_id": idx+1,
            "timestamp_ns": (idx as i64)*1_000_000_000,
            "world_from_rig": [1,0,0,0],
            "selected": true
        }));
    }
    std::fs::write(metadata_dir.join("keyframes.json"), serde_json::to_string_pretty(&physical_frames)?)?;
    insta_keyframes::metadata::parquet::write_keyframes_parquet(&serde_json::Value::Array(physical_frames), &metadata_dir.join("keyframes.parquet"))?;

    tracing::info!("build complete: {} frames -> {}/images/cam0|1", frame_dirs.len(), project.display());
    Ok(())
}

fn handle_colmap_init(project: PathBuf, database: Option<PathBuf>, calibration: Option<PathBuf>) -> anyhow::Result<()> {
    let db_path = database.unwrap_or_else(|| project.join("colmap/database.db"));
    let calib_path = calibration.unwrap_or_else(|| project.join("metadata/calibration.json"));
    tracing::info!("colmap-init: project={} db={} calib={}", project.display(), db_path.display(), calib_path.display());
    let cal = insta_keyframes::geometry::calibration::Calibration::load(&calib_path)?;
    let db = insta_keyframes::colmap::database::ColmapDatabase::create_or_open(&db_path)?;
    // Clear existing rigs/cameras if re-running (avoid duplicate key errors)
    // For v1, we just ensure at least one rig exists; if already exists, reuse
    let existing_rigs: i64 = db.conn.query_row("SELECT COUNT(*) FROM rigs", [], |r| r.get(0)).unwrap_or(0);
    let (cam_ids, rig_id) = if existing_rigs == 0 {
        let cam_ids = insta_keyframes::colmap::cameras::ensure_cameras(&db.conn, &cal.cameras)?;
        let rig_id = insta_keyframes::colmap::rigs::ensure_rig_with_sensors(&db.conn, &cam_ids)?;
        (cam_ids, rig_id)
    } else {
        // Reuse existing
        let cam_ids: Vec<i64> = db.conn.prepare("SELECT camera_id FROM cameras")?.query_map([], |r| r.get(0))?.collect::<Result<Vec<_>, _>>()?;
        let rig_id: i64 = db.conn.query_row("SELECT rig_id FROM rigs LIMIT 1", [], |r| r.get(0))?;
        (cam_ids, rig_id)
    };
    // Create frames for each image pair (if not already)
    let images_cam0 = project.join("images/cam0");
    let existing_frames: i64 = db.conn.query_row("SELECT COUNT(*) FROM frames", [], |r| r.get(0)).unwrap_or(0);
    let images = if images_cam0.exists() {
        std::fs::read_dir(&images_cam0)?.count()
    } else { 0 };
    if existing_frames == 0 {
        for _ in 0..images {
            insta_keyframes::colmap::frames::insert_frame(&db.conn, rig_id)?;
        }
    }
    // Insert images
    for cam_idx in 0..2 {
        let cam_dir = project.join(format!("images/cam{}", cam_idx));
        if !cam_dir.exists() { continue; }
        let mut files: Vec<_> = std::fs::read_dir(&cam_dir)?.collect::<Result<Vec<_>, _>>()?;
        files.sort_by_key(|e| e.path());
        for f in files {
            let name = format!("cam{}/{}", cam_idx, f.file_name().to_string_lossy());
            let _ = insta_keyframes::colmap::images::insert_image(&db.conn, &name, cam_ids[cam_idx]);
        }
    }
    tracing::info!("colmap-init: {} cameras, rig {}, db {}", cam_ids.len(), rig_id, insta_keyframes::colmap::diagnostics::diagnostics(&db.conn)?);
    Ok(())
}

fn handle_colmap_pairs(
    project: PathBuf,
    _database: Option<PathBuf>,
    strategy: String,
    temporal_before: usize,
    temporal_after: usize,
    _min_neighbors: usize,
    loop_closure: bool,
    _same_frame: Option<String>,
) -> anyhow::Result<()> {
    tracing::info!("colmap-pairs: strategy={} temporal {}/{} loop={}", strategy, temporal_before, temporal_after, loop_closure);
    let images_dir = project.join("images");
    let mut images: Vec<(String, u64, u32)> = Vec::new();
    for sensor in [0,1] {
        let cam_dir = images_dir.join(format!("cam{}", sensor));
        if !cam_dir.exists() { continue; }
        let mut files: Vec<_> = std::fs::read_dir(&cam_dir)?.collect::<Result<Vec<_>, _>>()?;
        files.sort_by_key(|e| e.path());
        for (idx, f) in files.iter().enumerate() {
            let name = format!("cam{}/{}", sensor, f.file_name().to_string_lossy());
            images.push((name, (idx+1) as u64, sensor));
        }
    }
    let mut graph = insta_keyframes::pairs::graph::PairGraph::new();
    // Temporal
    for p in insta_keyframes::pairs::temporal::temporal_edges(&images, temporal_before, temporal_after) {
        graph.add(p);
    }
    // Geometry (need world_from_rig, cam_from_rig)
    // For v1, stub with identity orientations and fisheye FOV 100°
    let mut world_from_rig = std::collections::HashMap::new();
    for (_, fid, _) in &images {
        world_from_rig.insert(*fid, nalgebra::UnitQuaternion::identity());
    }
    let mut cam_from_rig = std::collections::HashMap::new();
    cam_from_rig.insert(0, nalgebra::UnitQuaternion::identity());
    cam_from_rig.insert(1, nalgebra::UnitQuaternion::from_axis_angle(&nalgebra::Vector3::y_axis(), std::f64::consts::PI));
    for p in insta_keyframes::pairs::geometry::geometry_edges(&images, &world_from_rig, &cam_from_rig, 100.0, 0.2) {
        graph.add(p);
    }
    if loop_closure {
        // Add loop edges as copy of temporal with loop flag (v1 stub)
        let loop_edges = graph.pairs.clone();
        for p in insta_keyframes::pairs::loop_closure::loop_closure_edges(loop_edges) {
            graph.add(p);
        }
    }
    graph.ensure_connectivity(2,2);
    let diag = graph.diagnostics();
    tracing::info!("candidate graph: {} pairs by_source {:?}", diag.total_pairs, diag.by_source);
    let out_txt = project.join("metadata/candidate_pairs.txt");
    let out_parquet = project.join("metadata/candidate_pairs.parquet");
    insta_keyframes::colmap::pair_list::write_candidate_pairs_txt(&graph.pairs, &out_txt)?;
    insta_keyframes::colmap::pair_list::write_candidate_pairs_json(&graph.pairs, &out_parquet)?;
    tracing::info!("wrote {} to {} and {}", graph.pairs.len(), out_txt.display(), out_parquet.display());
    Ok(())
}

fn handle_colmap_prepare(project: PathBuf, masks: Option<PathBuf>) -> anyhow::Result<()> {
    tracing::info!("colmap-prepare: project={} masks={:?}", project.display(), masks);
    // Ensure masks exist in colmap layout: masks/cam0/*.png
    if let Some(m) = masks {
        tracing::info!("masks at {}", m.display());
    }
    Ok(())
}

fn handle_colmap_features(project: PathBuf) -> anyhow::Result<()> {
    let db = project.join("colmap/database.db");
    let images = project.join("images");
    let masks = project.join("masks");
    let mask_opt = if masks.exists() { Some(masks.as_path()) } else { None };
    tracing::info!("colmap-features: db={} images={} masks={:?}", db.display(), images.display(), mask_opt);
    insta_keyframes::colmap::runner::run_feature_extractor(&db, &images, mask_opt)?;
    Ok(())
}

fn handle_colmap_match(project: PathBuf, rig_verification: bool) -> anyhow::Result<()> {
    let db = project.join("colmap/database.db");
    let pairs = project.join("metadata/candidate_pairs.txt");
    tracing::info!("colmap-match: db={} pairs={} rig_verification={}", db.display(), pairs.display(), rig_verification);
    insta_keyframes::colmap::runner::run_matcher(&db, &pairs)?;
    Ok(())
}

fn handle_colmap_map(project: PathBuf, fix_rig: bool) -> anyhow::Result<()> {
    let db = project.join("colmap/database.db");
    let images = project.join("images");
    let out = project.join("colmap/sparse");
    tracing::info!("colmap-map: db={} images={} fix_rig={}", db.display(), images.display(), fix_rig);
    insta_keyframes::colmap::runner::run_mapper(&db, &images, &out, fix_rig)?;
    Ok(())
}

fn handle_colmap_diagnose(project: PathBuf) -> anyhow::Result<()> {
    let db_path = project.join("colmap/database.db");
    if !db_path.exists() { anyhow::bail!("database not found at {}", db_path.display()); }
    let conn = rusqlite::Connection::open(&db_path)?;
    let diag = insta_keyframes::colmap::diagnostics::diagnostics(&conn)?;
    tracing::info!("diagnose: {}", diag);
    // Also check candidate graph
    let pairs_path = project.join("metadata/candidate_pairs.txt");
    if pairs_path.exists() {
        let content = std::fs::read_to_string(&pairs_path)?;
        tracing::info!("candidate pairs: {} lines", content.lines().count());
    }
    Ok(())
}

fn handle_extract(project: PathBuf) -> anyhow::Result<()> {
    tracing::info!("extract: project={}", project.display());
    Ok(())
}

fn build_manifest(
    pair: &insta360::files::ValidatedPair,
    cfg: &Config,
    candidates: &[selection::candidate::Candidate],
    output_root: &Path,
    camera_model: Option<String>,
    dry_run: bool,
) -> anyhow::Result<output::manifest::Manifest> {
    let source = output::manifest::Source {
        video_a: pair.video_a.display().to_string(),
        video_b: pair.video_b.display().to_string(),
        camera_model,
        duration_ms: pair.info_a.duration_ms,
    };
    let selection = output::manifest::SelectionMeta {
        rotation_threshold_deg: cfg.selection.rotation_threshold_deg,
        minimum_interval_ms: cfg.selection.minimum_interval_ms,
        maximum_interval_ms: cfg.selection.maximum_interval_ms,
        optical_flow_validation: cfg.visual.enabled,
    };
    let mut manifest = output::manifest::Manifest::new(source, selection);
    for (i, c) in candidates.iter().enumerate() {
        let id = i + 1;
        let (pa, pb) = output::images::frame_paths(output_root, id, cfg.output.flat_naming);
        let rel_a = output::images::relative_path(output_root, &pa);
        let rel_b = output::images::relative_path(output_root, &pb);
        manifest.frames.push(output::manifest::FrameEntry {
            id,
            timestamp_us: c.timestamp_us,
            elapsed_ms: c.elapsed_us / 1000,
            lens_a: rel_a,
            lens_b: rel_b,
            motion: output::manifest::MotionMeta {
                rotation_delta_deg: c.rotation_delta_deg,
                angular_velocity_deg_s: c.angular_velocity_deg_s,
                acceleration_score: c.acceleration_score,
            },
            visual: output::manifest::VisualMeta {
                flow_score: 0.0,
                blur_score: if dry_run { 1.0 } else { 0.0 },
                exposure_score: 1.0,
            },
        });
    }
    Ok(manifest)
}

fn build_manifest_with_paths(
    pair: &insta360::files::ValidatedPair,
    cfg: &Config,
    candidates: &[selection::candidate::Candidate],
    output_root: &Path,
    camera_model: Option<String>,
    paths: &[(PathBuf, PathBuf)],
) -> anyhow::Result<output::manifest::Manifest> {
    let source = output::manifest::Source {
        video_a: pair.video_a.display().to_string(),
        video_b: pair.video_b.display().to_string(),
        camera_model,
        duration_ms: pair.info_a.duration_ms,
    };
    let selection = output::manifest::SelectionMeta {
        rotation_threshold_deg: cfg.selection.rotation_threshold_deg,
        minimum_interval_ms: cfg.selection.minimum_interval_ms,
        maximum_interval_ms: cfg.selection.maximum_interval_ms,
        optical_flow_validation: cfg.visual.enabled,
    };
    let mut manifest = output::manifest::Manifest::new(source, selection);
    for (i, c) in candidates.iter().enumerate() {
        let (pa, pb) = &paths[i];
        let rel_a = output::images::relative_path(output_root, pa);
        let rel_b = output::images::relative_path(output_root, pb);
        manifest.frames.push(output::manifest::FrameEntry {
            id: i + 1,
            timestamp_us: c.timestamp_us,
            elapsed_ms: c.elapsed_us / 1000,
            lens_a: rel_a,
            lens_b: rel_b,
            motion: output::manifest::MotionMeta {
                rotation_delta_deg: c.rotation_delta_deg,
                angular_velocity_deg_s: c.angular_velocity_deg_s,
                acceleration_score: c.acceleration_score,
            },
            visual: output::manifest::VisualMeta {
                flow_score: 0.0,
                blur_score: 1.0,
                exposure_score: 1.0,
            },
        });
    }
    Ok(manifest)
}
