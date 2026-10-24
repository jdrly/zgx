#!/usr/bin/env python3
"""Aggregate a Time Profiler export (record.sh's .xml) by function.

    samples.py out.xml [N]                     # functions by self samples
    samples.py out.xml --incl [N]              # by samples anywhere in the stack
    samples.py out.xml --under=FN [--incl]     # only stacks through FN, cut at FN
    samples.py out.xml --children=FN           # FN's direct callees, by samples
    samples.py out.xml --callers=FN [--depth=3]  # what calls FN
    samples.py out.xml THREAD ...              # only threads whose name has THREAD
    samples.py out.xml --threads               # samples per thread

FN and THREAD match substrings (Rust symbols are mangled: `Host::draw`).
"""
import collections
import sys
import xml.etree.ElementTree as ET


def main():
    paths = [a for a in sys.argv[1:] if a.endswith('.xml')]
    args = [a for a in sys.argv[1:] if not a.endswith('.xml')]
    if not paths:
        sys.exit(__doc__)
    option = lambda name: next((a.split('=', 1)[1] for a in args if a.startswith(f'--{name}=')), None)
    incl, threads = '--incl' in args, '--threads' in args
    under, children, callers = option('under'), option('children'), option('callers')
    depth = int(option('depth') or 1)
    count = next((int(a) for a in args if a.isdigit()), 40)
    thread = next((a for a in args if not a.startswith('--') and not a.isdigit()), None)
    counts, total = collections.Counter(), 0
    for path in paths:
        ids = {}

        def resolve(element):
            if 'ref' in element.attrib:
                return ids[element.attrib['ref']]
            if 'id' in element.attrib:
                ids[element.attrib['id']] = element
            for child in element:
                resolve(child)
            return element

        for row in ET.parse(path).getroot().iter('row'):
            name, stack = None, None
            for cell in row:
                cell = resolve(cell)
                if cell.tag == 'thread':
                    name = cell.attrib.get('fmt', '')
                elif cell.tag == 'tagged-backtrace':
                    found = cell.find('backtrace')
                    stack = resolve(found) if found is not None else None
                elif cell.tag == 'backtrace':
                    stack = cell
            if thread and (name is None or thread not in name):
                continue
            if threads:
                total += 1
                counts[(name or '?').split(' (')[0]] += 1
                continue
            if stack is None:
                continue
            frames = [resolve(f).attrib.get('name', '?') for f in stack if f.tag == 'frame']
            if children or callers:
                at = [i for i, f in enumerate(frames) if (children or callers) in f]
                if not at:
                    continue
                total += 1
                i = at[-1]
                if children:
                    counts[frames[i - 1] if i > 0 else '<self>'] += 1
                else:
                    for caller in set(frames[i + 1:i + 1 + depth]):
                        counts[caller] += 1
                continue
            if under:
                at = [i for i, f in enumerate(frames) if under in f]
                if not at:
                    continue
                frames = frames[:at[-1] + 1]
            total += 1
            if incl:
                for frame in set(frames):
                    counts[frame] += 1
            else:
                counts[frames[0] if frames else '?'] += 1
    print('samples', total)
    for name, n in counts.most_common(count):
        print(f'{n:6d} {100 * n / max(total, 1):5.1f}% {name[:150]}')


if __name__ == '__main__':
    main()
