# Specification: IMU-Guided Keyframe Extractor for Insta360 Video

## 1. Purpose

Build a Rust command-line program that selects useful keyframes from Insta360 360-degree video using camera IMU telemetry, validates those candidate frames using image-based motion analysis, and extracts synchronized images from both lens video streams.

The primary objective is to reduce the number of frames passed into downstream photogrammetry, Structure-from-Motion (SfM), or Gaussian Splatting pipelines while preserving enough visual overlap and scene coverage for reliable camera-pose reconstruction.

The program is intended to sit before tools such as Spirula Studio, COLMAP, Nerfstudio, or other reconstruction systems.

---

## 2. Core Design Principle

The program should treat **timestamps as the primary keyframe-selection output**.

The pipeline should:

1. Read Insta360 video files.
2. Extract and synchronize IMU telemetry.
3. Estimate camera motion over time.
4. Select candidate timestamps when motion exceeds configurable thresholds.
5. Optionally analyze image change at those timestamps.
6. Reject redundant or visually poor candidates.
7. Extract a synchronized image from each lens video at every accepted timestamp.
8. Write the resulting image pairs and metadata to an output dataset.

The program should not require Gaussian Splatting or SfM software to perform keyframe selection.

---

## 3. Scope

### 3.1 In Scope

- Insta360 cameras that record:
  - two synchronized lens streams;
  - timestamped gyroscope and/or accelerometer telemetry.
- Extraction of IMU data from Insta360 source files.
- Timestamp synchronization between telemetry and lens video streams.
- Motion-based candidate keyframe selection.
- Rotation-aware frame selection.
- Translation/motion estimation where feasible.
- Optional optical-flow or image-difference validation.
- Extraction of one frame from each lens at every selected timestamp.
- Quality filtering.
- Machine-readable metadata describing every selected frame pair.
- Command-line operation.
- macOS support as a primary platform.
- Apple Silicon optimization where practical.
- Extensible support for additional Insta360 camera models.

### 3.2 Out of Scope for Initial Version

- Full SfM reconstruction.
- Bundle adjustment.
- Gaussian Splat training.
- Semantic segmentation beyond optional masking hooks.
- Stitching the two fisheye images into equirectangular panoramas.
- Automatic mesh generation.
- Full SLAM.
- Cloud processing.

These can be added later as downstream or optional stages.

---

## 4. Target Platform

### Primary

- macOS
- Apple Silicon:
  - M1
  - M2
  - M3
  - M4 and later

### Secondary

- Linux
- Windows

The architecture should isolate platform-specific acceleration so the core pipeline remains portable.

---

## 5. Inputs

The program should accept an Insta360 recording consisting of two synchronized video files representing the two camera lenses.

Example conceptual input:

```text
capture/
    lens_a.insv
    lens_b.insv
```

The actual filename convention must not be hard-coded. The program should identify the relationship between source files through metadata where possible or accept explicit file paths.

### CLI Example

```bash
insta-keyframes \
    --video-a VID_001.insv \
    --video-b VID_002.insv \
    --output ./frames
```

Alternative:

```bash
insta-keyframes \
    --input-directory ./capture \
    --output ./frames
```

The program should validate that the supplied streams:

- belong to the same recording;
- have compatible durations;
- have synchronized or reconcilable timestamps;
- contain expected telemetry.

---

## 6. Outputs

For every selected keyframe timestamp, the program should produce:

```text
output/
    manifest.json
    frames/
        000001/
            lens_a.jpg
            lens_b.jpg
        000002/
            lens_a.jpg
            lens_b.jpg
        ...
```

Optional flat naming:

```text
frames/
    000001_a.jpg
    000001_b.jpg
    000002_a.jpg
    000002_b.jpg
```

The manifest should be authoritative.

---

## 7. Manifest Format

Example:

