# Specification: IMU/Geometry-Assisted COLMAP Pipeline for Insta360 Keyframes and Gaussian Splatting

**Status:** Draft implementation specification  
**Target language:** Rust  
**Existing binaries:** `imu-keyframes`, `scene-mask`  
**Primary integration target:** COLMAP 3.13+ / 4.x database and rig model  
**Primary capture device:** Insta360 X3-class dual-lens 360 camera  

---

## 1. Purpose

This specification defines incremental changes to the existing `imu-keyframes` and `scene-mask` tools so that they can prepare a high-quality, reduced-size image set for COLMAP and Gaussian Splat training.

The central design is:

1. Use Insta360 IMU data to select useful keyframes rather than sampling video at a fixed interval.
2. Preserve the known rigid geometry between the two physical lenses.
3. Use time, IMU orientation, lens geometry, and conservative field-of-view overlap tests to construct a sparse **candidate image-pair graph**.
4. Configure COLMAP's native **rig/frame/camera** structures to represent the dual-lens camera correctly.
5. Manipulate the COLMAP SQLite database directly for cameras, rigs, frames, images, and application-owned metadata/pair planning, while leaving visual feature extraction, descriptor creation, matching, geometric verification, and SfM to COLMAP.
6. Generate masks for people, people shadows, and user-selected objects before COLMAP feature extraction, so excluded regions do not contribute keypoints.
7. Preserve visual loop-closure matching as a complementary mechanism because IMU orientation alone cannot determine global position or detect all revisits.
8. Produce a COLMAP sparse reconstruction suitable for downstream Gaussian Splat training.

The optimization target is not to replace COLMAP. The objective is to reduce unnecessary images and image-pair comparisons while improving the geometric constraints supplied to COLMAP.

---

## 2. High-Level Architecture

```text
Insta360 recording
    |
    +-- front/rear lens video streams
    +-- IMU / gyro samples
    +-- capture metadata
             |
             v
      imu-keyframes
             |
             +-- selected synchronized keyframes
             +-- timestamp + orientation metadata
             +-- calibrated lens/rig geometry
             +-- candidate-pair graph
             +-- COLMAP rig/frame/image configuration
             |
             v
        scene-mask
             |
             +-- COLMAP-compatible masks
             +-- mask diagnostics
             |
             v
        COLMAP database
             |
             +-- rigs
             +-- cameras
             +-- frames
             +-- images
             +-- keypoints            [COLMAP-owned]
             +-- descriptors          [COLMAP-owned]
             +-- matches              [COLMAP-owned]
             +-- two_view_geometries  [COLMAP-owned]
             |
             v
       COLMAP mapper
             |
             v
      sparse reconstruction
             |
             v
      Gaussian Splat trainer
             |
             v
          scene.ply
```

---

## 3. Design Principles

### 3.1 COLMAP remains authoritative for visual geometry

The Rust tools SHALL NOT implement a replacement feature matcher, RANSAC verifier, bundle adjuster, or SfM mapper in the first implementation.

COLMAP SHALL remain responsible for:

- keypoint extraction;
- feature descriptors;
- descriptor matching;
- geometric verification;
- fundamental / essential / homography estimation;
- two-view geometry;
- incremental or global SfM;
- bundle adjustment;
- sparse 3D point generation.

The Rust code SHALL be responsible for reducing and structuring COLMAP's search problem.

### 3.2 Bias pair selection toward recall

A false positive candidate pair costs matching time.

A false negative candidate pair may remove a critical edge from the reconstruction graph and fragment the model.

Candidate pruning SHALL therefore be conservative. Geometry constraints are a method for eliminating clearly implausible pairs, not for proving that every retained pair overlaps.

### 3.3 Use native COLMAP rig support

COLMAP 3.12+ represents multi-camera platforms with `rigs` and `frames`. The two Insta360 lenses SHALL be represented as cameras/sensors in a single rigid rig where the extraction process produces perspective views corresponding to the two physical lenses.

If the downstream representation instead uses virtual perspective views rendered from a stitched 360 panorama, those virtual cameras SHALL be modeled as a calibrated virtual rig with known intrinsics/extrinsics.

### 3.4 Do not treat IMU acceleration as reliable metric translation by default

Gyroscope-based relative orientation is useful and stable over short intervals.

Double-integrated accelerometer position is typically too drift-prone for use as a hard camera-position constraint without a stronger inertial navigation estimator.

The initial implementation SHALL use IMU orientation as a strong prior and accelerometer-derived translation only as an optional heuristic.

---

## 4. Scope

### 4.1 In scope

- extending the existing `imu-keyframes` binary;
- extending the existing `scene-mask` binary;
- synchronized extraction from the two lens video streams;
- IMU/video timestamp synchronization;
- keyframe selection based on motion, blur, overlap, and coverage;
- calibrated rigid dual-lens geometry;
- world-relative camera viewing direction derived from IMU orientation;
- conservative FOV overlap estimation;
- candidate matching graph generation;
- local temporal candidate generation;
- cross-lens candidate generation;
- loop-closure candidate augmentation;
- COLMAP SQLite initialization and validation;
- COLMAP rig/frame/camera/image population;
- use of COLMAP masks;
- invoking COLMAP feature extraction and matching;
- verifying COLMAP output tables and reconstruction health;
- sparse model generation;
- output layout suitable for Gaussian Splat training;
- diagnostics and performance metrics.

