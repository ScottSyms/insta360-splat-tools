#!/usr/bin/env python3
"""
Full workflow runner: keyframe extraction → masking → matching pair selection → COLMAP →
undistort → Gaussian Splat (msplat by default, opensplat via --splat-backend). Gates splat
training on a reconstruction quality check (coverage/fragmentation) and checks the trained
.ply for NaN gaussians afterward. Batch by default, --interactive to pause between steps.

When run in a terminal, shows a curses TUI: a fixed header (current step, live elapsed/
total time, the running command line) above a scrolling pane with that command's live
stdout/stderr. Falls back to plain sequential print output when stdout isn't a terminal
(redirected to a file/pipe) or with --no-tui.

Usage:
  python3 workflow.py
  python3 workflow.py --project ./room --samples ./samples
  python3 workflow.py --skip-colmap  # skip colmap stages if colmap not installed
  python3 workflow.py --skip-splat   # skip splat training
  python3 workflow.py --no-tui       # plain print output even in a terminal
"""

import argparse
import curses
import re
import subprocess
import sys
import os
import select
import time
import shlex
from pathlib import Path

# Our own Rust binaries' logger (tracing_subscriber) emits ANSI color codes unconditionally
# — it doesn't check whether stdout is a real terminal before coloring, so they show up even
# though we've piped its stdout to read it. curses.addnstr doesn't interpret escape codes
# (it's not a terminal emulator), so left in, they render as literal "^[[2m..." text in the
# TUI's body pane instead of doing anything. Strip them before display.
_ANSI_RE = re.compile(r"\x1b\[[0-9;]*[a-zA-Z]")

def strip_ansi(s):
    return _ANSI_RE.sub("", s)

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


class Tui:
    """Fixed header (step name, elapsed/total time, command line) over a scrolling
    body pane showing the running subprocess's live stdout/stderr, using curses so
    the header stays pinned while output scrolls beneath it (no new dependency —
    curses is stdlib, keeping workflow.py's "no pip deps" property)."""

    HEADER_LINES = 4

    def __init__(self, stdscr):
        self.stdscr = stdscr
        curses.curs_set(0)
        stdscr.nodelay(True)
        self.output_lines = []
        self.height = self.width = 0
        self._make_windows()

    def _make_windows(self):
        self.height, self.width = self.stdscr.getmaxyx()
        body_h = max(1, self.height - self.HEADER_LINES)
        self.header_win = curses.newwin(min(self.HEADER_LINES, self.height), self.width, 0, 0)
        self.body_win = curses.newwin(body_h, self.width, min(self.HEADER_LINES, self.height), 0)

    def _safe_addnstr(self, win, y, x, s, attr=0):
        w = win.getmaxyx()[1]
        n = w - x - 1
        if n <= 0:
            return
        try:
            win.addnstr(y, x, s, n, attr)
        except curses.error:
            pass  # writing to the bottom-right cell raises in some terminals; harmless

    def check_resize(self):
        ch = self.stdscr.getch()
        while ch != -1:
            if ch == curses.KEY_RESIZE:
                curses.update_lines_cols()
                self._make_windows()
                self._redraw_body()
            ch = self.stdscr.getch()

    def set_header(self, step_idx, step_total, title, cmd_str, elapsed, overall_elapsed):
        w = self.header_win
        w.erase()
        self._safe_addnstr(w, 0, 0, f" [{step_idx}/{step_total}] {title}", curses.A_BOLD)
        if self.HEADER_LINES > 1 and self.height > 1:
            self._safe_addnstr(w, 1, 0, f" elapsed: {elapsed:7.1f}s    total: {overall_elapsed:8.1f}s")
        if self.HEADER_LINES > 2 and self.height > 2:
            self._safe_addnstr(w, 2, 0, f" $ {cmd_str}", curses.A_DIM)
        if self.HEADER_LINES > 3 and self.height > 3:
            self._safe_addnstr(w, 3, 0, "-" * self.width)
        w.noutrefresh()

    def append_output(self, line):
        for l in line.splitlines() or [""]:
            self.output_lines.append(strip_ansi(l))
        if len(self.output_lines) > 4000:
            self.output_lines = self.output_lines[-4000:]
        self._redraw_body()

    def _redraw_body(self):
        w = self.body_win
        w.erase()
        h = w.getmaxyx()[0]
        for i, line in enumerate(self.output_lines[-h:]):
            self._safe_addnstr(w, i, 0, line)
        w.noutrefresh()

    def refresh(self):
        curses.doupdate()

    def wait_key(self, msg):
        self.append_output(msg)
        self.refresh()
        self.stdscr.nodelay(False)
        try:
            self.stdscr.getch()
        finally:
            self.stdscr.nodelay(True)


