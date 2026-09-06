use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "imu-keyframes", version, about = "IMU-guided keyframe extractor for Insta360 video")]
pub struct Cli {
    /// First lens video file (.insv)
    #[arg(long, value_name = "FILE")]
    pub video_a: Option<PathBuf>,

    /// Second lens video file (.insv)
    #[arg(long, value_name = "FILE")]
    pub video_b: Option<PathBuf>,

    /// Input directory containing paired .insv files
    #[arg(long, value_name = "DIR")]
    pub input_directory: Option<PathBuf>,

    /// Output directory (legacy, for project use --project)
    #[arg(long, short = 'o', value_name = "DIR")]
    pub output: Option<PathBuf>,

    /// Rotation threshold in degrees
    #[arg(long, value_name = "DEG")]
    pub rotation_threshold: Option<f64>,

    /// Minimum interval (e.g. 300ms or 300)
    #[arg(long, value_name = "DUR")]
    pub min_interval: Option<String>,

    /// Maximum interval (e.g. 2500ms or 2500)
    #[arg(long, value_name = "DUR")]
    pub max_interval: Option<String>,

    /// Enable optical flow validation
    #[arg(long)]
    pub optical_flow: bool,

    /// Disable blur rejection
    #[arg(long)]
    pub no_reject_blur: bool,

    /// Reject blur (default)
    #[arg(long)]
    pub reject_blur: bool,

    /// Preset name
    #[arg(long, value_name = "PRESET")]
    pub preset: Option<String>,

    /// Output format: jpeg|png
    #[arg(long, value_name = "FMT")]
    pub format: Option<String>,

    /// JPEG quality 1-100
    #[arg(long, value_name = "N")]
    pub jpeg_quality: Option<u8>,

    /// Use flat file naming
    #[arg(long)]
    pub flat_naming: bool,

    /// Verbose (-v, -vv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    /// Analyze telemetry and select timestamps without extracting images
    Analyze {
        #[arg(long, value_name = "FILE")]
        video_a: Option<PathBuf>,
        #[arg(long, value_name = "FILE")]
        video_b: Option<PathBuf>,
        #[arg(long, value_name = "DIR")]
        input_directory: Option<PathBuf>,
        #[arg(long, value_name = "DIR")]
        output: Option<PathBuf>,
    },
    /// Generate HTML preview
    Preview {
        #[arg(long, value_name = "FILE")]
        video_a: Option<PathBuf>,
        #[arg(long, value_name = "FILE")]
        video_b: Option<PathBuf>,
        #[arg(long, value_name = "DIR")]
        input_directory: Option<PathBuf>,
        #[arg(long, value_name = "DIR")]
        output: Option<PathBuf>,
    },
    /// Full build: extract + select + geometry + colmap init (Spec3 §6)
    Build {
        #[arg(long, value_name = "FILE")]
        front: Option<PathBuf>,
        #[arg(long, value_name = "FILE")]
        rear: Option<PathBuf>,
        #[arg(long, value_name = "DIR")]
        input_directory: Option<PathBuf>,
        #[arg(long, value_name = "DIR")]
        project: PathBuf,
        #[arg(long, value_name = "FILE")]
        calibration: Option<PathBuf>,
        #[arg(long, value_name = "STR", default_value = "insta360-x3")]
        camera: String,
    },
    /// Create/configure COLMAP database with rig/frames (§15.1)
    ColmapInit {
        #[arg(long, value_name = "DIR")]
        project: PathBuf,
        #[arg(long, value_name = "FILE")]
        database: Option<PathBuf>,
        #[arg(long, value_name = "FILE")]
        calibration: Option<PathBuf>,
    },
    /// Generate candidate pair graph (§10, §15.3)
    ColmapPairs {
        #[arg(long, value_name = "DIR")]
        project: PathBuf,
        #[arg(long, value_name = "FILE")]
        database: Option<PathBuf>,
        #[arg(long, value_name = "STR", default_value = "imu-geometry")]
        strategy: String,
        #[arg(long, default_value_t = 4)]
        temporal_before: usize,
        #[arg(long, default_value_t = 8)]
        temporal_after: usize,
        #[arg(long, default_value_t = 4)]
        min_neighbors: usize,
        #[arg(long)]
        loop_closure: bool,
        #[arg(long)]
        same_frame: Option<String>,
        /// Max |frame_id delta| considered by the orientation-overlap geometry pass. The
        /// overlap estimate is rotation-only (§3.4: no metric translation), so for a
        /// translating (walking) capture it cannot by itself bound candidate growth — an
        /// unbounded window degenerates to near all-pairs on any capture with limited yaw
        /// variation. Keeps runtime tractable; raise for captures with more looping/revisits.
        #[arg(long, default_value_t = 30)]
        geometry_window: u64,
        /// Minimum estimated_overlap [0,1] for a geometry/cross-lens candidate pair (§10.5)
        #[arg(long, default_value_t = 0.6)]
        geometry_overlap_threshold: f32,
    },
    /// Prepare COLMAP database (colmap-prepare alias)
    ColmapPrepare {
        #[arg(long, value_name = "DIR")]
        project: PathBuf,
        #[arg(long, value_name = "DIR")]
        masks: Option<PathBuf>,
    },
    /// COLMAP feature extraction with masks (§15.2)
    ColmapFeatures {
        #[arg(long, value_name = "DIR")]
        project: PathBuf,
    },
    /// COLMAP matching with candidate pairs (§15.4)
    ColmapMatch {
        #[arg(long, value_name = "DIR")]
        project: PathBuf,
        #[arg(long)]
        rig_verification: bool,
    },
    /// COLMAP mapper (§15.6)
    ColmapMap {
        #[arg(long, value_name = "DIR")]
        project: PathBuf,
        #[arg(long)]
        fix_rig: bool,
    },
    /// Diagnose COLMAP graph health (§20)
    ColmapDiagnose {
        #[arg(long, value_name = "DIR")]
        project: PathBuf,
    },
    /// Legacy extract phase
    Extract {
        #[arg(long, value_name = "DIR")]
        project: PathBuf,
    },
}

impl Cli {
    pub fn effective_video_a(&self) -> Option<&PathBuf> {
        self.video_a.as_ref()
    }
    pub fn effective_video_b(&self) -> Option<&PathBuf> {
        self.video_b.as_ref()
    }
}

pub fn parse_duration_ms(s: &str) -> anyhow::Result<i64> {
    let s = s.trim().to_lowercase();
    if let Some(stripped) = s.strip_suffix("ms") {
        Ok(stripped.parse::<i64>()?)
    } else if let Some(stripped) = s.strip_suffix('s') {
        let v: f64 = stripped.parse()?;
        Ok((v * 1000.0) as i64)
    } else {
        Ok(s.parse::<i64>()?)
    }
}
