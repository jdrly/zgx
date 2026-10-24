#!/usr/bin/env python3
"""The window server's CPU for two configurations of one app, in paired seconds.

    python3 scripts/windowserver_alternate.py SECONDS command...

The command switches its own configuration on even and odd Unix seconds (an
app's scratch toggle). Each whole second's window server and app CPU time
(`ps -o time=`) is credited to the configuration that ran it, so the
desktop's drift cancels between neighbouring seconds. Prints the means and
the mean even-minus-odd difference with its standard error. macOS only.
See docs/performance-feature-audit.md.
"""
import statistics
import subprocess
import sys
import time


def cpu_seconds(pid):
    """A process's CPU time so far, or None once it is gone."""
    text = subprocess.run(['ps', '-o', 'time=', '-p', str(pid)], capture_output=True, text=True).stdout.strip()
    if not text:
        return None
    days = 0
    if '-' in text:
        days, text = text.split('-')
    value = 0.
    for part in text.split(':'):
        value = value * 60 + float(part)
    return value + int(days) * 86400


def sleep_to_next_second():
    now = time.time()
    time.sleep(int(now) + 1 - now + 0.02)


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    seconds, command = int(sys.argv[1]), sys.argv[2:]
    server = subprocess.run(['pgrep', '-x', 'WindowServer'], capture_output=True, text=True).stdout.split()[0]
    app = subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    # Past start-up, which belongs to neither configuration.
    time.sleep(4.5)
    server_by, app_by = {0: [], 1: []}, {0: [], 1: []}
    sleep_to_next_second()
    previous = (int(time.time()), cpu_seconds(server), cpu_seconds(app.pid))
    end = time.time() + seconds
    while time.time() < end and app.poll() is None:
        sleep_to_next_second()
        current = (int(time.time()), cpu_seconds(server), cpu_seconds(app.pid))
        if current[2] is None:
            break
        parity, elapsed = previous[0] % 2, current[0] - previous[0]
        server_by[parity].append((current[1] - previous[1]) / elapsed * 100)
        app_by[parity].append((current[2] - previous[2]) / elapsed * 100)
        previous = current
    app.terminate()
    app.wait()
    pairs = [even - odd for even, odd in zip(server_by[0], server_by[1])]
    if len(pairs) < 2:
        sys.exit('too few paired seconds')
    mean = lambda values: statistics.mean(values) if values else float('nan')
    print(f'even: window server {mean(server_by[0]):.2f}% app {mean(app_by[0]):.2f}% | '
          f'odd: window server {mean(server_by[1]):.2f}% app {mean(app_by[1]):.2f}% | '
          f'even - odd {mean(pairs):+.2f} (se {statistics.stdev(pairs) / len(pairs) ** 0.5:.2f}, n={len(pairs)})')


if __name__ == '__main__':
    main()
