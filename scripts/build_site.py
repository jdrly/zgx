#!/usr/bin/env python3
"""The public performance page (site/): one page of the latest accepted macOS
comparison, every scene of it, from the runs scripts/run_macos_comparison.py
recorded.

    python3 scripts/build_site.py \\
        docs/results/latest-macos-<date>-<commit> \\
        docs/results/latest-macos-heavy-<date>-<commit> \\
        docs/results/latest-macos-animate-<date>-<commit>

Only accepted, full (not quick or profile-guided) runs of one clean commit are
published. Every median and range shown is recomputed from the runs' trials
(current.csv, pacing.csv) and checked against their summaries, and every
claim the page's headline makes is checked against the numbers. Writes
site/index.html (charts as text and CSS, no script), site/data.json and
site/data.csv.
"""
import argparse, csv, html, json, math, statistics
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REPO = 'https://github.com/zeronsh/zgx'
NAMES = {'zgx': 'zgx', 'gpui': 'GPUI', 'gpui-fast': 'GPUI Fast', 'quickgui': 'QuickGUI'}
SCENES = {
    'heavy': ('Heavy dashboard', 'A 1280×800 dashboard: 6 tiles, 30 cards of 26 sparkline bars, a status table '
              'and a log. Most values change 60 times a second; the table scrolls.'),
    'lab': ('Text and a 100,000-row list', 'A streaming transcript beside a 100,000-row virtual list, updated 60 '
            'times a second. QuickGUI runs this scene only.'),
    'animate': ('Animation', '48 blinking dots and a shimmering line of text, animated on every refresh of a '
                '120 Hz display.'),
}
MODES = {'idle': 'Idle', 'stream': 'Streaming', 'scroll': 'Scrolling', 'both': 'Both', 'animate': 'Animating'}
# metric: (title, unit, digits, scale, source, modes it applies to)
METRICS = {
    'cpu': ('CPU', '% of one core', 1, 1, 'cpu_percent_one_core', 'all'),
    'gpu': ('GPU work', 'million GPU cycles a second', 1, 1, 'gpu_mcycles_per_second', 'active'),
    'p99': ('Frame time, 99th percentile', 'ms between frames', 1, 1, 'interval_p99_ms', 'paced'),
    'long': ('Long frames', '% of frames over 25 ms', 1, 100, 'long_interval_fraction', 'paced'),
    'memory': ('Memory', 'MB, physical footprint', 0, 1e-6, 'mean_footprint_bytes', 'all'),
}
TRIAL_KEYS = ['cpu_percent_one_core', 'gpu_mcycles_per_second', 'mean_footprint_bytes', 'windowserver_cpu_percent']
PACING_KEYS = ['interval_p99_ms', 'long_interval_fraction', 'frames_per_second']


def spread(values):
    return {'median': statistics.median(values), 'min': min(values), 'max': max(values)}


def check(summary_cell, values, what):
    for field, value in spread(values).items():
        assert math.isclose(summary_cell[field], value, rel_tol=1e-9, abs_tol=1e-9), (what, field, summary_cell[field], value)


def load(directory):
    """One accepted run: its cells, each recomputed from the trials."""
    audit = json.loads((directory / 'audit.json').read_text())
    summary = json.loads((directory / 'summary.json').read_text())
    protocol = audit['protocol']
    assert audit['accepted'], f'{directory}: not accepted'
    assert not protocol.get('quick') and not protocol.get('pgo'), f'{directory}: a quick or PGO run'
    assert not audit['source_dirty'], f'{directory}: measured with uncommitted changes'
    frameworks, modes = protocol['frameworks'], protocol['modes']
    trials = list(csv.DictReader((directory / 'current.csv').open()))
    paced = list(csv.DictReader((directory / 'pacing.csv').open()))
    cells = []
    for framework in frameworks:
        for mode in modes:
            rows = [t for t in trials if (t['framework'], t['mode']) == (framework, mode)]
            want = protocol.get('idle_repeats', protocol['repeats']) if mode == 'idle' else protocol['repeats']
            assert len(rows) == want and {int(t['repeat']) for t in rows} == set(range(want)), (directory, framework, mode)
            assert all(int(t['exit_code']) == 0 or t['terminated_after_report'].lower() == 'true' for t in rows)
            group = next(g for g in summary['groups'] if (g['framework'], g['mode']) == (framework, mode))
            cell = {'framework': framework, 'mode': mode, 'repeats': want}
            for key in TRIAL_KEYS:
                values = [float(t[key]) for t in rows if t.get(key) not in (None, '')]
                if values:
                    check(group[key], values, (directory.name, framework, mode, key))
                    cell[key] = spread(values)
            if mode != 'idle':
                ticks = [int(t['workload_ticks']) for t in rows]
                assert all(t > 0 for t in ticks), (directory, framework, mode)
                captures = [p for p in paced if (p['framework'], p['mode']) == (framework, mode)]
                assert len(captures) == protocol['pacing_repeats'], (directory, framework, mode)
                pace = next(p for p in summary['pacing'] if (p['framework'], p['mode']) == (framework, mode))
                for key in PACING_KEYS:
                    values = [float(p[key]) for p in captures]
                    check(pace[key], values, (directory.name, framework, mode, key))
                    cell[key] = spread(values)
                cell['pacing_captures'] = len(captures)
            cells.append(cell)
    env = audit['environment']
    return {
        'scene': audit['scene'], 'run': f'docs/results/{directory.name}', 'commit': audit['commit'],
        'started_utc': audit['started_utc'], 'frameworks': frameworks, 'modes': modes, 'cells': cells,
        'trials': audit['trials'], 'pacing_trials': audit['pacing_trials'], 'protocol': protocol,
        'environment': env, 'binaries': audit['binaries'],
    }


