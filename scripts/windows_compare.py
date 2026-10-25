#!/usr/bin/env python3
"""zgx against GPUI Fast, MyGo and GPUI on Windows: the comparison scenes
(`comparisons/README.md`), each framework's adapter built at its strongest,
run in turn on the same desktop. The Windows counterpart of the macOS
comparison runner.

Scenes and modes (each framework's binary per `comparisons/README.md`):

- `lab`: `idle`, `stream` (text streaming), `scroll`, `both`;
- `animate`: display-rate animation;
- `heavy`: the dashboard, every live value (`both`);
- `vlist`: each framework's own variable-height list: `scroll`, `fling`,
  `jump`;
- `busy` (GPUI's old adapter only): a busy window with one spinner.

Each run lasts `--warmup` plus `--seconds`, sampled every 100 ms after the
warm-up: CPU time (`GetProcessTimes`), CPU cycles (`QueryProcessCycleTime`,
finer than CPU time's scheduler ticks), working set and private bytes. The
frameworks alternate in rotating order over `--repeats`; a scene and mode
whose cycles spread wider than `--spread` of their median gets another
round, up to `--max-repeats`. A run is valid only if it lasted as long as it
was asked and did its work: it must report its ticks (`workload_ticks`,
`"ticks"`) or frames (`animation_frames`), at least nine tenths of 60 a
second outside `idle`; an invalid run is run again once. Cycles are also
given per tick or frame, so a run that did less work cannot look cheaper.

    cargo build --profile dist --locked -p zgx-desktop --example component_workload  (and heavy_workload, animation_workload, vlist_workload)
    cargo build --profile dist --locked --manifest-path comparisons/gpui-fast/Cargo.toml
    (cd comparisons/mygo && ./build.sh)
    python scripts/windows_compare.py --zgx target/dist/examples --gpui-fast comparisons/gpui-fast/target/dist --mygo comparisons/mygo/bin --output target/windows-compare

Keep the windows uncovered, the machine otherwise idle, and on mains power.
"""
import argparse
import ctypes
import hashlib
import json
import os
import pathlib
import platform
import re
import statistics
import subprocess
import time

import windows_input as win

FRAMEWORKS = ("zgx", "gpui-fast", "mygo", "gpui")
# Scene: (modes, {framework: binary}).
SCENES = {
    "lab": (("idle", "stream", "scroll", "both"), {
        "zgx": "component_workload.exe", "gpui-fast": "zgx-compare-gpui-fast.exe", "mygo": "lab.exe",
        "gpui": "zgx-compare-gpui.exe"}),
    "animate": (("animate",), {
        "zgx": "animation_workload.exe", "gpui-fast": "animate.exe", "mygo": "animate.exe", "gpui": "animate.exe"}),
    "heavy": (("both",), {
        "zgx": "heavy_workload.exe", "gpui-fast": "heavy.exe", "mygo": "heavy.exe", "gpui": "heavy.exe"}),
    "vlist": (("scroll", "fling", "jump"), {
        "zgx": "vlist_workload.exe", "gpui-fast": "vlist.exe", "mygo": "vlist.exe"}),
    "busy": (("busy",), {"zgx": "busy_workload.exe", "gpui": "busy.exe"}),
}
TICKS_PER_SECOND = 60
# The one font file every adapter loads (`comparisons/fonts`), staged as the
# macOS runner stages it.
FONT = pathlib.Path(__file__).resolve().parent.parent / "comparisons" / "fonts" / "DejaVuSans.ttf"
font_file = None


def units(text):
    """The work a run reports it did."""
    for pattern in (r"workload_ticks=(\d+)", r'"ticks":(\d+)', r"animation_frames=(\d+)"):
        match = re.search(pattern, text)
        if match:
            return int(match.group(1))
    return None


# Processes that read other apps' windows through UI Automation without being
# screen readers: present, one may have attached to a run's window.
UIA_CLIENTS = ("TextInputHost", "VoiceAccess", "Magnify", "Narrator", "nvda", "jfw", "Inspect",
               "AccessibilityInsights", "WinAppDriver", "PowerToys", "LiveCaptions")


def accessibility_state():
    """Whether a screen reader runs (SPI_GETSCREENREADER), and which known
    UIA client processes run, as this machine is during a run."""
    flag = ctypes.c_int(0)
    ctypes.windll.user32.SystemParametersInfoW(0x0046, 0, ctypes.byref(flag), 0)
    out = subprocess.run(["tasklist", "/fo", "csv", "/nh"], capture_output=True, text=True).stdout
    running = {line.split('","')[0].strip('"').removesuffix(".exe") for line in out.splitlines() if line}
    return bool(flag.value), sorted(name for name in running if name in UIA_CLIENTS)


