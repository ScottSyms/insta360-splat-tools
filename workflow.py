#!/usr/bin/env python3
"""
Full workflow runner: keyframe extraction → masking → matching pair selection → COLMAP → Gaussian Splat
Times each step, echoes command, totals time. Batch by default, --interactive to pause.

Usage:
  python3 workflow.py
  python3 workflow.py --project ./room --samples ./samples
  python3 workflow.py --skip-colmap  # skip colmap stages if colmap not installed
  python3 workflow.py --skip-splat   # skip opensplat training
"""

import argparse
import subprocess
import sys
import os
import time
import shlex
from pathlib import Path

def run_step(cmd, cwd=None):
    """Echo, time, and execute a command. Pause afterwards."""
    cmd_str = " ".join(shlex.quote(c) for c in cmd)
    print("\n" + "="*78)
    print(f"$ {cmd_str}")
    print("="*78)
    start = time.time()
    result = subprocess.run(cmd, cwd=cwd, stdin=subprocess.DEVNULL)
    elapsed = time.time() - start
    print(f"\n→ Step finished in {elapsed:.1f}s  (exit {result.returncode})")
    if result.returncode != 0:
        print(f"  ! Command failed with code {result.returncode}, continuing anyway...")
    return elapsed, result.returncode

def pause(msg="Press Enter to continue to next step (Ctrl-C to abort)..."):
    # Robust pause: handles \n, \r (^M), raw mode, and non-tty
    if not sys.stdin.isatty():
        print(f"\n{msg} (non-interactive, continuing automatically)")
        return
    try:
        sys.stdout.write(f"\n{msg} ")
        sys.stdout.flush()
        # Use os.read for raw handling of \r (^M) vs \n - works in both cooked and raw modes
        while True:
            try:
                data = os.read(sys.stdin.fileno(), 1)
            except OSError:
                break
            if not data or data in (b"\n", b"\r"):
                # Handle CRLF: if we got \r, peek for following \n and consume
                if data == b"\r":
                    try:
                        import select
                        if select.select([sys.stdin], [], [], 0.0)[0]:
                            nxt = os.read(sys.stdin.fileno(), 1)
                            # if nxt is \n, consumed; otherwise ignore (already read)
                            pass
                    except:
                        pass
                break
        sys.stdout.write("\n")
        sys.stdout.flush()
    except KeyboardInterrupt:
        print("\nAborted by user.")
        sys.exit(1)
    except EOFError:
        pass
    except Exception:
        try:
            input()
        except:
            pass