def run_step_tui(cmd, title, tui, step_idx, step_total, overall_start):
    """Like run_step, but streams the subprocess's output into the TUI's body pane
    and keeps the header's elapsed-time counter live even during long silent stretches
    (e.g. COLMAP's rig-verification pass, which prints nothing for many minutes)."""
    cmd_str = " ".join(shlex.quote(c) for c in cmd)
    start = time.time()
    try:
        proc = subprocess.Popen(
            cmd, stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            text=True, bufsize=1,
        )
    except OSError as e:
        tui.append_output(f"! failed to start: {e}")
        tui.refresh()
        return 0.0, 127

    while True:
        tui.check_resize()
        elapsed = time.time() - start
        tui.set_header(step_idx, step_total, title, cmd_str, elapsed, time.time() - overall_start)
        ready, _, _ = select.select([proc.stdout], [], [], 0.2)
        if ready:
            line = proc.stdout.readline()
            if line == "":
                break  # EOF: process closed stdout (has exited or is about to)
            tui.append_output(line.rstrip("\n"))
        tui.refresh()

    proc.wait()
    elapsed = time.time() - start
    tui.append_output(f"-> step finished in {elapsed:.1f}s (exit {proc.returncode})")
    if proc.returncode != 0:
        tui.append_output(f"! command failed with code {proc.returncode}, continuing anyway...")
    tui.set_header(step_idx, step_total, title, cmd_str, elapsed, time.time() - overall_start)
    tui.refresh()
    return elapsed, proc.returncode

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
    parser.add_argument("--force", action="store_true", help="Train the splat even if the reconstruction quality check (step 7c) fails, using its best-available sub-model anyway")
    parser.add_argument("--min-coverage", type=float, default=0.9, help="Reconstruction quality gate: minimum fraction of images the best sub-model must register (default 0.9)")
    parser.add_argument("--max-reproj-error", type=float, default=2.0, help="Reconstruction quality gate: max mean reprojection error in px (default 2.0)")
    parser.add_argument("--no-tui", action="store_true", help="Disable the curses TUI (fixed header + scrolling output pane) even when running in a terminal; plain print output instead")
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

    def prepare_undistorted_splat_input(undistorted_dir: Path, staging_dir: Path):
        # `colmap image_undistorter --output_type COLMAP` writes cameras.bin/images.bin/
        # points3D.bin directly under <undistorted_dir>/sparse (no "0" subdirectory), and
        # images under <undistorted_dir>/images — neither matches what a trainer expects
        # (<input>/sparse/0 + <input>/images) on its own. Stage a directory that does, using
        # os.path.relpath (not hand-counted ".." chains) so the symlink target is correct
        # regardless of nesting depth.
        staging_dir.mkdir(parents=True, exist_ok=True)
        images_link = staging_dir / "images"
        # .exists() follows symlinks, so it's False for a stale/broken one left over from
        # an earlier run against a since-removed target — which then makes symlink_to()
        # below fail with FileExistsError (the dirent itself is still there).
        if images_link.is_symlink() or images_link.exists():
            images_link.unlink()
        images_link.symlink_to(
            os.path.relpath(undistorted_dir / "images", staging_dir), target_is_directory=True
        )
        sparse_dir = staging_dir / "sparse"
        sparse_dir.mkdir(exist_ok=True)
        zero_link = sparse_dir / "0"
        if zero_link.is_symlink() or zero_link.exists():
            zero_link.unlink()
        zero_link.symlink_to(
            os.path.relpath(undistorted_dir / "sparse", sparse_dir), target_is_directory=True
        )

    steps = []

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

    # Catches a bad reconstruction (low coverage, fragmented into disconnected sub-models)
    # before it gets silently fed to a splat trainer. Also stages splat_input/ so the
    # trainer (which always reads <input>/sparse/0, with no flag to pick a different
    # sub-model) actually points at whichever sub-model is best, not just index 0.
    splat_input = project / "splat_input"
    steps.append((
        "7c — COLMAP reconstruction quality check",
        [sys.executable, "check_quality.py", "reconstruction", "--project", str(project),
         "--min-coverage", str(args.min_coverage), "--max-reproj-error", str(args.max_reproj_error),
         "--prepare-input", str(splat_input)],
    ))

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
        # Our calibration is OPENCV_FISHEYE (real ~200 deg FOV lenses) — correct for
        # reconstruction, but neither msplat's nor opensplat's COLMAP loader supports that
        # model (checked both sources directly: msplat errors "Unsupported COLMAP camera
        # model" on anything but SIMPLE_PINHOLE/PINHOLE/SIMPLE_RADIAL/RADIAL/OPENCV).
        # `colmap image_undistorter` converts the fisheye reconstruction + images to a
        # PINHOLE one either trainer can read. Verified end-to-end on a 60-frame test
        # project: the previous OPENCV-as-fisheye default produced 58-60% NaN gaussians
        # (gradient-based optimization diverging on degenerate reprojection geometry);
        # OPENCV_FISHEYE + this undistortion step produced 0% NaN, 100% image coverage in
        # one sub-model (vs. fragmenting into ~10), PSNR 20.9 (vs. ~10).
        undistorted_dir = project / "undistorted"
        steps.append((
            "7d — Undistort (fisheye -> pinhole for splat training)",
            ["colmap", "image_undistorter",
             "--image_path", str(project / "images"),
             "--input_path", str(splat_input / "sparse" / "0"),
             "--output_path", str(undistorted_dir),
             "--output_type", "COLMAP"],
        ))
        splat_input_undistorted = project / "undistorted_staged"

        backend = args.splat_backend
        splat_bin = find_msplat() if backend == "msplat" else find_opensplat()
        splat_output = project / "splat.ply"
        splat_extra = shlex.split(args.splat_args) if args.splat_args else default_splat_extra(backend, accelerate)
        splat_cmd = build_splat_cmd(backend, splat_bin, splat_input_undistorted, splat_output, splat_extra)
        steps.append((f"8/8 — Gaussian Splat ({backend})" + (" [fast]" if accelerate else ""), splat_cmd))
        # Catches a trainer that "succeeds" (normal exit code, normal-sized file) but wrote
        # majority-NaN gaussians because optimization diverged — invisible from the exit
        # code or file size alone, only visible by actually looking at the vertex data.
        steps.append((
            "8b — Gaussian quality check",
            [sys.executable, "check_quality.py", "splat", str(splat_output)],
        ))

    if args.skip_colmap:
        # Keep only build + mask — undistortion, splat, and its quality check all need
        # colmap output, so they're skipped too.
        steps = [
            s for s in steps
            if "colmap" not in s[0].lower() and "Diagnose" not in s[0]
            and "Gaussian" not in s[0] and "Undistort" not in s[0]
        ]

    print(f"\nFull workflow: {len(steps)} steps")
    print(f"Project: {project}   Samples: {samples}")
    if calibration:
        print(f"Calibration: {calibration}")

    interactive = args.interactive or args.pause

    def after_step(title, code):
        """Shared post-step handling for both runners below. Returns (should_stop, message)."""
        if title.startswith("7d") and code == 0:
            prepare_undistorted_splat_input(undistorted_dir, splat_input_undistorted)
        if title.startswith("7c") and code != 0 and not args.force:
            return True, (
                "STOPPING: reconstruction quality check failed (see above) — training a "
                "splat from this would very likely reproduce a broken/incoherent result "
                "(fragmented coverage, or a reprojection error high enough to indicate bad "
                "geometry). Pass --force to train anyway against the best sub-model found "
                f"(staged at {splat_input}), or --min-coverage/--max-reproj-error to relax "
                "the thresholds."
            )
        return False, None

    def run_all_plain():
        total = 0.0
        overall_start = time.time()
        for title, cmd in steps:
            print(f"\n\n### {title}")
            elapsed, code = run_step(cmd)
            total += elapsed
            stop, msg = after_step(title, code)
            if msg:
                print("\n" + "="*78 + f"\n{msg}\n" + "="*78)
            if stop:
                break
            if interactive:
                pause()
        return total, time.time() - overall_start

    def run_all_tui(stdscr):
        tui = Tui(stdscr)
        total = 0.0
        overall_start = time.time()
        try:
            for idx, (title, cmd) in enumerate(steps, 1):
                elapsed, code = run_step_tui(cmd, title, tui, idx, len(steps), overall_start)
                total += elapsed
                stop, msg = after_step(title, code)
                if msg:
                    tui.append_output("")
                    tui.append_output("=" * 78)
                    tui.append_output(msg)
                    tui.append_output("=" * 78)
                    tui.refresh()
                if stop:
                    break
                if interactive:
                    tui.wait_key(" -- step complete, press any key to continue --")
        except KeyboardInterrupt:
            pass
        return total, time.time() - overall_start

    use_tui = sys.stdout.isatty() and not args.no_tui
    if use_tui:
        try:
            total, overall = curses.wrapper(run_all_tui)
        except curses.error as e:
            print(f"(TUI unavailable ({e}), falling back to plain output)")
            total, overall = run_all_plain()
    else:
        total, overall = run_all_plain()

    print("\n" + "="*78)
    print("Workflow complete")
    print(f"  Steps: {len(steps)}")
    print(f"  Total time: {overall:.1f}s  ({overall/60:.1f} min)")
    print(f"  Sum of step times: {total:.1f}s")
    print(f"  Project: {project}")
    print(f"  Next: check {splat_input}/sparse/0 (symlinked to the best sub-model) and {project}/masks/cam0/")
    print("="*78)

if __name__ == "__main__":
    main()