### 4.2 Out of scope for v1

- replacing COLMAP's matcher;
- directly predicting feature-to-feature correspondences from IMU data;
- restricting descriptor matching to an IMU-predicted pixel region;
- full visual-inertial odometry;
- full SLAM implementation;
- globally accurate IMU-only position estimation;
- modifying COLMAP source code;
- writing rows into `matches` or `two_view_geometries` from Rust;
- implementing Gaussian Splat optimization itself unless a separate existing trainer wrapper already exists.

---

## 5. Required Output Directory Layout

A project SHALL use a deterministic structure similar to:

```text
project/
  input/
    front.insv
    rear.insv

  images/
    cam0/
      00000001.jpg
      00000002.jpg
      ...
    cam1/
      00000001.jpg
      00000002.jpg
      ...

  masks/
    cam0/
      00000001.jpg.png
      00000002.jpg.png
      ...
    cam1/
      00000001.jpg.png
      00000002.jpg.png
      ...

  metadata/
    keyframes.parquet
    imu.parquet
    calibration.json
    candidate_pairs.parquet
    candidate_pairs.txt
    diagnostics.json

  colmap/
    database.db
    sparse/
      0/
        rigs.bin
        cameras.bin
        frames.bin
        images.bin
        points3D.bin

  splat/
    ... trainer output ...
```

Images captured at the same physical instant SHALL use the same basename in each camera folder. This supports COLMAP's rig/frame model cleanly.

Example:

```text
images/cam0/00000127.jpg
images/cam1/00000127.jpg
```

represent the two sensor measurements in physical frame 127.

---

## 6. `imu-keyframes` Responsibilities

The existing keyframe program SHALL be extended into four logical phases:

```text
extract -> select -> geometry -> colmap
```

These MAY be implemented as subcommands or as stages in a single `build` command.

Recommended CLI:

```bash
imu-keyframes extract ...
imu-keyframes select ...
imu-keyframes geometry ...
imu-keyframes colmap-init ...
imu-keyframes colmap-pairs ...
```

A convenience orchestration command MAY expose:

```bash
imu-keyframes build ...
```

---

## 7. Keyframe Metadata Model

Each physical keyframe SHALL have a unique `frame_id` and each extracted image SHALL have a unique `image_id` within application metadata.

Recommended logical schema:

```rust
struct PhysicalFrame {
    frame_id: u64,
    timestamp_ns: i64,
    source_frame_index: u64,

    // Orientation of rig in chosen world/reference coordinates.
    world_from_rig: Quaternion<f64>,

    // Optional inertial heuristics; never treated as authoritative in v1.
    angular_velocity_rad_s: [f64; 3],
    linear_accel_m_s2: [f64; 3],

    sharpness_score: f32,
    motion_score: f32,
    selected: bool,
}

struct ImageMeasurement {
    app_image_id: u64,
    frame_id: u64,
    sensor_id: u32,
    relative_path: String,
    width: u32,
    height: u32,

    world_from_camera: Quaternion<f64>,
    camera_forward_world: [f64; 3],
}
```

The stored quaternion convention MUST be explicit in metadata. The implementation SHALL never infer quaternion ordering from context.

Required metadata fields:

- quaternion ordering: `wxyz` or `xyzw`;
- handedness;
- world axis convention;
- camera axis convention;
- timestamp timebase;
- IMU-to-rig transformation;
- camera-to-rig transformations.

---

## 8. Camera Calibration and Rig Geometry

### 8.1 Calibration file

A calibration file SHALL describe both physical cameras and the IMU relationship.

Example logical form:

```json
{
  "version": 1,
  "rig": "insta360-x3",
  "coordinate_convention": "documented-project-convention",
  "imu_from_rig": {
    "rotation_wxyz": [1, 0, 0, 0],
    "translation_m": [0, 0, 0]
  },
  "cameras": [
    {
      "sensor_id": 0,
      "name": "cam0",
      "model": "OPENCV_FISHEYE",
      "width": 0,
      "height": 0,
      "params": [],
      "camera_from_rig": {
        "rotation_wxyz": [1, 0, 0, 0],
        "translation_m": [0, 0, 0]
      }
    },
    {
      "sensor_id": 1,
      "name": "cam1",
      "model": "OPENCV_FISHEYE",
      "width": 0,
      "height": 0,
      "params": [],
      "camera_from_rig": {
        "rotation_wxyz": [0, 0, 1, 0],
        "translation_m": [0, 0, 0]
      }
    }
  ]
}
```

The actual camera model and parameters MUST be derived from the extraction/projection method and calibration. The example above is not normative.

### 8.2 Known geometry

The implementation SHALL support known `camera_from_rig` transforms for each lens.

For a given physical frame `i`:

```text
world_from_camera(i,sensor)
    = world_from_rig(i) * rig_from_camera(sensor)
```

or the mathematically equivalent expression according to the project's declared transform direction.

Unit tests SHALL verify the transform convention using known synthetic rotations.

### 8.3 Rig calibration refinement

Known rig geometry SHOULD be treated as fixed initially.

The COLMAP mapper SHOULD be run with rig-relative pose refinement disabled when calibration is trusted:

```text
Mapper.ba_refine_sensor_from_rig = false
```

A later calibration mode MAY allow COLMAP to refine sensor-from-rig transforms.

---

## 9. Keyframe Selection

The existing selector SHALL retain its current logic where useful, but selection SHOULD consider:

- minimum angular displacement from the previous retained keyframe;
- estimated translational motion heuristic if available;
- image sharpness / blur;
- exposure quality;
- time since previous retained keyframe;
- maximum allowed gap;
- scene-change / visual information measure if already implemented;
- coverage of orientation space;
- expected overlap with neighboring retained frames.

The selector SHALL preserve synchronized dual-camera measurements as one physical frame. It SHALL NOT independently retain the front image but drop the rear image for the same timestamp unless explicitly configured.

Recommended policy:

```text
select physical frame -> extract all configured rig-camera measurements
```

---

## 10. Candidate Pair Graph

### 10.1 Definition

Let each extracted camera image be a vertex `v`.

The matching plan is a graph:

```text
G = (V, E)
```

where each edge `(i,j)` means that COLMAP is permitted/requested to visually match images `i` and `j`.

Candidate edges SHALL be generated from the union:

```text
E = E_temporal
  U E_geometry
  U E_cross_lens
  U E_loop_closure
  U E_safety
```

### 10.2 Temporal edges

Each image SHALL be paired with nearby physical frames.

Example configurable parameters:

```text
--temporal-before 4
--temporal-after 8
```

The window SHOULD be expressed in retained physical keyframes rather than original source video frames.

For each sensor, local temporal edges SHOULD include:

```text
cam0/frame100 <-> cam0/frame101
cam0/frame100 <-> cam0/frame102
...
cam1/frame100 <-> cam1/frame101
...
```

Cross-sensor temporal edges MAY also be generated if geometry predicts overlap.

### 10.3 Same-frame policy

The two physical fisheye lenses on an Insta360 camera generally look in approximately opposite directions. Depending on extraction/projection geometry, same-frame images may have little useful overlap.

The system SHALL support:

```text
--same-frame-pairs auto|always|never
```

`auto` SHALL use calibrated geometry to estimate whether meaningful overlap exists.

If cameras are truly non-overlapping, COLMAP's option to skip image pairs within the same frame SHOULD be enabled.

### 10.4 Relative orientation

For images `i` and `j`, derive the camera-to-camera relative rotation from the IMU rig orientations and calibrated sensor transforms.

A normalized angular separation SHALL be computed:

```text
theta_ij = angle(R_i^-1 * R_j)
```

This value MAY participate in pruning and scoring but SHALL NOT by itself determine overlap.

### 10.5 FOV overlap

The pair generator SHALL estimate whether two camera viewing regions plausibly share scene content.

Implementation options, in increasing order of fidelity:

1. compare camera forward vectors and effective FOV cones;
2. sample rays across the calibrated image boundary and compare spherical viewing polygons;
3. project one camera's sampled rays into the other's orientation frame and measure angular intersection.

Version 1 SHOULD implement method 2 or 3 because fisheye views are poorly approximated by a simple narrow pinhole cone.

The output SHALL include an `estimated_overlap` value in `[0,1]`.

The overlap test SHALL be conservative.

### 10.6 Cross-lens temporal geometry

The pair generator SHALL explicitly consider pairs such as:

```text
cam0/frame100 <-> cam1/frame107
```

A rotation of the physical rig may cause a later rear-facing lens to observe a region previously observed by the front-facing lens.

This is a key intended optimization and SHALL be covered by integration tests.

### 10.7 Candidate scoring

Every candidate pair SHOULD be assigned a diagnostic score even if final selection uses hard rules.

Suggested model:

```text
score(i,j) =
      wt * temporal_score
    + wo * overlap_score
    + wr * rotation_score
    + wc * cross_lens_score
    + wl * loop_score
    + ws * safety_score
```

The exact weights SHALL be configurable and versioned.

The pair table SHALL record component scores independently so selection behavior can be diagnosed later.

### 10.8 Hard exclusion rules

Pairs MAY be excluded immediately when all of the following are true:

- outside the configured temporal neighborhood;
- no plausible calibrated FOV intersection;
- not supplied by loop closure;
- not required by a safety connectivity rule.

No hard exclusion SHALL be based solely on accelerometer-derived position in v1.

### 10.9 Graph connectivity safety

After pruning, each physical frame SHALL retain at least a configurable minimum number of candidate links to earlier and later frames where possible.

Recommended defaults:

```text
min_prev_edges = 2
min_next_edges = 2
```

The graph validator SHALL report:

- isolated images;
- isolated physical frames;
- connected components;
- articulation points or fragile cuts where inexpensive to compute;
- images with fewer than the target number of neighbors.

If pruning creates disconnected temporal components, safety edges SHALL be reintroduced before COLMAP matching.

---

## 11. Loop Closure

IMU orientation cannot reliably determine whether the camera has returned to a previously visited location.

Loop closure SHALL therefore remain visually driven.

Recommended initial strategies:

1. use COLMAP sequential matching with loop detection for a subset of pairs; or
2. use a visual retrieval stage to identify top-K nonlocal image candidates and add them to the candidate graph.

Loop candidates SHALL be merged into the application's pair graph and tagged:

