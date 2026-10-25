#!/usr/bin/env python3
"""What a copy of selected display text puts on the macOS pasteboard.

    python3 scripts/macos_pasteboard_probe.py [--target-dir DIR]

Builds the selectable_text example and runs it with --pasteboard-probe: it
selects all of its document (headings, rich text, a 10,000-line virtual
transcript), copies it as Copy does, reads its own pasteboard back and
prints what it holds. Checks that the copy offers both public.html and
public.utf8-plain-text, that each is what was copied, and that the
transcript's rows it never mounted are in it.

The app reads only what it wrote itself, so macOS asks no one's leave
(another process reading the pasteboard would prompt). It is launched as a
direct child, from /tmp, with a clean environment, and sends no input: safe
on a desktop in use. If a system dialog appears, stop and report it; never
click it. macOS only.
"""
import argparse
import os
import pathlib
import platform
import re
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--target-dir', default=os.environ.get('CARGO_TARGET_DIR', str(ROOT / 'target')))
    args = parser.parse_args()
    if platform.system() != 'Darwin':
        print('macOS only')
        return 2
    env = dict(os.environ, CARGO_TARGET_DIR=args.target_dir)
    subprocess.run(['cargo', 'build', '-q', '-p', 'zgx-desktop', '--example', 'selectable_text', '--locked'], cwd=ROOT, env=env, check=True)
    binary = pathlib.Path(args.target_dir) / 'debug/examples/selectable_text'
    clean = {key: os.environ[key] for key in ('HOME', 'USER') if key in os.environ}
    clean['PATH'] = '/usr/bin:/bin'
    with tempfile.TemporaryDirectory(prefix='zgx-pasteboard-') as cwd:
        run = subprocess.run([str(binary), '--pasteboard-probe'], cwd=cwd, env=clean, capture_output=True, text=True, timeout=30)
    lines = [line for line in run.stdout.splitlines() if line.startswith('PASTEBOARD ')]
    print('\n'.join(line[:200] for line in lines))
    found = ' '.join(lines)
    checks = {
        'written': 'written=true' in found,
        'public.html offered': '"public.html"' in found,
        'public.utf8-plain-text offered': '"public.utf8-plain-text"' in found,
        'HTML is the copy': 'html_matches=true' in found,
        'text is the copy': 'text_matches=true' in found,
    }
    count = re.search(r'text_lines=(\d+)', found)
    checks['unmounted transcript rows copied'] = bool(count) and int(count.group(1)) > 10_000
    for name, ok in checks.items():
        print(f"{'ok  ' if ok else 'FAIL'} {name}")
    if run.returncode != 0:
        print(run.stderr[-2000:], file=sys.stderr)
    return 0 if all(checks.values()) and run.returncode == 0 else 1


if __name__ == '__main__':
    sys.exit(main())