```json
{
  "source": {
    "video_a": "VID_001.insv",
    "video_b": "VID_002.insv",
    "camera_model": "Insta360 X3"
  },
  "selection": {
    "rotation_threshold_deg": 5.0,
    "minimum_interval_ms": 250,
    "maximum_interval_ms": 3000,
    "optical_flow_validation": true
  },
  "frames": [
    {
      "id": 1,
      "timestamp_us": 123456789,
      "elapsed_ms": 4120,
      "lens_a": "frames/000001/lens_a.jpg",
      "lens_b": "frames/000001/lens_b.jpg",
      "motion": {
        "rotation_delta_deg": 5.42,
        "angular_velocity_deg_s": 13.8,
        "acceleration_score": 0.21
      },
      "visual": {
        "flow_score": 0.48,
        "blur_score": 0.92,
        "exposure_score": 0.87
      }
    }
  ]
}
```

The manifest format should be versioned.

Example:

```json
"schema_version": "1.0"
```

---

## 8. Processing Pipeline

```text
Insta360 files
      |
      v
File validation
      |
      v
Telemetry extraction
      |
      v
Time synchronization
      |
      v
IMU normalization
      |
      v
Motion integration
      |
      v
Candidate timestamp selection
      |
      v
Visual validation
      |
      v
Quality filtering
      |
      v
Frame extraction from Lens A + Lens B
      |
      v
Manifest + images
```

---

# 9. Stage 1: File Inspection

The program should inspect each source file and determine:

- codec;
- resolution;
- frame rate;
- duration;
- creation timestamp;
- camera model where available;
- stream identifiers;
- telemetry presence;
- orientation metadata.

The program should attempt to automatically identify which stream corresponds to each lens.

If this cannot be established reliably, explicit CLI assignment should be required.

---

# 10. Stage 2: Telemetry Extraction

The program should extract, at minimum:

- gyroscope X/Y/Z;
- accelerometer X/Y/Z;
- timestamp;
- camera orientation metadata if available.

Preferred implementation:

- use an existing Rust Insta360 telemetry parser where practical;
- isolate telemetry parsing behind a trait.

Example:

```rust
trait TelemetrySource {
    fn samples(&mut self) -> Result<Vec<ImuSample>>;
}
```

Example model:

```rust
struct ImuSample {
    timestamp_us: i64,
    gyro_rad_s: [f64; 3],
    accel_m_s2: [f64; 3],
}
```

The code must not couple motion-selection logic directly to any one camera model or telemetry format.

---

# 11. Stage 3: Timestamp Synchronization

The system must establish a common time domain for:

- IMU samples;
- lens A video;
- lens B video.

The program should support:

1. exact embedded timestamps, where available;
2. stream-relative timestamps;
3. calibrated offsets.

Internal timing should use an integer representation such as microseconds or nanoseconds.

Avoid floating-point timestamps for authoritative frame indexing.

Example:

```rust
type TimestampUs = i64;
```

Synchronization errors should be measured and logged.

Configurable maximum allowable lens skew:

```text
default: 2 ms
```

If the lens streams cannot be synchronized within tolerance, the program should fail or emit an explicit warning depending on configuration.

---

# 12. Stage 4: IMU Preprocessing

Raw IMU data should be cleaned before motion analysis.

Recommended processing:

- unit normalization;
- timestamp ordering;
- duplicate removal;
- invalid sample rejection;
- optional gyro bias correction;
- optional accelerometer bias correction;
- low-pass filtering;
- optional resampling to a uniform rate.

Potential filter choices:

- simple exponential low-pass filter;
- Butterworth filter;
- complementary filter;
- Madgwick orientation filter;
- Mahony filter.

The first implementation should favor simplicity and reproducibility.

---

# 13. Stage 5: Orientation Estimation

Gyroscope samples should be integrated over time to estimate camera orientation changes.

Preferred internal representation:

- quaternion.

Example:

```rust
struct Orientation {
    q: [f64; 4]
}
```

For keyframe selection, absolute world orientation is less important than reliable relative rotation between timestamps.

For each candidate interval calculate:

```text
delta_rotation = angular difference between orientations
```

---

# 14. Stage 6: Motion-Based Candidate Selection

A frame should become a candidate when one or more motion criteria are met.

## 14.1 Rotation Threshold

Example:

```text
rotation_threshold = 5 degrees
```

If accumulated camera rotation since the last accepted keyframe exceeds the threshold:

```text
select candidate
```