```text
reason = LOOP_CLOSURE
```

Loop closure SHALL ignore the local temporal exclusion window so that distant revisits can be discovered.

Recommended output metadata:

```text
image_a
image_b
source = geometry|temporal|loop|safety
score
frame_delta
time_delta_s
angular_delta_deg
estimated_overlap
```

---

## 12. `scene-mask` Changes

### 12.1 Existing behavior

The existing masking implementation SHOULD be retained.

The required change is primarily the output contract and integration timing.

### 12.2 Mask timing

Masks MUST be generated before COLMAP feature extraction if the masked regions are intended to be excluded from feature generation.

Pipeline order:

```text
keyframe extraction
    -> scene masking
    -> COLMAP feature extraction with mask_path
    -> matching
    -> mapping
```

### 12.3 COLMAP mask convention

For image:

```text
images/cam0/00000127.jpg
```

the corresponding COLMAP mask SHALL be:

```text
masks/cam0/00000127.jpg.png
```

The mask SHALL have the same pixel dimensions as the source image.

Pixel convention:

```text
0      black -> excluded from feature extraction
nonzero      -> permitted
```

The implementation SHOULD emit binary `0/255` grayscale PNG masks.

### 12.4 Mask classes

Minimum supported semantic targets:

- person;
- person shadow;
- tripod / monopod / camera operator equipment;
- arbitrary user-specified classes or prompts supported by the current masking model.

CLI example:

```bash
scene-mask \
  --images project/images \
  --output project/masks \
  --remove person \
  --remove "person shadow" \
  --remove tripod \
  --colmap-layout
```

### 12.5 Conservative mask dilation

The tool SHOULD support configurable dilation around detected people and equipment:

```text
--dilate-px N
```

A small dilation helps prevent feature extraction on object boundaries.

Shadow masks SHOULD default to lower-confidence but slightly expanded regions if the existing detector can distinguish shadow confidence.

### 12.6 Mask diagnostics

Per-image diagnostics SHALL include:

- total masked pixels;
- masked percentage;
- detected classes;
- number of components;
- confidence summary;
- whether more than a warning threshold was masked.

Images exceeding a configurable maximum masked fraction SHOULD be flagged as potentially unusable.

---

## 13. COLMAP Database Ownership Model

### 13.1 Principle

The Rust application SHALL manipulate COLMAP's schema only where there is a clear ownership boundary.

Application-owned/configured:

- database creation / schema validation;
- `cameras` records, where calibration is known;
- `rigs` configuration;
- `frames` configuration;
- `images` records and frame associations where supported by the selected COLMAP API/schema;
- correspondence between application frame/image IDs and COLMAP IDs;
- candidate-pair planning outside COLMAP core match tables.

COLMAP-owned:

- `keypoints`;
- `descriptors`;
- `matches`;
- `two_view_geometries`.

The Rust application SHALL NOT populate `matches` merely to represent candidate pairs. An entry in `matches` means actual feature-index correspondences, not a hint that a pair should be matched.

### 13.2 Current COLMAP tables

The implementation SHALL expect current COLMAP databases to contain:

```text
rigs
cameras
frames
images
keypoints
descriptors
matches
two_view_geometries
```

The application SHALL introspect `sqlite_master` and `PRAGMA table_info(...)` at startup and fail with a clear incompatibility error if the installed COLMAP schema does not match the supported schema range.

### 13.3 Database creation

Preferred approach:

```bash
colmap database_creator --database_path project/colmap/database.db
```

The Rust code MAY execute this command automatically.

Direct creation of COLMAP tables from duplicated Rust SQL is NOT preferred because schema changes between COLMAP versions would create unnecessary maintenance risk.

### 13.4 Database access library

Rust SHOULD use `rusqlite` for direct SQLite access.

All mutations SHALL use transactions.

Recommended settings:

```sql
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 5000;
```

WAL mode MAY be used by application-only preparation phases but SHOULD be tested against COLMAP behavior before becoming the default.

### 13.5 Camera rows

Each physical lens / rendered virtual camera configuration sharing intrinsics SHALL reference a single COLMAP `camera_id`.

Camera IDs SHALL be:

- positive;
- nonzero;
- stable within a project;
- mapped explicitly in application metadata.

The application SHOULD prefer COLMAP/pycolmap or a small COLMAP helper interface for encoding camera parameter blobs if camera-model ABI/schema compatibility becomes fragile.

If implemented directly, camera parameters SHALL be written as contiguous row-major `float64` values in the exact order required by the selected COLMAP camera model.

### 13.6 Images

The `images.name` field SHALL be the unique relative image path below `image_path`.

Examples:

```text
cam0/00000127.jpg
cam1/00000127.jpg
```

Application image records SHALL maintain:

```text
app_image_id <-> colmap_image_id
```

The mapping SHALL be persisted in `metadata/keyframes.parquet` or a dedicated metadata table/file.

### 13.7 Rigs and frames

The two lens cameras SHALL be assigned to one COLMAP rig.

Each synchronized timestamp SHALL become a COLMAP frame containing both measurements where both images exist.

Conceptually:

```text
rig 1
  reference camera: cam0
  camera cam1: known camera_from_rig transform

frame 127
  cam0 -> image_id 253
  cam1 -> image_id 254
```