def main():
    parser = argparse.ArgumentParser(description="Batch workflow for imu-keyframes + scene-mask + opensplat (COLMAP)")
    parser.add_argument("--project", default="./room", help="Project directory (will be created/cleared for build)")
    parser.add_argument("--samples", default="./samples", help="Directory containing paired .insv")
    parser.add_argument("--calibration", default=None, help="Optional calibration.json path")
    parser.add_argument("--skip-colmap", action="store_true", help="Skip colmap stages")
    parser.add_argument("--skip-splat", action="store_true", help="Skip Gaussian Splat training")
    parser.add_argument("--splat-backend", default="msplat", choices=["msplat", "opensplat"], help="Gaussian Splat trainer to use (default: msplat)")
    parser.add_argument("--splat-args", default="", help="Extra args for the splat trainer, in its own flag syntax (e.g. opensplat: '--downscale-factor 2 -n 1000', msplat: '--downscale-factor 2 --num-iters 30000')")
    parser.add_argument("--interactive", action="store_true", help="Pause between steps for interactive use (default: batch, no pause)")
    parser.add_argument("--pause", action="store_true", help="Alias for --interactive")
    parser.add_argument("--release", action="store_true", help="Use cargo run --release (faster)")
    parser.add_argument("--fast", action="store_true", help="Accelerated: Turbo + workers + downscale (fastest, slight quality loss)")
    parser.add_argument("--turbo", action="store_true", help="Alias for --fast")
    args = parser.parse_args()

    project = Path(args.project)
    samples = Path(args.samples)
    calibration = Path(args.calibration) if args.calibration else None
    accelerate = args.fast or args.turbo
    # Fast implies release for 20x speedup (debug 0.2 img/s → release 6.0 img/s)
    use_release = args.release or accelerate
    if accelerate and not use_release:
        print("→ --fast/--turbo implies --release for maximum speed")
        use_release = True

    # Resolve binaries: prefer target/release if --release/--fast or exists
    def bin_cmd(name):
        release = Path(f"target/release/{name}")
        debug = Path(f"target/debug/{name}")
        if use_release and release.exists():
            return [str(release)]
        if release.exists() and not debug.exists():
            return [str(release)]
        base = ["cargo", "run", "--bin", name]
        if use_release:
            base.insert(2, "--release")
        base.append("--")
        return base

    imu_bin = bin_cmd("imu-keyframes")
    mask_bin = bin_cmd("scene-mask")

    # Ensure swift helper built
    swift_helper = Path("target/release/vision-person")
    if not swift_helper.exists():
        swift_helper = Path("target/debug/vision-person")
    if not swift_helper.exists():
        print("Building vision helper...")
        subprocess.run(["swiftc", "tools/vision_person.swift", "-o", "target/debug/vision-person"])
        subprocess.run(["swiftc", "tools/vision_person.swift", "-o", "target/release/vision-person"])

    def find_binary(names, candidates):
        for p in candidates:
            if Path(p).exists():
                return p
        for cand in names:
            try:
                out = subprocess.run(["which", cand], capture_output=True, text=True)
                if out.returncode == 0 and out.stdout.strip():
                    return out.stdout.strip()
            except:
                pass
        return names[0]

    def find_opensplat():
        return find_binary(["opensplat"], ["/Users/scottsyms/.local/bin/opensplat", "/opt/homebrew/bin/opensplat"])

    def find_msplat():
        # Installed via `pipx install msplat[cli]` (see README) — pipx puts the entry point
        # in ~/.local/bin, which may not be on PATH for a non-interactive shell.
        return find_binary(["msplat-train"], [str(Path.home() / ".local/bin/msplat-train"), "/opt/homebrew/bin/msplat-train"])

    def prepare_splat_project(proj: Path):
        # Both opensplat and msplat expect a COLMAP dataset at <input>/sparse/0 + <input>/images.
        # Our layout is <project>/colmap/sparse/0 and <project>/images. Create symlinks so
        # <project> itself satisfies that: <project>/sparse -> colmap/sparse (relative to
        # <project>, i.e. just "colmap/sparse" — NOT proj/"colmap"/"sparse", which previously
        # produced a target string like "room/colmap/sparse" that resolves to the nonexistent
        # "room/room/colmap/sparse" once the OS interprets it relative to the symlink's own
        # directory) and <project>/colmap/images -> ../images, for the colmap-subfolder fallback.
        try:
            sparse_link = proj / "sparse"
            colmap_sparse = proj / "colmap" / "sparse"
            if colmap_sparse.exists() and not sparse_link.exists():
                try:
                    sparse_link.symlink_to(Path("colmap") / "sparse", target_is_directory=True)
                except:
                    pass
            colmap_images = proj / "colmap" / "images"
            if not colmap_images.exists() and (proj / "images").exists():
                try:
                    colmap_images.symlink_to(Path("..") / "images", target_is_directory=True)
                except:
                    pass
        except Exception as e:
            print(f"  (prepare splat project warning: {e})")

    steps = []
    total = 0.0

    # Step 1: Build (extract + select + geometry + initial colmap layout)
    step1 = imu_bin + ["build", "--input-directory", str(samples), "--project", str(project)]
    if calibration:
        step1 += ["--calibration", str(calibration)]
    steps.append(("1/8 — Keyframe extraction (imu-keyframes build)", step1))

    # Step 2: Masking — accelerated with --turbo (384, fast, 0 dilate, 15° temporal, no colmap double-write)
    if accelerate:
        step2 = mask_bin + ["--images", str(project / "images"), "--output", str(project / "masks"), "--colmap-layout", "--turbo", "--workers", "0"]
    else:
        step2 = mask_bin + ["--images", str(project / "images"), "--output", str(project / "masks"), "--colmap-layout", "--preset", "photogrammetry-clean"]
    steps.append(("2/8 — Masking (scene-mask)" + (" [turbo]" if accelerate else ""), step2))

    # Step 3: COLMAP init
    steps.append(("3/8 — COLMAP init (cameras/images)", imu_bin + ["colmap-init", "--project", str(project)]))

    # Step 4: Pair selection
    steps.append(("4/8 — Pair selection (colmap-pairs)", imu_bin + ["colmap-pairs", "--project", str(project), "--temporal-before", "4", "--temporal-after", "8"]))

    # Step 5: Feature extraction
    steps.append(("5/8 — COLMAP feature extraction", imu_bin + ["colmap-features", "--project", str(project)]))

    # Step 6: Matching
    steps.append(("6/8 — COLMAP matching", imu_bin + ["colmap-match", "--project", str(project), "--rig-verification"]))

    # Step 7: Mapping
    steps.append(("7/8 — COLMAP mapper (sparse reconstruction)", imu_bin + ["colmap-map", "--project", str(project), "--fix-rig"]))

    # Optionally diagnose
    steps.append(("7b — Diagnose", imu_bin + ["colmap-diagnose", "--project", str(project)]))

    # Step 8: Gaussian Splat — unless skipped
    def default_splat_extra(backend, fast):
        # Flag names differ per trainer: opensplat uses -n, msplat uses --num-iters.
        iters = "5000" if fast else "30000"
        downscale = "4" if fast else "2"
        if backend == "msplat":
            return ["--downscale-factor", downscale, "--num-iters", iters]
        return ["--downscale-factor", downscale, "-n", iters]

    def build_splat_cmd(backend, binary, input_dir, output_path, extra):
        if backend == "msplat":
            return [binary, "--input", str(input_dir), "--output", str(output_path)] + extra
        return [binary, str(input_dir), "-o", str(output_path)] + extra

    if not args.skip_splat:
        backend = args.splat_backend
        splat_bin = find_msplat() if backend == "msplat" else find_opensplat()
        splat_output = project / "splat.ply"
        splat_extra = shlex.split(args.splat_args) if args.splat_args else default_splat_extra(backend, accelerate)
        splat_cmd = build_splat_cmd(backend, splat_bin, project, splat_output, splat_extra)
        steps.append((f"8/8 — Gaussian Splat ({backend})" + (" [fast]" if accelerate else ""), splat_cmd))

    if args.skip_colmap:
        # Keep only build + mask + splat (if not skipped), but splat needs colmap so skip it too if colmap skipped
        steps = [s for s in steps if "COLMAP" not in s[0] and "Diagnose" not in s[0] and "Splat" not in s[0]]
        # Re-add splat only if explicitly not skipped and colmap was skipped? For now keep only build+mask
        if not args.skip_splat and not args.skip_colmap:
            pass  # already handled

    print(f"\nFull workflow: {len(steps)} steps")
    print(f"Project: {project}   Samples: {samples}")
    if calibration:
        print(f"Calibration: {calibration}")

    overall_start = time.time()

    interactive = args.interactive or args.pause
    for title, cmd in steps:
        print(f"\n\n### {title}")
        # Prepare for the splat trainer: ensure symlinks for COLMAP layout
        if "Splat" in title:
            prepare_splat_project(project)
            # Try project root first, fallback to colmap subfolder if that layout wasn't found
            elapsed, code = run_step(cmd)
            if code != 0:
                backend = args.splat_backend
                alt_extra = shlex.split(args.splat_args) if args.splat_args else default_splat_extra(backend, accelerate)
                alt_cmd = build_splat_cmd(backend, cmd[0], project / "colmap", project / "splat.ply", alt_extra)
                print(f"\n  (trying fallback: {' '.join(shlex.quote(c) for c in alt_cmd)})")
                elapsed2, code2 = run_step(alt_cmd)
                elapsed = elapsed2
                code = code2
        else:
            elapsed, code = run_step(cmd)
        total += elapsed
        if interactive:
            pause()

    overall = time.time() - overall_start

    print("\n" + "="*78)
    print("Workflow complete")
    print(f"  Steps: {len(steps)}")
    print(f"  Total time: {overall:.1f}s  ({overall/60:.1f} min)")
    print(f"  Sum of step times: {total:.1f}s")
    print(f"  Project: {project}")
    print(f"  Next: check {project}/colmap/sparse/0/ and {project}/masks/cam0/")
    print("="*78)

if __name__ == "__main__":
    main()