This should use quaternion angular distance rather than independent Euler-angle thresholds.

---

## 14.2 Minimum Time Interval

Prevent excessive frame density.

Example:

```text
minimum_interval = 250 ms
```

No candidate may be accepted before this interval unless an exceptional motion threshold is exceeded.

---

## 14.3 Maximum Time Interval

Guarantee coverage even when camera movement is minimal.

Example:

```text
maximum_interval = 3000 ms
```

If no other rule selects a frame within this period, force a candidate.

---

## 14.4 Angular Velocity

Very fast rotations may produce blurred frames.

If angular velocity exceeds a configurable threshold:

```text
delay candidate until angular velocity falls
```

This avoids selecting frames during whip pans.

---

## 14.5 Translation

Pure IMU translation estimation is difficult because accelerometer integration drifts rapidly.

Therefore translation should initially be treated as an approximate signal rather than authoritative displacement.

The program may use:

- acceleration magnitude;
- short-window integration;
- optical flow;
- visual feature displacement.

For indoor walking capture, visual motion should be preferred over double-integrated accelerometer position.

---

# 15. Stage 7: Visual Validation

IMU tells the system that the camera moved.

Visual validation should determine whether the scene changed enough to justify retaining a frame.

This stage should be optional but recommended.

Potential methods, in increasing computational cost:

1. perceptual hash difference;
2. image histogram difference;
3. feature-point displacement;
4. sparse optical flow;
5. dense optical flow.

The initial implementation should use sparse optical flow or feature displacement.

---

# 16. Optical Flow

Optical flow estimates how image features move between two frames.

For keyframe selection, the program does not need dense per-pixel motion.

A sparse method is sufficient.

Suggested workflow:

1. decode a low-resolution preview;
2. detect corners/features;
3. track them into the candidate frame;
4. calculate median displacement;
5. calculate percentage of successfully tracked features;
6. compute a normalized visual-change score.

Candidate acceptance example:

```text
accept if:
    rotation threshold exceeded
OR
    median optical-flow displacement exceeded
OR
    maximum time interval reached
```

A candidate may be rejected as redundant if visual displacement is below a threshold.

---

# 17. Apple Silicon Acceleration

On macOS, optional acceleration should be abstracted behind a visual-analysis backend.

Potential backend design:

```rust
trait VisualMotionBackend {
    fn compare(
        &self,
        previous: &DecodedFrame,
        current: &DecodedFrame
    ) -> Result<VisualMotionScore>;
}
```

Potential implementations:

- portable CPU implementation;
- OpenCV backend;
- macOS Vision backend;
- Metal/Core ML backend.

The macOS implementation should prefer Apple-native APIs where they materially improve performance.

The application must remain functional without GPU acceleration.

---

# 18. Stage 8: Frame Quality Filtering

Selected timestamps should be tested for basic image quality.

Recommended filters:

## 18.1 Blur

Estimate sharpness using:

- Laplacian variance;
- edge energy;
- another lightweight focus metric.

Reject frames below threshold.

If a selected timestamp is blurry, search within a configurable temporal window for a sharper nearby frame.

Example:

```text
search ±100 ms
```

---

## 18.2 Exposure

Reject or penalize frames with:

- severe underexposure;
- severe overexposure;
- clipped highlights over a configured percentage.

---

## 18.3 Motion Blur

Use combined:

- angular velocity;
- image sharpness.

High angular velocity plus low sharpness should strongly penalize a frame.

---

# 19. Stage 9: Optional Masking Hook

The initial application does not need to perform operator or object masking, but its pipeline should support it.

Recommended sequence:

```text
IMU candidate selection
    ->
visual validation
    ->
quality filtering
    ->
final frame extraction
    ->
masking
```

Masking should generally occur only after keyframe selection so expensive segmentation is not run against every video frame.

Potential future masking backends:

- SAM;
- MobileSAM;
- EfficientViT-SAM;
- Core ML person segmentation;
- custom fixed-region mask.

For 360 cameras where the operator or tripod is predictably located, a static or geometry-based mask may be substantially cheaper than semantic segmentation.

---

# 20. Stage 10: Frame Extraction