def cell(scene, framework, mode):
    return next(c for c in scene['cells'] if (c['framework'], c['mode']) == (framework, mode))


def value(scene, framework, mode, key, field='median'):
    return cell(scene, framework, mode)[key][field]


def fmt(v, digits):
    return f'{v:,.{digits}f}'


def headline(by):
    """The verdict and headline numbers, each checked against the data."""
    heavy = by['heavy']
    active = [(s, m) for s in by.values() for m in s['modes'] if m != 'idle']
    rivals = ['gpui', 'gpui-fast']
    # zgx below both GPUIs in CPU and GPU work in every active cell of every scene.
    for key in ('cpu_percent_one_core', 'gpu_mcycles_per_second'):
        for scene, mode in active:
            for rival in rivals:
                assert value(scene, 'zgx', mode, key) < value(scene, rival, mode, key), (scene['scene'], mode, key, rival)
    less = lambda key, mode, rival: 1 - value(heavy, 'zgx', mode, key) / value(heavy, rival, mode, key)
    heavy_modes = [m for m in heavy['modes'] if m != 'idle']
    gpu = [less('gpu_mcycles_per_second', m, r) for m in heavy_modes for r in rivals]
    long_zgx = max(value(s, 'zgx', m, 'long_interval_fraction') for s, m in active)
    long_rivals = max(value(s, r, m, 'long_interval_fraction') for s, m in active for r in rivals)
    assert long_zgx == 0, 'the headline says zgx has no long frames'
    idle = {f: value(heavy, f, 'idle', 'cpu_percent_one_core') for f in ['zgx', *rivals]}
    assert idle['zgx'] < min(idle[r] for r in rivals)
    verdict = ('In every test, zgx uses less CPU and less GPU than GPUI and GPUI Fast, '
               'and none of its frames run long.')
    cards = [
        (f"{less('cpu_percent_one_core', 'both', 'gpui') * 100:.0f}% / {less('cpu_percent_one_core', 'both', 'gpui-fast') * 100:.0f}%",
         'less CPU than GPUI / GPUI Fast',
         f"heavy dashboard, both: {fmt(value(heavy, 'zgx', 'both', 'cpu_percent_one_core'), 1)}% of a core against "
         f"{fmt(value(heavy, 'gpui', 'both', 'cpu_percent_one_core'), 1)}% and {fmt(value(heavy, 'gpui-fast', 'both', 'cpu_percent_one_core'), 1)}%"),
        (f'{min(gpu) * 100:.0f}–{max(gpu) * 100:.0f}%', 'less GPU work than either GPUI',
         'heavy dashboard, every active mode'),
        ('0%', 'long frames (over 25 ms)',
         f'median of 8 captures, every mode of every scene; GPUI and GPUI Fast: medians up to {long_rivals * 100:.1f}%'),
        (f"{fmt(idle['zgx'], 2)}%", 'CPU while idle',
         f"GPUI {fmt(idle['gpui'], 2)}%, GPUI Fast {fmt(idle['gpui-fast'], 2)}%"),
    ]
    return verdict, cards


