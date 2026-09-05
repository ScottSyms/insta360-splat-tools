# Insta360 Frames — `imu-keyframes` + `scene-mask`

Two Rust CLIs for Insta360 360° capture → reconstruction:

1. **`imu-keyframes`** — IMU-guided keyframe extraction. Timestamps are the primary output: find **when** the camera moved via gyro, validate, and extract synchronized `lens_a`/`lens_b` frames only at those instants. Sits before COLMAP, Nerfstudio, Spirula Studio, Gaussian Splatting.
2. **`scene-mask`** — High-throughput masking for the selected keyframes. Removes people / operator / shadows / known objects via a tiered pipeline (deterministic → Apple Vision → lightweight detector+ROI → promptable fallback) with temporal propagation. Outputs **masks** (preferred for SfM), not destructive edits.

> Specs: [`specification.md`](specification.md) (keyframes) and [`specification2.md`](specification2.md) (masking) — full designs including pipeline, modules, and MVP milestones.

---

## Binaries & Workflow

```
Insta360 .insv pair
      |
      v
imu-keyframes  --input-directory ./samples --output ./frames
      |
      +--> ./frames/manifest.json + frames/000001/lens_a.jpg + lens_b.jpg
      |
      v
scene-mask --input ./frames --output ./cleaned --remove people --remove shadows
      |
      +--> ./cleaned/manifest.json + frames/000001/lens_a.mask.png + lens_b.mask.png
      |
      v
Spirula / COLMAP (respects masks) → Gaussian Splat
```

---

## `imu-keyframes` — Features (v0.1 MVP)

- Paired Insta360 input (`VID_*_00_*.insv` + `VID_*_10_*.insv`, X3 verified, X4/X5 extensible)
- Trailer magic detection (`8db42d694ccc418790edff439fe026bf`), `telemetry-parser` (git master) behind `TelemetrySource` trait
- Quaternion orientation integration (`nalgebra::UnitQuaternion`) and angular-distance candidate selection
- Policy: `rotation_threshold` (deg, quaternion), `minimum_interval` / `maximum_interval`, angular-velocity gating
- Two-pass decode: IMU → timestamps → `ffmpeg` exact seek (`-ss`) + parallel `rayon` extraction
- `manifest.json` (versioned), `analysis.json` + `analysis.csv` diagnostics, presets, `tracing`
- macOS Apple Silicon primary (VideoToolbox via ffmpeg 9), portable core

## `scene-mask` — Features (v0.1 MVP)

- Consumes `imu-keyframes` output (`manifest.json` + `frames/`)
- Tiered pipeline per spec §8: **Tier 0 deterministic** (nadir cap, rect/polygon, precomputed PNG) → **Tier 1 Apple Vision** (person segmentation, `VisionPersonSegmenter` trait) → **Tier 2 lightweight detector+ROI** (stub `DummyDetector`/`DummyRoiSegmenter`) → **Tier 3 promptable** (`DummyPromptSegmenter` fallback)
- Shadow heuristic (§5): expand person mask south, luminance/chroma + spatial connection, `dilate`
- Temporal propagation (§9): reuse previous mask when `rotation_delta ≤ 8°`, otherwise re-segment
- Lens handling (§10): `lens_a`/`lens_b` independent, invariant `1 timestamp = 1 lens_a mask + 1 lens_b mask`
- Fisheye-native (§11), inference downsample (`inference_max_dimension = 768` → upsample), ROI 10–20% padding
- Mask composition (§14) union + keep subtraction, morphology `dilate/erode/close` via `imageproc` (LInf)
- Output `8-bit grayscale` masks (`0=retain, 255=exclude`) + `manifest.json` (§20) with `removed_fraction`
- Presets (§19): `insta360-operator`, `people-only`, `people-and-shadows`, `photogrammetry-clean`, `aggressive-clean`
- Parallelism (§21) per-frame, bounded memory, review HTML

---

## Platform

- **Primary:** macOS Apple Silicon (M1–M4), `ffmpeg 9` via Homebrew, `imageproc` morphology
- **Secondary:** Linux / Windows (feature-gated `ffmpeg`, `opencv`, `macos-vision`)

