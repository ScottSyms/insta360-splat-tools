#!/usr/bin/env python3
"""
Quality gates for the COLMAP -> Gaussian Splat pipeline.

Two checks, meant to run at the two points where a bad run otherwise looks
fine until you actually open the result:

  reconstruction  - after colmap-map, before handing the sparse model to a
                    splat trainer. Checks registered-image coverage and
                    fragmentation (how many disconnected sub-models), and
                    picks out the best one (both trainers always read
                    sparse/0, which is not necessarily the best).

  splat           - after training. Parses the produced .ply directly and
                    checks what fraction of gaussians have non-finite
                    (NaN/Inf) position/opacity/scale/rotation - a trainer
                    that "completes successfully" and writes a normal-sized
                    file can still have written majority-garbage data if
                    optimization diverged.

Usage:
  python3 check_quality.py reconstruction --project room
  python3 check_quality.py reconstruction --project room --min-coverage 0.9 --max-reproj-error 2.0
  python3 check_quality.py splat room/splat.ply
  python3 check_quality.py splat room/splat.ply --max-nan-fraction 0.01

Exit code is non-zero if any check fails, so it can gate a pipeline step.
"""

import argparse
import os
import re
import subprocess
import sys
from pathlib import Path

try:
    import numpy as np
except ImportError:
    print("error: numpy is required (pip3 install numpy / pipx inject ...)", file=sys.stderr)
    sys.exit(2)


def analyze_sparse_model(path: Path) -> dict:
    """Run `colmap model_analyzer` and pull out the fields we care about."""
    out = subprocess.run(
        ["colmap", "model_analyzer", "--path", str(path)],
        capture_output=True, text=True,
    )
    text = out.stdout + out.stderr

    def grab(pattern, cast=int):
        m = re.search(pattern, text)
        return cast(m.group(1)) if m else None

    return {
        "path": path,
        "cameras": grab(r"Cameras:\s*(\d+)"),
        "registered_frames": grab(r"Registered frames:\s*(\d+)"),
        "registered_images": grab(r"Registered images:\s*(\d+)"),
        "points": grab(r"Points:\s*(\d+)"),
        "mean_track_length": grab(r"Mean track length:\s*([\d.]+)", float),
        "mean_reprojection_error": grab(r"Mean reprojection error:\s*([\d.]+)px", float),
    }


def count_total_images(project: Path) -> int:
    total = 0
    for cam_dir in (project / "images").glob("cam*"):
        total += sum(1 for _ in cam_dir.glob("*.jpg"))
    return total


def prepare_splat_input(project: Path, best_sparse_dir: Path, staging_dir: Path) -> None:
    """
    Stage a directory a splat trainer can be pointed at directly, regardless of which
    sub-model actually turned out to be the best one. Both msplat and opensplat always
    read <input>/sparse/0 + <input>/images — neither has a flag to pick a different
    sub-model or an explicit image-source override — so instead of relying on
    sub-model 0 *being* the best one, always build a fresh staging dir whose sparse/0
    points at whichever sub-model actually is best.
    """
    staging_dir.mkdir(parents=True, exist_ok=True)
    images_link = staging_dir / "images"
    # .exists() follows symlinks, so it's False for a stale/broken one left over from an
    # earlier run against a since-removed target — which then makes symlink_to() below
    # fail with FileExistsError (the dirent itself is still there). Check is_symlink() too.
    if images_link.is_symlink() or images_link.exists():
        images_link.unlink()
    images_link.symlink_to(
        os.path.relpath(project / "images", staging_dir), target_is_directory=True
    )
    sparse_dir = staging_dir / "sparse"
    sparse_dir.mkdir(exist_ok=True)
    zero_link = sparse_dir / "0"
    if zero_link.is_symlink() or zero_link.exists():
        zero_link.unlink()
    zero_link.symlink_to(os.path.relpath(best_sparse_dir, sparse_dir), target_is_directory=True)
    print(f"Prepared splat input at {staging_dir} -> sparse/0 -> {best_sparse_dir}")