Once a timestamp is accepted, the program should extract the corresponding frame from both video streams.

Invariant:

```text
one accepted timestamp
    =
one Lens A image
    +
one Lens B image
```

The extraction layer should guarantee that both images represent the same capture instant within configured tolerance.

---

# 21. Video Decode Strategy

Avoid decoding the full-resolution video unnecessarily during candidate selection.

Recommended two-pass strategy:

## Pass 1 — Selection

Use:

- IMU telemetry;
- low-resolution video decode where visual validation is required.

## Pass 2 — Extraction

Seek directly to accepted timestamps and extract full-resolution frames.

This should substantially reduce processing cost.

---

# 22. Image Format

Supported output:

- JPEG;
- PNG.

Default:

```text
JPEG quality 95
```

For photogrammetry or Gaussian Splatting, high-quality JPEG is likely sufficient and considerably smaller than PNG.

Lossless PNG should remain available.

---

# 23. Lens Image Handling

The program should preserve the native lens image representation by default.

Do not automatically:

- stitch;
- rectify;
- convert to equirectangular;
- crop.

Those should be separate optional processing stages.

The output should retain enough metadata to identify:

- lens;
- orientation;
- timestamp;
- camera source.

---

# 24. Optional Fisheye Calibration

Future versions may include:

- camera intrinsics;
- fisheye distortion model;
- per-camera calibration profiles.

The program should permit camera calibration metadata to be attached to the manifest.

Example:

```json
"calibration": {
  "model": "fisheye",
  "lens_a": {
    "fx": 0,
    "fy": 0,
    "cx": 0,
    "cy": 0
  }
}
```

Calibration itself is not required for v1.

---

# 25. Candidate Selection Algorithm

Conceptual pseudocode:

```text
load telemetry
synchronize clocks

last_keyframe = recording_start
accumulated_rotation = 0

for each IMU sample:
    update orientation
    accumulated_rotation =
        angular_distance(
            orientation(last_keyframe),
            orientation(current_time)
        )

    if elapsed < minimum_interval:
        continue

    candidate =
        accumulated_rotation >= rotation_threshold
        OR elapsed >= maximum_interval

    if not candidate:
        continue

    if angular_velocity too high:
        continue

    preview = decode_preview(current_time)

    if visual_validation_enabled:
        score = compare(previous_keyframe_preview, preview)

        if score < visual_threshold
            AND elapsed < maximum_interval:
            continue

    candidate_time =
        choose_best_nearby_frame(current_time)

    accept candidate_time

    last_keyframe = candidate_time
```

---

# 26. Improved Candidate Search

Rather than accepting the exact IMU threshold-crossing timestamp, search a small temporal neighborhood.

Example:

```text
candidate threshold crossed at T

inspect:
T - 100 ms
T - 50 ms
T
T + 50 ms
T + 100 ms
```

Score each candidate using:

```text
score =
    visual novelty
    + sharpness
    + exposure quality
    - motion blur
```

Select the highest scoring timestamp.

This can produce substantially better reconstruction inputs.

---

# 27. Scene Coverage Strategy

The system should optimize for:

```text
maximum useful visual information
per selected frame
```

rather than a fixed output frame rate.

A good keyframe dataset should have:

- substantial overlap with neighboring frames;
- enough baseline for pose estimation;
- minimal near-duplicate imagery;
- limited motion blur;
- broad scene coverage.

---

# 28. Default Configuration Profile

Suggested initial defaults for indoor walking capture:

```toml
[selection]
rotation_threshold_deg = 5.0
minimum_interval_ms = 300
maximum_interval_ms = 2500

[visual]
enabled = true
backend = "auto"
minimum_flow_score = 0.10

[quality]
blur_filter = true
exposure_filter = true
candidate_search_window_ms = 100

[output]
format = "jpeg"
jpeg_quality = 95
```

These values are starting points and must be empirically tuned.

---

# 29. Camera Motion Profiles

Provide presets.

Example:

```bash
insta-keyframes --preset indoor-walk ...
```

Potential presets:

- `indoor-walk`
- `outdoor-walk`
- `vehicle`
- `tripod-pan`
- `slow-survey`
- `dense-reconstruction`