---

## Installation

```bash
rustc --version  # 1.96+
ffmpeg -version  # 9.x with VideoToolbox
brew install ffmpeg  # macOS

cargo build --release
./target/release/imu-keyframes --help
./target/release/scene-mask --help

# dev
cargo run --bin imu-keyframes -- --help
cargo run --bin scene-mask -- --help
```

Feature gates:

```bash
cargo build --features ffmpeg        # default (ffmpeg + imageproc)
cargo build --features macos-vision  # Apple Vision (macOS, Swift shim stub)
cargo build --no-default-features    # telemetry-only
```

---

## Quick Start

Samples: `./samples/` — `VID_20250731_222121_00_019.insv` holds telemetry, `_10` is video-only (X3 pairing, 2880×2880 24 fps, 203.75 s, 4,890 frames).

### `imu-keyframes`

#### Dry-run — tune without decoding

```bash
cargo run --bin imu-keyframes -- --input-directory ./samples --output /tmp/dry \
  analyze --output /tmp/dry
# /tmp/dry/manifest.json, analysis.json, analysis.csv
```

203 s → 205,376 IMU samples → 414 candidates at `5°/300ms/2500ms` (~2 fps, ~12× reduction).

#### Full extraction

```bash
# auto-discover _00/_10
cargo run --bin imu-keyframes -- --input-directory ./samples --output ./frames

# explicit + tuning
cargo run --bin imu-keyframes -- --video-a ./samples/VID_20250731_222121_00_019.insv \
  --video-b ./samples/VID_20250731_222121_10_019.insv \
  --output ./frames --rotation-threshold 5 --min-interval 300ms --max-interval 2500ms

# sparser (verified: 191 pairs at 10°/500ms/5000ms)
cargo run --bin imu-keyframes -- --input-directory ./samples --output ./sparse \
  --rotation-threshold 10 --min-interval 500ms --max-interval 5000ms

# presets
cargo run --bin imu-keyframes -- --input-directory ./samples --output ./frames --preset indoor-walk
# indoor-walk, outdoor-walk, vehicle, tripod-pan, slow-survey, dense-reconstruction
```

#### Release (as tested)

```bash
cargo run --release -- --input-directory ./samples --output ./frames
# → 414 pairs in ~114 s (release), 3× faster than debug
```

### `scene-mask`

Input is the **output** of `imu-keyframes`:

```bash
# People only (Vision fast path, CPU fallback on non-macOS)
cargo run --bin scene-mask -- --input ./frames --output ./cleaned --remove people

# People + shadows (heuristic fast, --shadow-mode ml is opt-in)
cargo run --bin scene-mask -- --input ./frames --output ./cleaned \
  --remove people --remove shadows

# Operator preset (static nadir + Vision + temporal propagation)
cargo run --bin scene-mask -- --input ./frames --output ./cleaned \
  --preset insta360-operator
# or: --preset people-and-shadows, photogrammetry-clean, aggressive-clean

# Known objects (compact detector → ROI, §5/13)
cargo run --bin scene-mask -- --input ./frames --output ./cleaned \
  --remove people --remove tripod --remove backpack

# Arbitrary prompt (grounding → promptable segmenter fallback, Tier 3)
cargo run --bin scene-mask -- --input ./frames --output ./cleaned \
  --prompt "remove the black tripod"

# Manual masks (Tier 0, §6)
cargo run --bin scene-mask -- --input ./frames --output ./cleaned \
  --rect 0.4,0.7,0.2,0.25 --polygon mask.json --mask existing.png

# Preset + options + review
cargo run --bin scene-mask -- --input ./frames --output ./cleaned \
  --preset photogrammetry-clean --dilate 6 --review
# also: --write-clean-images --clean-mode solid|transparent|blur|inpaint
#       --inference-max-dimension 768 --people-quality fast|balanced|accurate
#       --no-temporal (disable propagation)
```

Test on a 5-frame subset (as used in CI):

