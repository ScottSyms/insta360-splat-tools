Specification: High-Speed Object and Person Removal for Selected Insta360 Keyframes

1. Purpose

Build a second Rust binary that consumes the keyframe image pairs produced by the IMU-guided keyframe extractor and creates cleaned image pairs suitable for photogrammetry, Structure-from-Motion, and Gaussian Splatting.

The binary should remove:

people;

the camera operator;

shadows cast by people;

user-specified objects or regions;

recurring unwanted items such as tripods, bags, signs, vehicles, or temporary clutter.

Suggested binary name:

insta-mask

The primary design objective is extreme throughput on Apple Silicon.

2. Design Principle

Do not run a heavyweight segmentation model on every full-resolution image.

Use a tiered pipeline:

deterministic masks where possible;

Apple Vision person segmentation for people;

temporal propagation from one keyframe to the next;

compact object detection plus ROI segmentation for known object classes;

promptable segmentation only as a fallback for arbitrary user-described objects.

The default output should be masks, not destructive image editing. Inpainting may invent features that can interfere with SfM and Gaussian Splat training.

3. Input

Input is the output of the first binary:

selected/
    manifest.json
    frames/
        000001/
            lens_a.jpg
            lens_b.jpg
        000002/
            lens_a.jpg
            lens_b.jpg

The program should preserve synchronized pairing between Lens A and Lens B.

4. Output

Default:

cleaned/
    manifest.json
    frames/
        000001/
            lens_a.jpg
            lens_a.mask.png
            lens_b.jpg
            lens_b.mask.png

Masks should be 8-bit grayscale:

0   = retain
255 = exclude

Optional cleaned images:

--write-clean-images

Modes:

transparent
solid
blur
inpaint

For reconstruction use, separate masks should remain the preferred output.

5. Core Removal Modes

People

insta-mask --remove people

On macOS, use Apple Vision person segmentation as the preferred fast path. Apple Vision provides person-segmentation and person-instance-mask requests, including a quality/performance setting for person segmentation.

Camera Operator

insta-mask --remove operator

Use the cheapest available method in this order:

fixed camera-relative mask;

person segmentation;

temporal propagation.

For Insta360 footage, the operator often occupies a predictable region, so a static or semi-static mask can be much faster than inference.

People Shadows

insta-mask --remove shadows

Use a fast heuristic by default:

start from the person mask;

search nearby darker connected regions;

compare local luminance and chroma;

test whether the dark region is spatially connected to the person;

use temporal movement correlation across keyframes;

dilate and feather the resulting shadow mask.

Optional:

--shadow-mode ml

A neural shadow model should be opt-in, not default.

Known Object Classes

Examples:

--remove tripod
--remove backpack
--remove chair
--remove vehicle

Preferred pipeline:

compact detector
    ->
bounding box
    ->
ROI segmentation

Do not run full-frame segmentation if detection already provides a useful region of interest.

Arbitrary User-Specified Objects

Example:

insta-mask --prompt "remove the black tripod"

Recommended pipeline:

text prompt
    ->
object grounding / detection
    ->
bounding box or point prompt
    ->
promptable segmenter

This should be treated as a fallback because it is more expensive.

6. Manual Removal

Support:

--rect x,y,w,h
--polygon mask.json
--mask existing-mask.png

Future interactive selection should allow:

click;

rectangle;

scribble;

polygon;

one-frame selection followed by propagation.

7. macOS / Apple Silicon Acceleration

Primary target:

macOS;

Apple Silicon;

M1 and later.

Use platform acceleration where practical:

Apple Vision;

Core ML;

Metal;

Accelerate;

Core Image;

VideoToolbox.

Platform-specific functionality should be isolated behind Rust traits.

Example:

trait PersonSegmenter {
    fn segment(&self, image: &DecodedImage) -> Result<Mask>;
}

Suggested implementations:

VisionPersonSegmenter
CoreMlPersonSegmenter
CpuPersonSegmenter

Apple-specific code may live behind a small Swift or Objective-C shim, or use Rust Objective-C bindings.

8. Performance Tiers

Tier 0: Deterministic

Near-zero inference cost:

fixed mask;

nadir mask;

polygon;

box;

static operator region;

luminance threshold;

precomputed mask.

Tier 1: Apple Vision

Use for:

people;

individual people;

foreground instances.

Tier 2: Lightweight ML

Use for:

known object detection;

small segmentation models;

ROI-only inference.

Tier 3: Heavy Promptable Segmentation

Use only when:

the user explicitly requests a particular object;

lower tiers cannot identify it reliably.

9. Temporal Propagation

This is a major optimization.

Instead of segmenting every selected image independently:

segment frame N
    ->
propagate mask to frame N+1
    ->
validate
    ->
reuse if valid

Potential propagation methods:

sparse optical flow;

dense optical flow;

affine transform;

feature transform;

homography where appropriate.

The first binary's motion metadata should be reused.

If viewpoint change is small:

propagate

If viewpoint change is large:

resegment

This should be one of the principal mechanisms for achieving high throughput.

10. Lens Handling

Process Lens A and Lens B independently but preserve their shared timestamp.

Invariant:

one keyframe timestamp
    =
one Lens A image
    +
one Lens B image
    +
one Lens A mask
    +
one Lens B mask

Cross-lens mask transfer may be added later for overlap regions.

11. Fisheye Support

The program must work directly with fisheye images.

Do not require:

stitching;

equirectangular conversion;

rectification.

Segmentation and masking should operate in native image space.

12. Inference Resolution

Do not run expensive models at native camera resolution unless required.

