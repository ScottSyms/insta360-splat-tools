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

    // tracing
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

    // Handle subcommands — take ownership of command to avoid borrow issues
    let command = cli.command.clone();
    if let Some(cmd) = command {
        match cmd {
            cli::Commands::Analyze { video_a, video_b, input_directory, output } => {
                let out = output.unwrap_or(cli.output.clone());
                return run_analyze(cli, video_a, video_b, input_directory, out);
            }
            cli::Commands::Preview { video_a, video_b, input_directory, output } => {
                let out = output.unwrap_or(cli.output.clone());
                return run_analyze(cli, video_a, video_b, input_directory, out);
            }
        }
    }

    run(cli, false)
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
    // Try to infer from subcommand already handled; fallback error
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
    let output_root = &cli.output;

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

    // Sync
    let sync = insta360::sync::synchronize(&pair, cfg.sync.max_lens_skew_ms)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    // Telemetry extraction
    let mut samples: Vec<ImuSample> = Vec::new();
    let mut camera_model = pair.camera_model.clone();

    if let Some(tel_path) = &pair.telemetry_file {
        tracing::info!("extracting telemetry from {}", tel_path.display());
        let mut source = insta360::telemetry::Insta360TelemetrySource::new(tel_path);
        match source.samples() {
            Ok(mut s) => {
                camera_model = source.camera_model().or(camera_model);
                tracing::info!("telemetry: {} samples, model {:?}", s.len(), camera_model);
                // Preprocess
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

    // Fallback if no IMU: synthesize samples at 100Hz for time-based selection
    if samples.is_empty() {
        tracing::info!("synthesizing IMU samples for fallback selection (no gyro)");
        let dur_us = if sync.duration_us > 0 { sync.duration_us } else { 30_000_000 };
        let step = 10_000; // 100Hz
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

    // Orientation integration
    let orientations = imu::orientation::OrientationState::integrate(&samples);
    tracing::info!("integrated {} orientations", orientations.orientations.len());

    // Candidate selection
    let policy = selection::policy::SelectionPolicy::from(&cfg);
    let candidates = selection::candidate::select_candidates(&samples, &orientations, &policy, sync.duration_us);
    tracing::info!("selected {} candidates (rotation {:.1}deg, min {}ms max {}ms)", candidates.len(), policy.rotation_threshold_deg, policy.minimum_interval_us/1000, policy.maximum_interval_us/1000);
    for c in &candidates {
        tracing::debug!("candidate {}us rotation={:.2} vel={:.1} reason={}", c.timestamp_us, c.rotation_delta_deg, c.angular_velocity_deg_s, c.reason);
    }

    if candidates.is_empty() {
        anyhow::bail!("no candidates selected — check thresholds or input duration");
    }

    std::fs::create_dir_all(output_root).context("failed to create output dir")?;

    // Diagnostics (always write)
    if let Err(e) = output::diagnostics::write_diagnostics(output_root, &samples, &candidates) {
        tracing::warn!("failed to write diagnostics: {e}");
    }

    // Quality filtering (blur) — optional preview decode
    // For MVP, we apply simple blur search if enabled and not dry-run
    // But decoding previews for each candidate would be expensive; do it lazily if needed
    // For now, assume all candidates pass quality unless we decode.

    let mut filtered_candidates = candidates.clone();
    if cfg.quality.blur_filter && !dry_run {
        // Attempt to compute blur scores via preview decode; skip on failure
        let extractor_a = video::decoder::FfmpegExtractor::new(pair.video_a.clone());
        // We do lightweight preview scoring to maybe reject; but for now just log
        tracing::info!("blur filtering enabled (threshold {}), but preview blur check is best-effort", cfg.quality.blur_threshold);
        // Could implement per-candidate preview here if desired; MVP keeps all
        let _ = &extractor_a;
    }

    // If dry-run, write manifest with empty visual scores and skip extraction
    if dry_run {
        tracing::info!("dry-run: skipping frame extraction");
        let manifest = build_manifest(&pair, &cfg, &filtered_candidates, output_root, camera_model, true)?;
        let manifest_path = output_root.join("manifest.json");
        std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;
        tracing::info!("wrote manifest to {}", manifest_path.display());
        return Ok(());
    }

    // Frame extraction
    let extractor_a = video::decoder::FfmpegExtractor::new(pair.video_a.clone());
    let extractor_b = video::decoder::FfmpegExtractor::new(pair.video_b.clone());

    // Prepare jobs
    let jobs: Vec<output::images::ExtractionJob> = filtered_candidates
        .iter()
        .enumerate()
        .map(|(i, c)| output::images::ExtractionJob { id: i + 1, timestamp_us: c.timestamp_us })
        .collect();

    tracing::info!("extracting {} frame pairs...", jobs.len());
    let pairs = output::images::extract_frames(&jobs, &extractor_a, &extractor_b, output_root, cfg.output.flat_naming)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    tracing::info!("extracted {} pairs", pairs.len());

    // Manifest with actual paths
    let manifest = build_manifest_with_paths(&pair, &cfg, &filtered_candidates, output_root, camera_model, &pairs)?;
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
    // Merge explicit analyze args over cli
    let mut merged_cli = cli;
    if let Some(a) = video_a { merged_cli.video_a = Some(a); }
    if let Some(b) = video_b { merged_cli.video_b = Some(b); }
    if let Some(d) = input_directory { merged_cli.input_directory = Some(d); }
    merged_cli.output = output;
    run(merged_cli, true)
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