```bash
python3 -c "import json; m=json.load(open('./frames/manifest.json')); m['frames']=m['frames'][:5]; json.dump(m, open('/tmp/small_selected/manifest.json','w'), indent=2)"
mkdir -p /tmp/small_selected/frames; for i in 000001 000002 000003 000004 000005; do cp -r ./frames/frames/$i /tmp/small_selected/frames/; done
cargo run --bin scene-mask -- --input /tmp/small_selected --output /tmp/cleaned_small \
  --remove people --static-nadir --dilate 0 --verbose
# → 5 frames, 10 masks, ~40 s debug (decode-bound, 2880² JPEG), 0.2 img/s; release ~3× faster
```

---

## CLI Reference

### `imu-keyframes`

```
imu-keyframes [OPTIONS] --output <DIR> [COMMAND]
  --video-a <FILE>            First lens .insv
  --video-b <FILE>            Second lens .insv
  --input-directory <DIR>     Dir with paired .insv (auto _00/_10)
 -o, --output <DIR>           Output dir
  --rotation-threshold <DEG>  Quaternion angular distance (default 5.0)
  --min-interval <DUR>        Minimum interval e.g. 300ms (default 300ms)
  --max-interval <DUR>        Maximum gap force-select e.g. 2500ms (default 2500ms)
  --optical-flow              Enable visual validation (v0.2)
  --no-reject-blur / --reject-blur
  --preset <PRESET>           indoor-walk | outdoor-walk | vehicle | tripod-pan | slow-survey | dense-reconstruction
  --format <FMT>              jpeg|png (default jpeg)
  --jpeg-quality <N>          1-100 (default 95)
  --flat-naming               frames/000001_a.jpg vs frames/000001/lens_a.jpg
 -v, --verbose

Commands: analyze, preview
```

`DUR` accepts `300`, `300ms`, `2.5s`.

### `scene-mask`

```
scene-mask --input <DIR> --output <DIR> [OPTIONS]
  --input <DIR>                Dir from imu-keyframes (manifest.json)
 -o, --output <DIR>            Output dir for masks + manifest
  --remove <ITEM>              Repeatable: people, operator, shadows, tripod, backpack, chair, vehicle, ...
  --prompt <PROMPT>            Arbitrary text prompt (Tier 3 fallback)
  --rect <x,y,w,h>             Manual rect (pixels or 0..1 normalized), repeatable
  --polygon <FILE>             Polygon JSON [[x,y],...] normalized
  --mask <FILE>                Precomputed PNG mask
  --preset <PRESET>            insta360-operator | people-only | people-and-shadows | photogrammetry-clean | aggressive-clean
  --write-clean-images         Also write lens_a.clean.jpg (masked)
  --clean-mode <MODE>          transparent|solid|blur|inpaint (default solid)
  --shadow-mode <MODE>         fast (heuristic, default) | ml (opt-in neural)
  --people-quality <Q>         fast|balanced|accurate (Vision quality)
  --inference-max-dimension <N> Downsample before segmentation (default 768)
  --dilate <N>                 Final mask dilation radius (default 0, 5-15 for conservative)
  --no-temporal                Disable temporal propagation
  --static-nadir / --no-static-nadir
  --no-colmap                  Disable COLMAP masks/ folder
  --no-colmap-invert           Keep 255=exclude for COLMAP (default inverted 0=masked for COLMAP)
  --colmap-masks-dir <DIR>     COLMAP masks subdir (default "masks")
  --review                     Write review.html
 -v, --verbose
```

Feedback (default `INFO`): one line per frame with timing and `removed_fraction`, plus summary:

```
[1/5] 000001 lens_a 14.0% lens_b 9.7%  10745ms
[2/5] 000002 lens_a 22.5% lens_b 8.5%  10517ms
...
Processed: 5 frames (10 images) in 44.8s  Throughput: 0.2 images/s
COLMAP masks: masks/ (inverted=true) — use: colmap feature_extractor --image_path ./cleaned/frames --ImageReader.masks ./cleaned/masks
```