class Invalid(Exception):
    pass


def run(path, mode, seconds, warmup, log_path):
    total = warmup + seconds
    # ZGUI_*: adapters built before the rename read those alone.
    env = dict(os.environ, ZGX_MODE=mode, ZGX_SECONDS=f"{total:g}", ZGX_INITIAL_TICKS="0",
               ZGUI_MODE=mode, ZGUI_SECONDS=f"{total:g}", ZGUI_INITIAL_TICKS="0", ZGX_ACCESSIBILITY_LOG="1",
               **({"ZGX_FONT_FILE": str(font_file), "ZGUI_FONT_FILE": str(font_file)} if font_file else {}))
    started = time.perf_counter()
    samples = []
    with open(log_path, "w") as log:
        proc = subprocess.Popen([str(path)], env=env, stdout=log, stderr=log)
        try:
            while proc.poll() is None:
                elapsed = time.perf_counter() - started
                if elapsed > total + 30:
                    raise Invalid(f"{path.name} did not end")
                if elapsed >= warmup:
                    try:
                        private, working = win.memory(proc.pid)
                        samples.append((time.perf_counter(), win.cpu_seconds(proc.pid), win.cycles(proc.pid),
                                        working, private))
                    except OSError:
                        pass
                time.sleep(0.1)
        finally:
            if proc.poll() is None:
                proc.kill()
            code = proc.wait()
    lasted = time.perf_counter() - started
    text = pathlib.Path(log_path).read_text(errors="replace")
    done = units(text)
    # The last sample can be of a process already releasing its memory.
    samples = [s for s in samples if s[3] > 0][:-1]
    if code or len(samples) < 10 or done is None:
        raise Invalid(f"{path.name} {mode}: exit {code}, {len(samples)} samples, work {done}; see {log_path}")
    wall = samples[-1][0] - samples[0][0]
    if lasted < total * 0.95 or wall < seconds * 0.85:
        raise Invalid(f"{path.name} {mode}: ran {lasted:.1f} s, sampled {wall:.1f} s, of {total:g}")
    if mode != "idle" and done < 0.9 * TICKS_PER_SECOND * total:
        raise Invalid(f"{path.name} {mode}: {done} ticks or frames in {total:g} s; see {log_path}")
    cpu = samples[-1][1] - samples[0][1]
    cycles = samples[-1][2] - samples[0][2]
    # The work over the sampled span, of the whole run's.
    sampled_units = done * wall / total if done else 0
    return {
        "cpu_percent": 100 * cpu / wall,
        "mcycles_per_second": cycles / wall / 1e6,
        "mcycles_per_unit": (cycles / 1e6 / sampled_units) if sampled_units else None,
        "units": done,
        "working_set_mib": statistics.mean(s[3] for s in samples) / 2**20,
        "peak_working_set_mib": max(s[3] for s in samples) / 2**20,
        "private_mib": statistics.mean(s[4] for s in samples) / 2**20,
        # zgx says when a UI Automation client read its window (the others
        # have no provider, so nothing to read).
        "uia_client_attached": "accessibility: a client asked for the tree" in text,
    }


