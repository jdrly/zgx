#!/usr/bin/env python3
"""Inside the VM: zgx vs GPUI on heavy (idle/stream/scroll/both) and animate,
under Xvfb + openbox on lavapipe/llvmpipe (software Vulkan). CPU from
/proc/<pid>/stat utime+stime, RSS from /proc: scripts/compare.py's run_trial.
Order rotates each repeat."""
import json, os, pathlib, subprocess, sys, time
sys.path.insert(0, os.path.expanduser('~/zgx/scripts'))
import compare

REPEATS = int(os.environ.get('REPEATS', 3))
SECONDS, WARMUP = 20., 5.
OUT = pathlib.Path(os.path.expanduser('~/linux-run')); OUT.mkdir(exist_ok=True)
Z = os.path.expanduser('~/zgx/target/dist/examples')
G = os.path.expanduser('~/zgx/comparisons/gpui/target/dist')
SCENES = {'heavy': ({'zgx': f'{Z}/heavy_workload', 'gpui': f'{G}/heavy'}, ['idle', 'stream', 'scroll', 'both']),
          'animate': ({'zgx': f'{Z}/animation_workload', 'gpui': f'{G}/animate'}, ['animate'])}
os.environ.update(DISPLAY=':99', LP_NUM_THREADS='4')
xvfb = subprocess.Popen(['Xvfb', ':99', '-screen', '0', '2560x1600x24', '-nolisten', 'tcp'], stderr=open(OUT / 'xvfb.log', 'w'))
time.sleep(1)
wm = subprocess.Popen(['openbox'], stdout=open(OUT / 'openbox.log', 'w'), stderr=subprocess.STDOUT)
time.sleep(1)
rows = []
try:
    for repeat in range(REPEATS):
        for scene, (bins, modes) in SCENES.items():
            names = list(bins)[repeat % 2:] + list(bins)[:repeat % 2]
            for mode in modes:
                for name in names:
                    row = compare.run_trial(pathlib.Path(bins[name]), name, mode, repeat, SECONDS, WARMUP, str(OUT / scene))
                    row['scene'] = scene
                    rows.append(row)
                    print(f"{scene:7} {name:4} {mode:7} r{repeat} CPU {row['cpu_percent_one_core']:7.2f}% "
                          f"RSS {row['mean_rss_bytes']/1048576:7.1f} MiB ticks {row['workload_ticks']}", flush=True)
                    time.sleep(1)
finally:
    wm.terminate(); xvfb.terminate()
    (OUT / 'rows.json').write_text(json.dumps(rows, indent=2))