The implementation MUST verify that the exact database representation matches the supported COLMAP version rather than hard-coding undocumented SQL layouts.

Preferred implementation choices, in order:

1. use COLMAP's `rig_configurator` after populating cameras/images;
2. use pycolmap through a small helper if direct rig/frame SQL proves version-sensitive;
3. direct SQL only after schema introspection and version-specific adapters are implemented.

"Level 2 integration" in this specification means the Rust application owns and inspects the SQLite database lifecycle; it does not require bypassing stable COLMAP utilities when those utilities safely create complex rig/frame records.

### 13.8 Keypoints and descriptors

The Rust program SHALL treat these tables as read-only diagnostics after COLMAP feature extraction.

It MAY query:

```text
count(images)
count(keypoints)
keypoints.rows per image
descriptors.rows per image
```

It SHALL NOT generate feature blobs in v1.

### 13.9 Matches

`matches` stores actual pairs of feature indices between two images.

The Rust program MAY read it for diagnostics.

It SHALL NOT write candidate-pair placeholders into it.

The current COLMAP pair identifier uses:

```text
MAX_IMAGE_ID = 2147483647

if image_id1 > image_id2:
    pair_id = MAX_IMAGE_ID * image_id2 + image_id1
else:
    pair_id = MAX_IMAGE_ID * image_id1 + image_id2
```

The program SHOULD implement this encoding for diagnostics and joins, with unit tests using the reverse transformation.

### 13.10 `two_view_geometries`

This table stores geometrically verified matches and associated two-view geometry.

It is the table used by reconstruction.

The Rust application SHALL treat it as COLMAP-owned.

After matching, the application SHOULD inspect it to measure:

- number of verified pairs;
- inlier counts;
- verification ratio by candidate reason;
- which candidate edges failed verification;
- connected components of the verified graph.

This provides the primary feedback loop for tuning geometry pruning.

---

## 14. Candidate Pair Handoff to COLMAP

Direct SQLite manipulation does **not** mean candidate pairs should be represented as fake rows in COLMAP's `matches` table.

The candidate graph SHALL be stored in application metadata and handed to COLMAP through a supported custom-pair matching interface.

Recommended artifact:

```text
metadata/candidate_pairs.txt
```

with one image pair per line:

```text
cam0/00000100.jpg cam0/00000101.jpg
cam0/00000100.jpg cam1/00000107.jpg
cam1/00000100.jpg cam1/00000101.jpg
```

The Rust program SHALL retain the richer scoring data in `candidate_pairs.parquet` and generate the text list as a COLMAP execution artifact.

If a future COLMAP API permits direct programmatic submission of a pair list without an intermediate text file, the adapter MAY use it while preserving the same logical candidate graph.

---

## 15. COLMAP Execution Pipeline

### 15.1 Database initialization

```bash
colmap database_creator \
  --database_path project/colmap/database.db
```

Then:

```bash
imu-keyframes colmap-init \
  --project project \
  --database project/colmap/database.db \
  --calibration project/metadata/calibration.json
```

`colmap-init` SHALL:

1. validate the database schema;
2. populate or validate cameras;
3. populate/validate image records;
4. configure rig and frame associations;
5. persist COLMAP ID mappings;
6. never create feature matches.

### 15.2 Feature extraction with masks

Conceptual command:

```bash
colmap feature_extractor \
  --database_path project/colmap/database.db \
  --image_path project/images \
  --ImageReader.mask_path project/masks
```

Additional camera options SHALL be supplied according to whether cameras already exist in the database and the selected COLMAP version.

Feature extraction SHALL occur after camera/rig initialization when that is required by the chosen integration path.

### 15.3 Pair generation

```bash
imu-keyframes colmap-pairs \
  --project project \
  --database project/colmap/database.db \
  --metadata project/metadata/keyframes.parquet \
  --output project/metadata/candidate_pairs.txt \
  --strategy imu-geometry \
  --temporal-before 4 \
  --temporal-after 8 \
  --min-neighbors 4
```

### 15.4 Feature matching

The COLMAP adapter SHALL invoke the current supported custom-list matcher/importer mechanism for the installed version.

The wrapper SHOULD hide version-specific command naming from the user.

Conceptual wrapper:

```bash
imu-keyframes colmap-match \
  --database project/colmap/database.db \
  --pairs project/metadata/candidate_pairs.txt \
  --rig-verification
```

Internally, the tool MAY execute COLMAP's matching command directly.

Rig-aware geometric verification SHOULD be enabled where supported.

### 15.5 Loop closure augmentation

Two supported operating modes:

```text
mode A: pair graph includes loop candidates before first matching run
mode B: local/geometry match first, then add loop candidates and match incrementally
```

Mode B is preferred for diagnostics because it permits measuring the marginal value of loop closure.

### 15.6 Mapper

```bash
mkdir -p project/colmap/sparse

colmap mapper \
  --database_path project/colmap/database.db \
  --image_path project/images \
  --output_path project/colmap/sparse \
  --Mapper.ba_refine_sensor_from_rig 0
```

If camera intrinsics are trusted and calibrated, the project MAY also disable focal length / extra parameter refinement after testing.

---

## 16. Gaussian Splat Handoff

The pipeline SHALL not assume one specific Gaussian Splat implementation.

Instead it SHALL define a standard handoff:

```text
image root:        project/images
mask root:         project/masks
COLMAP sparse:     project/colmap/sparse/0
```

The splat adapter SHALL verify that the selected trainer supports:

- COLMAP camera models produced by this workflow;
- rig-derived camera poses after sparse reconstruction;
- image masks, if training masks are to be applied independently of COLMAP feature masks.

A trainer-specific adapter MAY undistort or reproject images using COLMAP before training if required.

Example abstract command:

```bash
splat-train \
  --images project/images \
  --masks project/masks \
  --colmap project/colmap/sparse/0 \
  --output project/splat
```

---

## 17. Proposed CLI

### 17.1 Full pipeline

```bash
imu-keyframes build \
  --front input/front.insv \
  --rear input/rear.insv \
  --project project \
  --camera insta360-x3 \
  --calibration x3-calibration.json

scene-mask \
  --images project/images \
  --output project/masks \
  --remove person \
  --remove "person shadow" \
  --remove tripod \
  --colmap-layout

imu-keyframes colmap-prepare \
  --project project \
  --masks project/masks

imu-keyframes colmap-match \
  --project project \
  --strategy imu-geometry+loop

imu-keyframes colmap-map \
  --project project
```

### 17.2 Optional single-command orchestration

A later convenience binary or subcommand MAY expose:

```bash
insta-splat build \
  --input recording.insv \
  --camera insta360-x3 \
  --remove person \
  --remove "person shadow" \
  --remove tripod \
  --output room-scan
```

This SHALL remain orchestration over discrete stages so intermediate data is inspectable and resumable.

---

## 18. Application Metadata Database / Files

COLMAP's database SHALL not be overloaded with arbitrary application-specific columns.

Application-specific state SHOULD live in Parquet or a separate SQLite database.

Recommended files:

```text
metadata/keyframes.parquet
metadata/candidate_pairs.parquet
metadata/run.json
```

If SQLite is preferred:

```text
metadata/pipeline.db
```

Suggested candidate pair schema:

```sql
CREATE TABLE candidate_pairs (
    image_id_a         INTEGER NOT NULL,
    image_id_b         INTEGER NOT NULL,
    colmap_image_id_a  INTEGER,
    colmap_image_id_b  INTEGER,
    source_mask        INTEGER NOT NULL,
    score              REAL NOT NULL,
    temporal_score     REAL,
    overlap_score      REAL,
    rotation_score     REAL,
    loop_score         REAL,
    frame_delta        INTEGER,
    time_delta_s       REAL,
    angular_delta_deg  REAL,
    estimated_overlap  REAL,
    selected           INTEGER NOT NULL,
    PRIMARY KEY(image_id_a, image_id_b)
);
```

`source_mask` MAY encode multiple reasons, e.g. temporal + geometry + loop.

---

## 19. Incremental and Resume Behavior

Every major phase SHALL be resumable.

The system SHALL hash or fingerprint:

- input video identity;
- calibration file;
- keyframe selection configuration;
- masking configuration/model version;
- candidate pair configuration;
- COLMAP version;
- COLMAP feature extraction configuration;
- matching configuration.

A configuration change SHALL invalidate only dependent downstream stages.

Examples:

- changing pair scoring does not require re-extracting images or regenerating masks;
- changing the mask requires feature re-extraction because keypoints were generated under the old mask;
- changing feature type requires clearing/rebuilding keypoints, descriptors, matches, and two-view geometries;
- adding new candidate pairs SHOULD only require matching those previously unverified pairs where the COLMAP version supports incremental matching behavior.

The tool SHALL warn that COLMAP skips pairs that already have two-view geometry in an existing database unless matching state is explicitly cleaned.

---

## 20. Diagnostics

### 20.1 Keyframe diagnostics

Report:

- input frame count;
- retained physical keyframes;
- reduction ratio;
- angular displacement distribution;
- frame interval distribution;
- rejected blur count;
- maximum temporal gap.

### 20.2 Candidate graph diagnostics

Report:

- potential exhaustive pair count `N*(N-1)/2`;
- selected candidate pair count;
- reduction ratio;
- candidate count by source;
- mean/median neighbors per image;
- maximum neighbors;
- connected component count;
- number of safety edges reintroduced;
- same-sensor vs cross-sensor pair counts.

### 20.3 COLMAP feature diagnostics

Report:

- images with zero keypoints;
- keypoints per image;
- masked area vs keypoint count;
- descriptor rows per image;
- feature extraction time.

### 20.4 Match diagnostics

After matching:

- raw matched pair count;
- verified pair count;
- verification success rate;
- median inliers per verified pair;
- success rate by candidate source;
- success rate by angular separation bucket;
- success rate by estimated overlap bucket;
- verified graph connected components.

A particularly useful metric SHALL be:

```text
useful_pair_precision = verified_candidate_pairs / attempted_candidate_pairs
```

but tuning SHALL also monitor reconstruction completeness to ensure precision improvements are not caused by over-pruning.

### 20.5 Reconstruction diagnostics

Report:

- registered images / total images;
- registered physical frames;
- sparse point count;
- observations per image;
- mean track length;
- reprojection error;
- number of disconnected sparse models;
- bundle adjustment time;
- total pipeline time.

---

## 21. Performance Targets