def key_metric(rows):
    """What a scene's spread is judged by: cycles per tick or frame where
    there is work, else per second."""
    per_unit = [r["mcycles_per_unit"] for r in rows if r["mcycles_per_unit"]]
    return per_unit if len(per_unit) == len(rows) else [r["mcycles_per_second"] for r in rows]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    for name in FRAMEWORKS:
        parser.add_argument(f"--{name}", type=pathlib.Path, help=f"{name}'s adapter binaries")
    parser.add_argument("--scenes", default="lab,animate,heavy,vlist")
    parser.add_argument("--seconds", type=float, default=10, help="sampled, after the warm-up")
    parser.add_argument("--warmup", type=float, default=3)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--max-repeats", type=int, default=6)
    parser.add_argument("--spread", type=float, default=0.05,
                        help="another round while (max - min) / median exceeds this")
    parser.add_argument("--font", type=pathlib.Path, default=FONT, help="the one font file every adapter loads")
    parser.add_argument("--output", type=pathlib.Path, default=pathlib.Path("target/windows-compare"))
    args = parser.parse_args()
    global font_file
    font_file = args.font.resolve() if args.font.exists() else None
    if font_file is None:
        print(f"no font file at {args.font}: each adapter draws in its own font")
    dirs = {name: getattr(args, name.replace("-", "_")) for name in FRAMEWORKS}
    dirs = {name: d for name, d in dirs.items() if d}
    if len(dirs) < 2:
        raise SystemExit("give at least two frameworks' binaries (--zgx, --gpui-fast, --mygo, --gpui)")
    args.output.mkdir(parents=True, exist_ok=True)
    screen_reader, uia_clients = accessibility_state()
    print(f"screen reader running: {screen_reader}; UI Automation clients running: {', '.join(uia_clients) or 'none known'}")
    results, binaries, failures = {}, {}, []
    rounds = 0

    def round_of(scene, mode, frameworks, index):
        shift = index % len(frameworks)
        for name in frameworks[shift:] + frameworks[:shift]:
            path = binaries[f"{scene}/{name}"]
            log = args.output / f"{scene}.{mode}.{name}.{len(results.get((scene, mode, name), []))}.log"
            for attempt in (1, 2):
                try:
                    row = run(path, mode, args.seconds, args.warmup, log)
                    break
                except Invalid as error:
                    if attempt == 2:
                        failures.append(str(error))
                        print(f"INVALID {error}", flush=True)
                        row = None
            if row is None:
                continue
            results.setdefault((scene, mode, name), []).append(row)
            print(f"{scene:7} {mode:6} {name:9} CPU {row['cpu_percent']:6.2f}%  "
                  f"{row['mcycles_per_second']:8.1f} Mcyc/s  {(row['mcycles_per_unit'] or 0):7.2f} Mcyc/unit  "
                  f"units {row['units']:5}  WS {row['working_set_mib']:6.1f}  private {row['private_mib']:6.1f} MiB"
                  f"{'  UIA client attached' if row['uia_client_attached'] else ''}", flush=True)

    for scene in args.scenes.split(","):
        modes, names = SCENES[scene]
        frameworks = [n for n in FRAMEWORKS if n in dirs and n in names and (dirs[n] / names[n]).exists()]
        for name in frameworks:
            binaries[f"{scene}/{name}"] = dirs[name] / names[name]
        if len(frameworks) < 2:
            print(f"{scene}: fewer than two frameworks have its binary; skipped")
            continue
        for mode in modes:
            for index in range(args.repeats):
                round_of(scene, mode, frameworks, index)
            index = args.repeats
            while index < args.max_repeats:
                spreads = []
                for name in frameworks:
                    values = key_metric(results.get((scene, mode, name), []))
                    if len(values) >= 2:
                        spreads.append((max(values) - min(values)) / statistics.median(values))
                if not spreads or max(spreads) <= args.spread:
                    break
                print(f"{scene} {mode}: spread {max(spreads):.1%}, another round", flush=True)
                round_of(scene, mode, frameworks, index)
                index += 1
    summary = []
    for (scene, mode, name), rows in results.items():
        median = {key: statistics.median(r[key] for r in rows if r[key] is not None)
                  for key in rows[0]
                  if not isinstance(rows[0][key], bool) and any(r[key] is not None for r in rows)}
        values = key_metric(rows)
        summary.append({"scene": scene, "mode": mode, "framework": name, "median": median,
                        "spread": (max(values) - min(values)) / statistics.median(values) if values else None,
                        "uia_client_attached_runs": sum(r["uia_client_attached"] for r in rows), "runs": rows})
    metadata = {
        "platform": platform.platform(),
        "processor": platform.processor(),
        "seconds": args.seconds,
        "warmup": args.warmup,
        "repeats": args.repeats,
        "max_repeats": args.max_repeats,
        "spread": args.spread,
        "screen_reader_running": screen_reader,
        "uia_client_processes_running": uia_clients,
        "invalid_runs": failures,
        "font_file": str(font_file) if font_file else None,
        "binary_bytes": {key: os.path.getsize(path) for key, path in binaries.items()},
        "binary_sha256": {key: hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()
                          for key, path in binaries.items()},
    }
    (args.output / "summary.json").write_text(json.dumps({"metadata": metadata, "results": summary}, indent=1))
    shown = [n for n in FRAMEWORKS if n in dirs]
    by = {(s["scene"], s["mode"], s["framework"]): s["median"] for s in summary}
    print("\nscene   mode    metric              " + "".join(f"{n:>11}" for n in shown))
    for scene in args.scenes.split(","):
        for mode in SCENES[scene][0]:
            for key in ("mcycles_per_unit", "mcycles_per_second", "cpu_percent", "private_mib", "working_set_mib"):
                cells = [by.get((scene, mode, n), {}).get(key) for n in shown]
                if sum(c is not None for c in cells) < 2:
                    continue
                print(f"{scene:7} {mode:7} {key:19} " + "".join(f"{c:11.2f}" if c is not None else f"{'-':>11}"
                                                              for c in cells))
    if failures:
        print(f"\n{len(failures)} invalid runs, see summary.json")


if __name__ == "__main__":
    main()