def chart(scene, metric):
    title, unit, digits, scale, key, applies = METRICS[metric]
    modes = [m for m in scene['modes'] if applies == 'all' or m != 'idle']
    rows = [(m, f, c[key]) for m in modes for f in scene['frameworks']
            for c in [cell(scene, f, m)] if key in c]
    top = max(s['median'] * scale for _, _, s in rows) or 1
    out = [f'<figure class="chart"><table><caption><span class="title">{title}</span> '
           f'<span class="unit">{unit}</span> <span class="better">lower is better</span></caption>',
           '<colgroup><col class="c-mode"><col class="c-fw"><col></colgroup>'
           '<thead><tr><th scope="col">Mode</th><th scope="col">Framework</th>'
           '<th scope="col">Median <span class="range-head">(range)</span></th></tr></thead>']
    for mode in modes:
        group = [(f, s) for m, f, s in rows if m == mode]
        out.append('<tbody>')
        for i, (framework, s) in enumerate(group):
            median, low, high = (s[k] * scale for k in ('median', 'min', 'max'))
            width = 100 * median / top
            # Idle CPU is a fraction of a percent: two places show it.
            places = 2 if (metric, mode) == ('cpu', 'idle') else digits
            head = f'<th scope="rowgroup" rowspan="{len(group)}" class="mode">{MODES[mode]}</th>' if i == 0 else ''
            low, high = fmt(low, places), fmt(high, places)
            spread_text = '' if low == high else f' <span class="range">({low}–{high})</span>'
            out.append(f'<tr class="fw-{framework}">{head}<th scope="row">{NAMES[framework]}</th>'
                       f'<td><span class="track" aria-hidden="true"><span class="bar" style="width:{width:.1f}%"></span></span>'
                       f'<span class="value">{fmt(median, places)}</span>{spread_text}</td></tr>')
        out.append('</tbody>')
    out.append('</table></figure>')
    return '\n'.join(out)