---

## Outputs

### `imu-keyframes`

```
output/
  manifest.json
  analysis.json
  analysis.csv
  frames/
    000001/lens_a.jpg
    000001/lens_b.jpg
```

Flat: `frames/000001_a.jpg` with `--flat-naming`.

**`manifest.json`** (versioned, authoritative):

```json
{
  "schema_version": "1.0",
  "source": {
    "video_a": "./samples/VID_20250731_222121_00_019.insv",
    "video_b": "./samples/VID_20250731_222121_10_019.insv",
    "camera_model": "Insta360 X3",
    "duration_ms": 203750
  },
  "selection": {
    "rotation_threshold_deg": 5.0,
    "minimum_interval_ms": 300,
    "maximum_interval_ms": 2500,
    "optical_flow_validation": false
  },
  "frames": [{
    "id": 2, "timestamp_us": 300371, "elapsed_ms": 300,
    "lens_a": "frames/000002/lens_a.jpg", "lens_b": "frames/000002/lens_b.jpg",
    "motion": { "rotation_delta_deg": 14.21, "angular_velocity_deg_s": 35.46, "acceleration_score": 1.09 },
    "visual": { "flow_score": 0, "blur_score": 1, "exposure_score": 1 }
  }]
}
```

### `scene-mask`

Default (masks preferred; inpainting invention can harm SfM) — **COLMAP-compatible by default**:

```
cleaned/
  manifest.json
  frames/
    000001/lens_a.jpg
    000001/lens_a.mask.png   # 8-bit grayscale 0=retain 255=exclude (primary)
    000001/lens_b.jpg
    000001/lens_b.mask.png
    000001/lens_a.clean.jpg  # only with --write-clean-images
  masks/
    000001_lens_a.png        # COLMAP-compatible: 0=masked, 255=valid (inverted by default)
    000001_lens_b.png
```

Use directly with COLMAP:

```bash
colmap feature_extractor --database_path database.db \
  --image_path ./cleaned/frames \
  --ImageReader.masks ./cleaned/masks
# --no-colmap to disable masks/ folder
# --no-colmap-invert to keep 255=exclude (if your COLMAP build expects white=masked)
```

Optional: `--write-clean-images --clean-mode transparent|solid|blur|inpaint`.

**`manifest.json`** (§20):

```json
{
  "schema_version": "1.0",
  "source_manifest": "./frames/manifest.json",
  "processing": {
    "people_backend": "apple-vision",
    "people_quality": "fast",
    "shadow_mode": "fast",
    "temporal_propagation": true,
    "preset": "insta360-operator"
  },
  "frames": [{
    "id": 1,
    "lens_a": { "source": "frames/000001/lens_a.jpg", "mask": "frames/000001/lens_a.mask.png", "removed_fraction": 0.023 },
    "lens_b": { "source": "frames/000001/lens_b.jpg", "mask": "frames/000001/lens_b.mask.png", "removed_fraction": 0.023 }
  }]
}
```

`review.html` (with `--review`) shows `original | mask overlay | cleaned` for quick visual check (§23).

---

## Pipeline

### `imu-keyframes`

```
Insta360 files → File validation → Telemetry extraction → Time sync (µs)
→ IMU normalization → Motion integration (quat) → Candidate selection
→ Visual validation* → Quality filtering → Frame extraction → Manifest
```

`*` `HistogramBackend` stub, `SparseFlowBackend` v0.2.

Module map:

```
src/main.rs / cli.rs / config.rs / error.rs
insta360/{files,telemetry,sync,metadata}
imu/{filters,orientation,motion}
video/{decoder,frame,seek}
vision/{optical_flow,blur,exposure}
selection/{candidate,policy,scoring}
output/{images,manifest,diagnostics}
```

Traits: `TelemetrySource::samples()`, `FrameExtractor::extract_frame()`, `VisualMotionBackend::compare()`.

### `scene-mask`

```
manifest → image pair → deterministic masks → person segmentation
→ known-object detection → ROI segmentation → shadow inference
→ temporal propagation/validation → merge (OR, keep subtract) → morphology → masks (+ optional cleaned) → manifest
```