Preferred pattern:

full-resolution image
    ->
downsample
    ->
segment
    ->
upsample mask
    ->
native-resolution edge refinement

Config example:

[performance]
inference_max_dimension = 768

13. ROI Segmentation

If a detector produces:

x, y, width, height

crop the region with 10-20% padding and segment only that crop.

This is a hard performance requirement for generic object removal.

14. Mask Composition

Combine mask sources by union:

final_mask =
    static_mask
    OR person_mask
    OR operator_mask
    OR object_mask
    OR shadow_mask
    OR manual_mask

Support protected regions:

--keep "painting"

Protected masks should subtract from the removal mask.

15. Mask Refinement

Support inexpensive morphology:

dilation;

erosion;

close;

connected-component filtering;

edge feathering.

For reconstruction, slightly conservative masking is preferable.

Example:

expand person mask by 5-15 pixels

This reduces leftover hair, fingers, clothing edges, and shadow fragments.

16. Processing Pipeline

load manifest
    |
    v
load image pair
    |
    v
deterministic masks
    |
    v
person segmentation
    |
    v
known-object detection
    |
    v
ROI segmentation
    |
    v
shadow inference
    |
    v
temporal propagation / validation
    |
    v
merge masks
    |
    v
mask morphology
    |
    +--> save masks
    |
    +--> optional cleaned images
    |
    v
write output manifest

17. Rust Module Layout

src/
    main.rs
    cli.rs
    config.rs
    manifest.rs

    image/
        decode.rs
        encode.rs
        resize.rs

    mask/
        mod.rs
        combine.rs
        morphology.rs
        feather.rs
        static_mask.rs

    people/
        mod.rs
        vision.rs

    objects/
        mod.rs
        detector.rs
        segmenter.rs
        prompt.rs

    shadows/
        mod.rs
        detect.rs
        temporal.rs

    temporal/
        mod.rs
        propagate.rs
        validate.rs

    output/
        mod.rs
        manifest.rs
        masks.rs

18. Suggested CLI

People only:

insta-mask     --input ./selected     --output ./cleaned     --remove people

People and shadows:

insta-mask     --input ./selected     --output ./cleaned     --remove people     --remove shadows

People and known objects:

insta-mask     --input ./selected     --output ./cleaned     --remove people     --remove tripod     --remove backpack

Prompted removal:

insta-mask     --input ./selected     --prompt "remove the red chair"

Operator preset:

insta-mask     --input ./selected     --preset insta360-operator

19. Presets

Provide:

insta360-operator
people-only
people-and-shadows
photogrammetry-clean
aggressive-clean

Example:

[preset.insta360-operator]
remove_people = true
remove_shadows = true
static_nadir_mask = true
temporal_propagation = true

20. Output Manifest

Example:

{
  "schema_version": "1.0",
  "source_manifest": "../selected/manifest.json",
  "processing": {
    "people_backend": "apple-vision",
    "people_quality": "fast",
    "shadow_mode": "fast",
    "temporal_propagation": true
  },
  "frames": [
    {
      "id": 1,
      "lens_a": {
        "source": "../selected/frames/000001/lens_a.jpg",
        "mask": "frames/000001/lens_a.mask.png",
        "removed_fraction": 0.083
      },
      "lens_b": {
        "source": "../selected/frames/000001/lens_b.jpg",
        "mask": "frames/000001/lens_b.mask.png",
        "removed_fraction": 0.014
      }
    }
  ]
}

21. Parallelism

Parallelize:

image decode;

Lens A and Lens B processing;

independent frame pairs;

mask morphology;

image encoding.

Bound concurrency to avoid exhausting unified memory.

Example:

[performance]
workers = "auto"
max_inflight_images = 8

Temporal propagation should preserve chronological state per lens.

22. Memory Strategy

Prefer:

reusable buffers;

pooled masks;

minimal copies;

CVPixelBuffer/IOSurface integration where practical;

avoiding repeated JPEG decode.

Long-term optimized macOS path:

decoded image
    ->
shared pixel buffer
    ->
Vision / Core ML
    ->
mask

23. Review Mode

Provide:

insta-mask review ./cleaned

Generate a local HTML review page showing:

original | mask overlay | cleaned

This avoids needing a full GUI.

24. Performance Metrics

Report:

images/sec
megapixels/sec
decode time
person segmentation time
object detection time
mask propagation time
percentage propagated
percentage fully segmented
peak memory

Example:

Processed: 1,000 lens images
Full segmentation: 126
Propagated masks: 874
Average throughput: 34.2 images/s

The key metric is expensive inference rate, not raw image count.

25. MVP

Version 0.1 should implement:

consume the first binary's manifest;

read selected fisheye image pairs;

Apple Vision person segmentation on macOS;

fixed/polygon masks;

basic person-shadow expansion;

PNG mask output;

output manifest;

parallel image processing;

fast/balanced/accurate presets.

26. Version 0.2

Add:

optical-flow mask propagation;

generic compact object detector;

ROI segmentation;

class-based removal;

review HTML.

27. Version 0.3

Add:

text-prompted object grounding;

promptable segmentation;

improved shadow detection;

Core ML backend selection;

cross-lens consistency;

Spirula/COLMAP export profiles.

28. Recommended Fast Path

For the intended Mac workflow:

People:
    Apple Vision

Operator:
    static mask + Apple Vision

Shadows:
    heuristic + temporal correlation

Known objects:
    compact detector + ROI segmenter

Arbitrary user-described object:
    grounding + promptable segmenter

Adjacent keyframes:
    propagate masks instead of rerunning inference