def check_reconstruction(
    project: Path, min_coverage: float, max_reproj_error: float, prepare_input: Path | None
) -> bool:
    sparse_root = project / "colmap" / "sparse"
    if not sparse_root.exists():
        print(f"FAIL: {sparse_root} not found — has colmap-map run?")
        return False

    submodels = sorted(
        (p for p in sparse_root.iterdir() if p.is_dir()),
        key=lambda p: int(p.name) if p.name.isdigit() else p.name,
    )
    total_images = count_total_images(project)
    print(f"Total images in project: {total_images}")
    print(f"Sub-models found: {len(submodels)}")

    if not submodels:
        print("FAIL: no sparse sub-models found")
        return False

    results = [analyze_sparse_model(sm) for sm in submodels]
    for r in results:
        print(
            f"  sparse/{r['path'].name}: registered={r['registered_images']} "
            f"points={r['points']} track_len={r['mean_track_length']} "
            f"reproj_err={r['mean_reprojection_error']}"
        )

    best = max(results, key=lambda r: r["registered_images"] or 0)
    best_n = best["registered_images"] or 0
    coverage = best_n / total_images if total_images else 0.0
    print(f"\nBest sub-model: sparse/{best['path'].name}  coverage={coverage:.1%} ({best_n}/{total_images})")

    ok = True

    if len(results) > 1:
        discarded = total_images - best_n
        print(
            f"WARNING: {len(results)} disconnected sub-models — the reconstruction is "
            f"fragmented (the mapper couldn't tie all images into one model). Both "
            f"msplat and opensplat only ever read one sub-model, so ~{discarded} images' "
            f"worth of coverage will be silently discarded whichever one you use."
        )

    if coverage < min_coverage:
        print(f"FAIL: best sub-model covers only {coverage:.1%} of images (< {min_coverage:.0%} threshold)")
        ok = False

    if best["mean_reprojection_error"] and best["mean_reprojection_error"] > max_reproj_error:
        print(
            f"FAIL: mean reprojection error {best['mean_reprojection_error']:.2f}px "
            f"exceeds {max_reproj_error}px threshold"
        )
        ok = False

    if best["path"].name != "0":
        print(f"NOTE: best sub-model is sparse/{best['path'].name}, not sparse/0.")

    if prepare_input is not None:
        # Always stage it, pass or fail: even the best-available (possibly still bad)
        # sub-model is what --force should train against, and the caller decides whether
        # a FAIL below should stop the pipeline before that staged input gets used.
        prepare_splat_input(project, best["path"], prepare_input)

    return ok


def load_ply(path: Path):
    with open(path, "rb") as f:
        header = []
        while True:
            line = f.readline()
            if not line:
                raise ValueError(f"{path}: unexpected EOF while reading PLY header")
            header.append(line.decode("ascii", errors="replace").strip())
            if line.strip() == b"end_header":
                break
        props = [l.split()[-1] for l in header if l.startswith("property")]
        vertex_lines = [l for l in header if l.startswith("element vertex")]
        if not vertex_lines:
            raise ValueError(f"{path}: no 'element vertex' in PLY header")
        n = int(vertex_lines[0].split()[-1])
        raw = f.read()
        expected_bytes = n * len(props) * 4
        if len(raw) < expected_bytes:
            raise ValueError(
                f"{path}: truncated — header declares {n} vertices x {len(props)} float "
                f"properties ({expected_bytes} bytes) but only {len(raw)} bytes of data follow"
            )
        data = np.frombuffer(raw[:expected_bytes], dtype=np.float32).reshape(n, len(props))
    return dict(zip(props, data.T)), n


def check_splat(ply_path: Path, max_nan_fraction: float) -> bool:
    d, n = load_ply(ply_path)
    print(f"{ply_path}: {n} gaussians")

    finite_mask = np.ones(n, dtype=bool)
    for v in d.values():
        finite_mask &= np.isfinite(v)
    n_bad = int((~finite_mask).sum())
    frac_bad = n_bad / n if n else 1.0
    print(f"Non-finite gaussians: {n_bad} ({frac_bad:.2%})")

    ok = True
    if frac_bad > max_nan_fraction:
        print(
            f"FAIL: {frac_bad:.2%} of gaussians are NaN/Inf (> {max_nan_fraction:.0%} "
            f"threshold) — training diverged. This usually means the *input* geometry was "
            f"degenerate (e.g. a rectilinear camera model applied to a fisheye lens, or a "
            f"badly fragmented/low-coverage reconstruction), not that the trainer itself is "
            f"low quality — check the reconstruction with `check_quality.py reconstruction` "
            f"before re-training."
        )
        ok = False

    if finite_mask.any():
        xyz = np.stack([d["x"][finite_mask], d["y"][finite_mask], d["z"][finite_mask]], axis=1)
        centroid = xyz.mean(axis=0)
        extent = xyz.max(axis=0) - xyz.min(axis=0)
        print(f"Valid-subset bbox extent: {extent} centroid: {centroid}")
        if "opacity" in d:
            opacity = 1 / (1 + np.exp(-d["opacity"][finite_mask]))
            print(f"Valid-subset opacity: mean={opacity.mean():.3f} frac>0.1={(opacity > 0.1).mean():.2%}")
    else:
        print("FAIL: no finite gaussians at all")
        ok = False

    return ok


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_recon = sub.add_parser("reconstruction", help="Check COLMAP sparse model coverage/fragmentation")
    p_recon.add_argument("--project", required=True, type=Path)
    p_recon.add_argument("--min-coverage", type=float, default=0.9)
    p_recon.add_argument("--max-reproj-error", type=float, default=2.0)
    p_recon.add_argument(
        "--prepare-input", type=Path, default=None,
        help="Stage a directory here with sparse/0 + images pointed at the best sub-model "
             "found, regardless of which sub-model index that actually is — pass this as "
             "--input to a splat trainer instead of the project root.",
    )

    p_splat = sub.add_parser("splat", help="Check a trained .ply for non-finite gaussians")
    p_splat.add_argument("ply_path", type=Path)
    p_splat.add_argument("--max-nan-fraction", type=float, default=0.01)

    args = parser.parse_args()

    if args.cmd == "reconstruction":
        ok = check_reconstruction(args.project, args.min_coverage, args.max_reproj_error, args.prepare_input)
    else:
        ok = check_splat(args.ply_path, args.max_nan_fraction)

    print("\nPASS" if ok else "\nFAIL")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