Example behavior:

### indoor-walk

- moderate rotation threshold;
- strong blur rejection;
- moderate visual-overlap requirement.

### slow-survey

- denser frame selection;
- smaller rotation threshold;
- lower minimum interval.

### vehicle

- emphasize visual displacement;
- shorter temporal interval;
- tolerate sustained translational motion.

---

# 30. CLI

Proposed command:

```text
insta-keyframes
```

Example:

```bash
insta-keyframes \
    --video-a VID_A.insv \
    --video-b VID_B.insv \
    --output ./selected \
    --rotation-threshold 5 \
    --min-interval 300ms \
    --max-interval 2500ms \
    --optical-flow \
    --reject-blur
```

---

# 31. Dry-Run Mode

Support:

```bash
insta-keyframes analyze ...
```

This mode should:

- analyze telemetry;
- select timestamps;
- generate metrics;
- not extract full-resolution images.

Outputs:

```text
analysis.json
```

Optional graphable CSV:

```text
timestamp,
rotation_delta,
angular_velocity,
flow_score,
blur_score,
selected
```

This will be valuable for tuning thresholds.

---

# 32. Preview Mode

Optional:

```bash
insta-keyframes preview ...
```

Generate:

- contact sheet;
- HTML timeline;
- thumbnail browser.

The preview should allow a user to visually assess whether keyframe density is appropriate.

---

# 33. Logging

Use structured logging.

Recommended Rust crates:

- `tracing`
- `tracing-subscriber`

Logging levels:

- error
- warn
- info
- debug
- trace

Example:

```text
INFO candidate timestamp=12.400s rotation=5.4deg
INFO rejected timestamp=12.400s reason=motion_blur
INFO accepted timestamp=12.467s flow=0.42 sharpness=0.91
```

---

# 34. Rust Architecture

Suggested crate/module layout:

```text
src/
    main.rs
    cli.rs
    config.rs

    insta360/
        mod.rs
        files.rs
        telemetry.rs
        metadata.rs

    imu/
        mod.rs
        filters.rs
        orientation.rs
        motion.rs

    video/
        mod.rs
        decoder.rs
        frame.rs
        seek.rs

    vision/
        mod.rs
        optical_flow.rs
        blur.rs
        exposure.rs

    selection/
        mod.rs
        candidate.rs
        scoring.rs
        policy.rs

    output/
        mod.rs
        images.rs
        manifest.rs
```

---

# 35. Dependency Philosophy

Prefer:

- Rust-native libraries;
- small dependencies;
- explicit FFI boundaries;
- optional platform acceleration.

Likely external dependencies may include:

- FFmpeg for robust video decoding/seeking;
- an Insta360 telemetry parser;
- image decoding/encoding libraries;
- quaternion/math crate;
- serde;
- clap;
- tracing.

Platform-specific acceleration should be feature-gated.

Example:

```toml
[features]
default = ["ffmpeg"]
macos-vision = []
opencv = []
metal = []
```

---

# 36. Error Handling

Use typed errors.

Recommended:

- `thiserror` for library errors;
- `anyhow` only at application boundaries if desired.

Important failure conditions:

- missing second lens video;
- mismatched recording;
- missing IMU;
- unsupported telemetry format;
- stream synchronization failure;
- video decode failure;
- inability to seek requested timestamp;
- output write failure.

---

# 37. Performance Requirements

Target performance should be measured separately for:

1. telemetry parsing;
2. preview decode;
3. visual validation;
4. full-resolution extraction.

The design goal is to avoid decoding and analyzing every full-resolution video frame.

The system should scale roughly with:

```text
IMU sample count
+
preview frames inspected
+
final keyframes extracted
```

rather than:

```text
all full-resolution video frames
```

---

# 38. Parallelism

Parallelize where safe.

Good candidates:

- final image extraction;
- quality metrics;
- lens A and lens B extraction;
- image encoding;
- candidate visual scoring.

Avoid breaking chronological dependencies in:

- IMU integration;
- orientation estimation;
- accumulated motion state;
- sequential selection policy.

A staged worker model is preferable to indiscriminate parallel iteration.