Module map (§17):

```
src/bin/scene-mask.rs / src/lib.rs
image/{decode,encode,resize}
mask/{combine,morphology,feather,static_mask}
people/{vision}
objects/{detector,segmenter,prompt}
shadows/{detect,temporal}
temporal/{propagate,validate}
insta_mask_manifest
```

Traits (§7/15): `PersonSegmenter::segment()`, `Detector::detect()`, `RoiSegmenter::segment_roi()`, `PromptableSegmenter::segment_prompt()`.

Performance tiers (§8): `Tier 0 deterministic (0 ms) → Tier 1 Vision → Tier 2 detector+ROI → Tier 3 promptable`. Inference at `inference_max_dimension` (default 768) then `resize_mask` upsample (§12).

---

## Configuration Defaults

**`imu-keyframes`:**

```toml
[selection]
rotation_threshold_deg = 5.0
minimum_interval_ms = 300
maximum_interval_ms = 2500
angular_velocity_threshold_deg_s = 150.0
[visual]
enabled = false
minimum_flow_score = 0.10
[quality]
blur_filter = true
candidate_search_window_ms = 100
blur_threshold = 80.0
[output]
format = "jpeg"
jpeg_quality = 95
[sync]
max_lens_skew_ms = 50  # X3 observed 41.6 ms
```

**`scene-mask`:**

```
people:       CPU fallback (Vision when macos-vision), quality fast
shadows:      fast heuristic (expand person mask south, darken_threshold 18, dilate 3); --shadow-mode ml is opt-in
temporal:     enabled, max_rotation 8°, max_translation 24 px
inference:    max_dim 768
morphology:   dilate 0 (use 5-15 for conservative reconstruction masking via --dilate)
```

Presets: `insta360-operator` (people+shadows+nadir+temporal), `people-only`, `people-and-shadows`, `photogrammetry-clean`, `aggressive-clean` (dilate 12).

---

## Development

```bash
cargo check --bins
cargo test                         # imu-keyframes: 8 tests (filters, orientation, candidate)
cargo run --bin imu-keyframes -- --input-directory ./samples --output /tmp/dry analyze
cargo run --bin scene-mask -- --input /tmp/small_selected --output /tmp/cleaned --preset people-and-shadows --dilate 0
RUST_LOG=debug cargo run --bin scene-mask -- --input ./frames --output ./cleaned --remove people -vv
```

Logging: `INFO` selection/propagation rate, `DEBUG` per-frame `a_frac`/`propagated`, `WARN` skew/missing.

Metrics logged (§24/§37): `images/sec`, `megapixels/sec`, `full segmentation` vs `propagated`, `peak memory` (future). Example from 5-frame debug run: `0.2–0.3 images/s` debug, `~0.9 images/s` release (decode-bound at 2880²); propagation rate `20%` when rotation > 8°.

---

## Insta360 Notes

- X3: telemetry only in `_00` file; `_10` tail zeros — `has_telemetry()` checks magic. Skew ~41.6 ms is normal; validated against `max_lens_skew_ms = 50`.
- Timebase: `TimestampUs = i64` µs; `telemetry-parser` `t * 1e6`; `ffmpeg -ss <sec>` exact seek.
- No stitching/rectification — fisheye-native processing (§11/§23), masks in native image space.

## Roadmap

- **v0.2 keyframes:** sparse LK flow, ±100 ms neighborhood sharpness, contact-sheet preview
- **v0.2 mask:** optical-flow mask propagation, compact YOLO/Core ML detector, ROI segmentation, `review.html` improvements
- **v0.3 keyframes:** Vision/Metal, masking hooks, adaptive density, Spirula export
- **v0.3 mask:** text grounding, SAM-style promptable segmentation, improved shadows, cross-lens consistency

## Specifications

- `specification.md` — keyframe extraction §§9–48
- `specification2.md` — masking §§1–28 (tiered pipeline, presets, parallelism, memory)

## License

MIT