Performance targets SHALL be expressed relative to a baseline, because absolute times depend heavily on hardware and image resolution.

For a representative walking capture:

1. keyframe selection SHOULD reduce source video frames by at least an order of magnitude relative to extracting every frame;
2. geometry-assisted candidate generation SHOULD reduce attempted image pairs substantially relative to exhaustive matching;
3. target pair count SHOULD scale approximately `O(N*k)` for local geometry edges rather than `O(N^2)`, excluding visual loop closure;
4. reconstruction registration rate SHOULD remain within 2% of the best baseline on accepted benchmark scenes;
5. no benchmark may pass solely because matching is faster if the sparse reconstruction fragments materially.

Recommended benchmark comparison:

```text
A. COLMAP exhaustive matching
B. COLMAP sequential matching
C. temporal-only custom pairs
D. temporal + IMU orientation
E. temporal + IMU + calibrated camera geometry
F. temporal + IMU + geometry + loop closure
```

Use identical images, masks, features, and mapper settings for B-F where possible.

---

## 22. Validation and Safety Rules

The pipeline SHALL fail early on:

- mismatched front/rear timestamps beyond configured tolerance;
- missing calibration for geometry-constrained mode;
- invalid or non-normalized quaternions;
- impossible camera calibration parameters;
- duplicate image paths;
- nonpositive COLMAP IDs;
- unsupported COLMAP database schema;
- image/mask dimension mismatch;
- missing mask where `--require-masks` is enabled;
- candidate graph with isolated frames after safety repair;
- COLMAP feature extraction with missing keypoint/descriptor rows;
- zero verified pair graph.

The pipeline SHALL warn, but MAY continue, on:

- one missing camera measurement in an otherwise valid frame;
- very high masked image fraction;
- large IMU timestamp interpolation gap;
- large angular jump between adjacent retained keyframes;
- low verified-pair rate;
- multiple reconstruction models.

---

## 23. Testing Strategy

### 23.1 Unit tests

Required unit test groups:

#### Quaternion/transform tests

- identity transformations;
- 90-degree rotations around each axis;
- composition order;
- inverse transforms;
- camera-from-rig vs rig-from-camera convention.

#### Pair ID tests

- COLMAP image-pair ID encoding;
- reverse decoding;
- order independence;
- maximum supported image ID boundary.

#### FOV overlap tests

Synthetic cameras:

- identical orientation -> high overlap;
- opposite narrow cameras -> zero overlap;
- partially intersecting cameras -> partial overlap;
- cam0 at time A vs cam1 after 180-degree rig rotation -> high predicted overlap.

#### Graph safety tests

- no isolated nodes after repair;
- temporal chain remains connected;
- loop edge joins otherwise separated revisit sections.

### 23.2 Database integration tests

For each supported COLMAP major/minor schema:

1. create DB with COLMAP;
2. run `colmap-init`;
3. verify cameras;
4. verify images;
5. verify rig/frame relationships;
6. run COLMAP feature extraction on a tiny fixture dataset;
7. ensure Rust did not write `matches` or `two_view_geometries` before matching;
8. run matching;
9. verify those tables are now populated by COLMAP;
10. run mapper and verify at least one model is produced.

### 23.3 Mask integration test

Create an image containing a high-feature synthetic region covered by a black mask.

After feature extraction, verify that no keypoints fall inside the black masked region.

### 23.4 End-to-end benchmark scenes

Minimum benchmark suite:

- small room with one loop;
- hallway out-and-back;
- multi-room interior;
- scene with a moving person;
- scene with substantial camera rotation;
- scene where front-to-rear cross-lens temporal matches are valuable.

---

## 24. Implementation Phases

### Phase 1 — Metadata and layout

- normalize synchronized physical-frame representation;
- create deterministic `cam0/` and `cam1/` image layout;
- persist IMU orientation per physical frame;
- add calibration loader and transform tests.

### Phase 2 — COLMAP-aware masking

- output `<image-name>.png` masks;
- ensure black means excluded;
- add mask validation;
- integrate `ImageReader.mask_path` into the wrapper.

### Phase 3 — Geometry pair generator

- temporal candidates;
- world camera orientations;
- FOV overlap estimation;
- cross-lens candidates;
- pair scoring;
- graph connectivity repair;
- pair diagnostics.

### Phase 4 — COLMAP DB Level-2 integration

- COLMAP-created DB lifecycle;
- schema introspection;
- camera/image ID mapping;
- rig/frame configuration;
- transaction handling;
- diagnostic reads of feature and match tables.

### Phase 5 — Matching adapter

- write custom pair-list execution artifact;
- run COLMAP matcher;
- enable rig verification where supported;
- read verification results;
- incremental loop closure pass.

### Phase 6 — Mapping and splat handoff

- mapper wrapper;
- fixed rig calibration option;
- reconstruction validation;
- trainer adapter interface.

### Phase 7 — Benchmark and tune

- compare exhaustive/sequential/custom strategies;
- tune pair thresholds based on verified matches and registration rate;
- establish default profiles for indoor walking scans.

---

## 25. Recommended Rust Module Structure

