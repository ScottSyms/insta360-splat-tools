#!/usr/bin/env python3
"""
Full workflow runner: keyframe extraction → masking → matching pair selection → COLMAP
Times each step, echoes command, pauses for user input, totals time.

Skips Gaussian Splat training (no binary available per spec).

Usage:
  python3 workflow.py
  python3 workflow.py --project ./room --samples ./samples
  python3 workflow.py --skip-colmap  # skip colmap stages if colmap not installed
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
    parser = argparse.ArgumentParser(description="Batch workflow for imu-keyframes + scene-mask (COLMAP)")
    parser.add_argument("--project", default="./room", help="Project directory (will be created/cleared for build)")
    parser.add_argument("--samples", default="./samples", help="Directory containing paired .insv")
    parser.add_argument("--calibration", default=None, help="Optional calibration.json path")
    parser.add_argument("--skip-colmap", action="store_true", help="Skip colmap stages")
    parser.add_argument("--interactive", action="store_true", help="Pause between steps for interactive use (default: batch, no pause)")
    parser.add_argument("--pause", action="store_true", help="Alias for --interactive")
    parser.add_argument("--release", action="store_true", help="Use cargo run --release (faster)")
    args = parser.parse_args()

    project = Path(args.project)
    samples = Path(args.samples)
    calibration = Path(args.calibration) if args.calibration else None

    # Resolve binaries: prefer target/release if --release or exists
    def bin_cmd(name):
        release = Path(f"target/release/{name}")
        debug = Path(f"target/debug/{name}")
        if args.release and release.exists():
            return [str(release)]
        if release.exists() and not debug.exists():
            return [str(release)]
        # fallback to cargo run
        profile = "--release" if args.release else ""
        # Use cargo run wrapper
        base = ["cargo", "run", "--bin", name]
        if args.release:
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

    steps = []
    total = 0.0

    # Step 1: Build (extract + select + geometry + initial colmap layout)
    step1 = imu_bin + ["build", "--input-directory", str(samples), "--project", str(project)]
    if calibration:
        step1 += ["--calibration", str(calibration)]
    steps.append(("1/7 — Keyframe extraction (imu-keyframes build)", step1))

    # Step 2: Masking
    step2 = mask_bin + ["--images", str(project / "images"), "--output", str(project / "masks"), "--colmap-layout", "--preset", "photogrammetry-clean"]
    # Alternative legacy: --input ./frames --output ./cleaned
    steps.append(("2/7 — Masking (scene-mask)", step2))

    # Step 3: COLMAP init
    steps.append(("3/7 — COLMAP init (cameras/images)", imu_bin + ["colmap-init", "--project", str(project)]))

    # Step 4: Pair selection
    steps.append(("4/7 — Pair selection (colmap-pairs)", imu_bin + ["colmap-pairs", "--project", str(project), "--temporal-before", "4", "--temporal-after", "8"]))

    # Step 5: Feature extraction
    steps.append(("5/7 — COLMAP feature extraction", imu_bin + ["colmap-features", "--project", str(project)]))

    # Step 6: Matching
    steps.append(("6/7 — COLMAP matching", imu_bin + ["colmap-match", "--project", str(project), "--rig-verification"]))

    # Step 7: Mapping
    steps.append(("7/7 — COLMAP mapper (sparse reconstruction)", imu_bin + ["colmap-map", "--project", str(project), "--fix-rig"]))

    # Optionally diagnose
    steps.append(("7b — Diagnose", imu_bin + ["colmap-diagnose", "--project", str(project)]))

    if args.skip_colmap:
        steps = steps[:2]  # only build + mask

    print(f"\nFull workflow: {len(steps)} steps")
    print(f"Project: {project}   Samples: {samples}")
    if calibration:
        print(f"Calibration: {calibration}")

    overall_start = time.time()

    interactive = args.interactive or args.pause
    for title, cmd in steps:
        print(f"\n\n### {title}")
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
