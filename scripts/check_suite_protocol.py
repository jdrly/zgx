#!/usr/bin/env python3
"""Whether the suite's short protocol reads what the full one did: each
median of a suite run (run_macos_comparison.py --suite) against the range of
the trials of a full-protocol run of the same scene, cell by cell.

    python3 scripts/check_suite_protocol.py SUITE_RUN FULL_RUN [--cells gpui-fast:*,zgx:cpu,zgx:memory]

`--cells` limits the check to cells both runs measured the same way (the
full run's other cells measured an older build): `framework:metric`, `*`
for every metric. Writes protocol-check.json beside the suite run and prints
a table; exits 1 when a median falls outside its range by more than the
allowance (the full run's own spread, at least 2% of its median), which is
how far two sessions of the full protocol differ.
"""
import argparse, json, statistics, sys
from pathlib import Path

METRICS = {'cpu': 'cpu_percent_one_core', 'gpu': 'gpu_mcycles_per_second', 'memory': 'mean_footprint_bytes'}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('suite', type=Path)
    parser.add_argument('full', type=Path)
    parser.add_argument('--cells', default='*:*')
    args = parser.parse_args()
    suite = json.loads((args.suite / 'summary.json').read_text())['groups']
    full = json.loads((args.full / 'summary.json').read_text())['groups']
    wanted = [c.split(':') for c in args.cells.split(',')]
    rows, failed = [], []
    for g in suite:
        if g['framework'] == 'desktop':
            continue
        old = next((o for o in full if (o['framework'], o['mode']) == (g['framework'], g['mode'])), None)
        if old is None:
            continue
        for name, key in METRICS.items():
            if not any(f in ('*', g['framework']) and m in ('*', name) for f, m in wanted):
                continue
            if key not in g or key not in old or old[key]['median'] == 0:
                continue
            # Idle memory settles at one of two levels by the driver's
            # timing, launch to launch, whatever the protocol.
            if name == 'memory' and g['mode'] == 'idle':
                continue
            median, low, high = g[key]['median'], old[key]['min'], old[key]['max']
            allowance = max(high - low, 0.02 * old[key]['median'])
            outside = max(low - median, median - high, 0)
            ok = outside <= allowance
            rows.append({'framework': g['framework'], 'mode': g['mode'], 'metric': name, 'suite_median': median,
                         'full_min': low, 'full_max': high, 'full_median': old[key]['median'],
                         'within_range': low <= median <= high, 'within_allowance': ok})
            if not ok:
                failed.append(rows[-1])
    inside = sum(r['within_range'] for r in rows)
    report = {'suite': str(args.suite), 'full': str(args.full), 'cells': args.cells, 'checked': len(rows),
              'within_range': inside, 'within_allowance': len(rows) - len(failed), 'rows': rows}
    (args.suite / 'protocol-check.json').write_text(json.dumps(report, indent=1) + '\n')
    for r in rows:
        scale = 1e-6 if r['metric'] == 'memory' else 1
        mark = 'in range' if r['within_range'] else ('near' if r['within_allowance'] else 'OUTSIDE')
        print(f"{r['framework']:9} {r['mode']:7} {r['metric']:6} suite {r['suite_median'] * scale:9.2f}  "
              f"full {r['full_min'] * scale:9.2f}–{r['full_max'] * scale:<9.2f} {mark}")
    print(f'{inside} of {len(rows)} medians inside the full protocol\'s range; '
          f'{len(rows) - len(failed)} within its allowance')
    sys.exit(1 if failed else 0)


if __name__ == '__main__':
    main()