```text
imu-keyframes/
  src/
    capture/
      insta360.rs
      timestamps.rs
      imu.rs

    keyframes/
      selector.rs
      sharpness.rs
      motion.rs

    geometry/
      calibration.rs
      transforms.rs
      fov.rs
      overlap.rs

    pairs/
      temporal.rs
      geometry.rs
      loop_closure.rs
      graph.rs
      score.rs

    colmap/
      version.rs
      schema.rs
      database.rs
      cameras.rs
      rigs.rs
      frames.rs
      images.rs
      pair_list.rs
      runner.rs
      diagnostics.rs

    metadata/
      parquet.rs
      run_manifest.rs

    cli/
      ...
```

`scene-mask` SHOULD expose a small output adapter module rather than coupling model inference logic to COLMAP:

```text
scene-mask/
  src/
    ... existing inference modules ...
    output/
      colmap_mask.rs
      diagnostics.rs
```

---

## 26. Example End-to-End Workflow

```bash
# 1. Extract synchronized keyframes and IMU metadata.
imu-keyframes build \
  --front ./recording/front.insv \
  --rear ./recording/rear.insv \
  --project ./room \
  --calibration ./calibration/insta360-x3.json

# 2. Generate semantic masks before COLMAP feature extraction.
scene-mask \
  --images ./room/images \
  --output ./room/masks \
  --remove person \
  --remove "person shadow" \
  --remove tripod \
  --colmap-layout

# 3. Create/configure COLMAP DB, cameras, images, rig and frames.
imu-keyframes colmap-prepare \
  --project ./room \
  --database ./room/colmap/database.db \
  --masks ./room/masks

# 4. Extract COLMAP features with semantic masks applied.
imu-keyframes colmap-features \
  --project ./room

# 5. Build geometry-constrained candidate graph.
imu-keyframes colmap-pairs \
  --project ./room \
  --strategy imu-geometry \
  --temporal-before 4 \
  --temporal-after 8 \
  --min-neighbors 4 \
  --loop-closure

# 6. Ask COLMAP to match only planned pairs and perform geometric verification.
imu-keyframes colmap-match \
  --project ./room \
  --rig-verification

# 7. Inspect match graph health before mapping.
imu-keyframes colmap-diagnose \
  --project ./room

# 8. Run COLMAP SfM while preserving calibrated rig geometry.
imu-keyframes colmap-map \
  --project ./room \
  --fix-rig

# 9. Train with a selected Gaussian Splat backend.
insta-splat train \
  --images ./room/images \
  --masks ./room/masks \
  --colmap ./room/colmap/sparse/0 \
  --output ./room/splat
```

---

## 27. Acceptance Criteria

The implementation is complete when all of the following are true:

1. Existing basic keyframe selection still functions.
2. Keyframes from both lenses are synchronized into explicit physical frames.
3. Each retained frame has an IMU-derived orientation.
4. Known dual-lens camera geometry is represented explicitly.
5. The software predicts cross-lens temporal overlap, not only same-lens adjacency.
6. Candidate pair count is materially lower than exhaustive matching on benchmark captures.
7. Candidate pruning does not create unexplained graph fragmentation.
8. `scene-mask` creates masks in COLMAP's required path/filename convention.
9. COLMAP extracts no features from black mask regions.
10. A COLMAP database created by the pipeline contains valid cameras, images, rigs, and frames.
11. Rust does not fabricate feature correspondences in `matches` or verified geometry in `two_view_geometries`.
12. COLMAP itself populates keypoints, descriptors, matches, and verified two-view geometry.
13. Rig-aware geometric verification is enabled when supported and appropriate.
14. The mapper produces a sparse reconstruction with the configured rig constraints.
15. The pipeline reports candidate-pair and verified-pair diagnostics sufficient to tune pruning thresholds.
16. The resulting sparse reconstruction can be consumed by at least one Gaussian Splat training backend.
17. Benchmark results compare the custom geometry strategy against COLMAP sequential and exhaustive baselines.

---

## 28. Key Architectural Decision

The most important boundary in this design is:

```text
Rust chooses the view graph.
COLMAP establishes visual correspondences and reconstructs geometry.
```

The IMU and rigid camera geometry are therefore used to constrain **which image pairs COLMAP attempts**, while COLMAP remains the authority on whether those images actually match.

This keeps the implementation small enough to maintain, preserves COLMAP's mature visual geometry stack, and provides a measurable optimization surface: candidate pair reduction, verified-pair yield, reconstruction registration rate, and total runtime.

---

## 29. COLMAP Compatibility Notes

This specification targets modern COLMAP releases with native rig/frame support.

Implementation SHALL verify the installed version at runtime because CLI names, matching options, and database details may evolve.

Current COLMAP documentation establishes the following behaviors that this design relies on:

- the SQLite database includes `rigs`, `cameras`, `frames`, `images`, `keypoints`, `descriptors`, `matches`, and `two_view_geometries`;
- cameras can be shared by many images;
- rigs define fixed sensor relationships and frames define synchronized rig instances;
- `matches` contains raw feature correspondences;
- `two_view_geometries` contains geometrically verified matches and is what reconstruction consumes;
- masks supplied during feature extraction prevent keypoints from being extracted in black regions;
- modern COLMAP supports rig-constrained geometric verification;
- COLMAP supports fixed rig sensor transforms during bundle adjustment.

The implementation SHOULD maintain a small `ColmapAdapter` abstraction so version-specific behavior is isolated from the keyframe and geometry code.