def page(by, verdict, cards, data):
    env = by['heavy']['environment']
    protocol = by['heavy']['protocol']
    commit = by['heavy']['commit']
    date = by['heavy']['started_utc'][:10]
    displays = env['displays']
    display = (f"{len(displays)} × {displays[0]['name']} ({displays[0]['resolution']})"
               if all(d['name'] == displays[0]['name'] and d['resolution'] == displays[0]['resolution'] for d in displays)
               else ', '.join(f"{d['name']} ({d['resolution']})" for d in displays))
    e = html.escape
    card_html = '\n'.join(f'<div class="card"><dt>{e(label)}</dt><dd><span class="big">{e(big)}</span>'
                          f'<span class="note">{e(note)}</span></dd></div>' for big, label, note in cards)
    sections = []
    for name in ('heavy', 'lab', 'animate'):
        scene = by[name]
        title, note = SCENES[name]
        charts = '\n'.join(chart(scene, m) for m in METRICS
                           if any(METRICS[m][4] in c for c in scene['cells']))
        sections.append(
            f'<section id="{name}" aria-labelledby="{name}-h"><h2 id="{name}-h">{e(title)}</h2>'
            f'<p class="scene-note">{e(note)} <a href="{REPO}/tree/main/{scene["run"]}">Raw data</a></p>'
            f'<div class="charts">{charts}</div></section>')
    runs = ''.join(f'<li><a href="{REPO}/tree/main/{by[n]["run"]}"><code>{by[n]["run"]}</code></a> '
                   f'({by[n]["trials"]} trials, {by[n]["pacing_trials"]} pacing captures)</li>' for n in ('heavy', 'lab', 'animate'))
    return f'''<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>zgx performance: the latest benchmark</title>
<meta name="description" content="{e(verdict)}">
<meta name="color-scheme" content="light dark">
<link rel="stylesheet" href="style.css">
</head>
<body>
<header>
<p class="eyebrow">zgx performance · macOS · {date}</p>
<h1>{e(verdict)}</h1>
<p class="lede">zgx against GPUI (0.2.2) and GPUI Fast, the same three scenes built from the same workload code, measured on one Mac. Lower is better throughout.</p>
<dl class="cards">
{card_html}
</dl>
<nav aria-label="Scenes"><a href="#heavy">Heavy dashboard</a> <a href="#lab">Text and list</a> <a href="#animate">Animation</a> <a href="#method">How this was measured</a></nav>
</header>
<main>
{''.join(sections)}
<section id="method" aria-labelledby="method-h">
<h2 id="method-h">How this was measured</h2>
<ul class="facts">
<li><strong>Machine:</strong> {e(env['chip'])} ({e(env['machine'])}, {env['cores']} cores, {env['memory_bytes'] // 2**30} GB), macOS {e(env['macos'])}, {e(env['power'])}.</li>
<li><strong>Displays:</strong> {e(display)}; the window on the main display, nothing covering it.</li>
<li><strong>Builds:</strong> every framework with its <code>dist</code> profile (release, fat LTO, one codegen unit) and the system allocator; {e(env['rust'])}. zgx at <a href="{REPO}/commit/{commit}"><code>{commit[:7]}</code></a>.</li>
<li><strong>Trials:</strong> each a fresh launch of {protocol['requested_seconds']} s, the first {protocol['warmup_seconds']} s discarded, the next {protocol['sampled_seconds']} s sampled every {int(protocol['interval_seconds'] * 1000)} ms; {protocol['repeats']} trials of each active mode, {protocol.get('idle_repeats', protocol['repeats'])} of idle, sampled {protocol['idle_window_seconds'][0]}–{protocol['idle_window_seconds'][1]} s after launch. Medians are shown, the trials' range beside them. Frame timing is {protocol['pacing_repeats']} separate {protocol['pacing_seconds']} s captures per mode.</li>
<li><strong>CPU:</strong> the app process's CPU time over the sample, as a percentage of one core (it can exceed 100).</li>
<li><strong>GPU work:</strong> GPU cycles the app's process used per second, from the GPU's counters.</li>
<li><strong>Frame time:</strong> the 99th percentile of the intervals between frames shown on screen; at 60 updates a second, 16.7 ms is on time (8.3 ms for the animation, drawn at 120 Hz). <strong>Long frames</strong> are intervals over 25 ms.</li>
<li><strong>Memory:</strong> the process's physical footprint (what Activity Monitor shows: resident memory plus GPU memory it owns), averaged over the sample.</li>
</ul>
<h3>Caveats</h3>
<ul class="facts">
<li>Idle memory is two-level: an idle window settles at one of two levels about 25 MB apart, launch to launch, as the Metal driver returns its startup memory. Which level a launch lands on is the driver's timing, not the app's, so read idle memory with its range.</li>
<li>In the text-and-list scene, zgx's memory ties GPUI Fast's in the active modes.</li>
<li>The window server's own CPU, which drawing to the screen adds, is level for all three frameworks; it is in the data, not charted.</li>
<li>Binary size is not compared.</li>
<li>One machine, one session per scene; a personal Mac in use, other applications kept running (each run waited until the machine was quiet). Windows and Linux are not compared here.</li>
</ul>
<h3>Data</h3>
<ul class="facts">
<li>This page's numbers: <a href="data.csv">data.csv</a>, <a href="data.json">data.json</a>.</li>
<li>Every trial, raw: {runs}</li>
<li>How the runs are made: <a href="{REPO}/blob/main/docs/benchmarking.md">benchmarking</a>; the code: <a href="{REPO}">github.com/zeronsh/zgx</a>.</li>
</ul>
</section>
</main>
<footer><p>Built by <code>scripts/build_site.py</code> from the accepted runs; every number recomputed from their trials.</p></footer>
</body>
</html>
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('results', type=Path, nargs=3, help='the lab, heavy and animate runs, in any order')
    parser.add_argument('--output', type=Path, default=ROOT / 'site')
    args = parser.parse_args()
    by = {s['scene']: s for s in map(load, args.results)}
    assert set(by) == {'heavy', 'lab', 'animate'}, f'one run of each scene, not {sorted(by)}'
    assert len({s['commit'] for s in by.values()}) == 1, 'every scene measured on one commit'
    verdict, cards = headline(by)
    data = {
        'verdict': verdict,
        'commit': by['heavy']['commit'],
        'frameworks': NAMES,
        'metrics': {m: {'title': t, 'unit': u, 'source': k} for m, (t, u, _, _, k, _) in METRICS.items()},
        'scenes': [{k: by[n][k] for k in ('scene', 'run', 'started_utc', 'frameworks', 'modes', 'trials',
                                          'pacing_trials', 'protocol', 'environment', 'cells')}
                   for n in ('heavy', 'lab', 'animate')],
    }
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / 'data.json').write_text(json.dumps(data, indent=1) + '\n')
    with (args.output / 'data.csv').open('w', newline='') as out:
        writer = csv.writer(out, lineterminator='\n')
        writer.writerow(['scene', 'mode', 'framework', 'metric', 'median', 'min', 'max', 'trials'])
        for n in ('heavy', 'lab', 'animate'):
            for c in by[n]['cells']:
                for key in TRIAL_KEYS + PACING_KEYS:
                    if key in c:
                        trials = c['pacing_captures'] if key in PACING_KEYS else c['repeats']
                        writer.writerow([n, c['mode'], c['framework'], key, c[key]['median'], c[key]['min'], c[key]['max'], trials])
    (args.output / 'index.html').write_text(page(by, verdict, cards, data))
    counts = ', '.join(f"{n} ({by[n]['trials']} trials)" for n in ('heavy', 'lab', 'animate'))
    print(f"Published {counts} at {by['heavy']['commit'][:7]} to {args.output}")


if __name__ == '__main__':
    main()