---

# 39. Reproducibility

Given identical:

- source files;
- configuration;
- software version;

the program should select the same keyframe timestamps.

Any nondeterministic computer-vision operations should use deterministic settings where possible.

---

# 40. Benchmarking

Create benchmark datasets for:

- static room;
- slow walk;
- rapid rotation;
- hallways;
- stairways;
- low light;
- outdoor walking;
- operator visible;
- reflective surfaces.

Compare:

- fixed FPS extraction;
- IMU-only selection;
- IMU + optical flow;
- IMU + optical flow + quality scoring.

Important metrics:

- selected frame count;
- runtime;
- downstream SfM registration rate;
- pose reconstruction failures;
- Gaussian Splat quality;
- total reconstruction time.

---

# 41. Key Success Metric

The primary test is not simply:

```text
How many frames were removed?
```

It is:

```text
Can the downstream reconstruction achieve equivalent or better quality
with substantially fewer input images?
```

For example:

```text
Original:
12,000 frames

Fixed 2 FPS:
1,600 frames

IMU-guided:
450 frames

Result:
same or better camera registration and splat quality
```

That would represent a successful optimization.

---

# 42. Future Enhancements

Potential later features:

- automatic operator masking;
- optical-flow mask propagation;
- AI blur detection;
- scene-change detection;
- adaptive keyframe thresholds;
- machine-learned keyframe scoring;
- SLAM-assisted translation estimation;
- GPS integration;
- magnetometer support;
- direct Spirula project export;
- COLMAP-compatible image and camera metadata export;
- equirectangular export;
- camera calibration;
- scene segmentation;
- spatial coverage heat maps;
- real-time capture feedback.

---

# 43. Adaptive Selection

A future version should support thresholds based on scene complexity.

Examples:

### Feature-rich room

Select somewhat more densely.

### Blank hallway

Use larger intervals unless translation is significant.

### Rapid turn

Delay selection until the camera stabilizes.

### Detailed object inspection

Increase keyframe density.

This requires estimating information gain rather than relying solely on fixed motion thresholds.

---

# 44. Direct Spirula Workflow

The intended workflow should be:

```text
Insta360 recording
        |
        v
insta-keyframes
        |
        +--> selected timestamp list
        |
        +--> Lens A keyframes
        |
        +--> Lens B keyframes
        |
        +--> manifest
        |
        v
optional masking / color correction
        |
        v
Spirula Studio
        |
        v
camera-pose estimation
        |
        v
Gaussian Splat training
```

---

# 45. Recommended MVP

Version 0.1 should implement only:

1. ingest two Insta360 files;
2. extract IMU data;
3. synchronize telemetry and video timestamps;
4. integrate gyro rotation;
5. select timestamps using:
   - minimum interval;
   - maximum interval;
   - rotation threshold;
6. extract synchronized frame pairs;
7. reject obviously blurred frames;
8. produce `manifest.json`;
9. produce CSV diagnostics.

Do not include optical flow in the first milestone unless the implementation is straightforward.

This establishes a complete usable vertical slice.

---

# 46. Version 0.2

Add:

- sparse optical flow;
- visual novelty scoring;
- candidate neighborhood search;
- better blur detection;
- automatic camera-model detection;
- preview/contact-sheet generation.

---

# 47. Version 0.3

Add:

- macOS Vision / Metal acceleration;
- masking hooks;
- adaptive keyframe density;
- Spirula-oriented export profile;
- downstream reconstruction benchmarking.

---

# 48. Summary

The application should be an **IMU-first, timestamp-centric keyframe selection system**.

The IMU should cheaply identify moments where meaningful camera motion has occurred. Image analysis should then determine whether those moments add useful visual information. Only after a timestamp is accepted should the application perform full-resolution extraction from both Insta360 lens streams.

The expected benefit is a substantially smaller, higher-information image dataset for SfM and Gaussian Splatting, reducing:

- feature extraction work;
- feature matching;
- bundle adjustment load;
- image I/O;
- Gaussian training input redundancy;
- total reconstruction time.

The program should remain modular enough that its keyframe-selection engine can later support other 360 cameras and video sources.
