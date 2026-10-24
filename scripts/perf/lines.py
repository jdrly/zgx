#!/usr/bin/env python3
"""A Time Profiler export's leaf samples by source line and inlined function.

    lines.py out.xml BINARY FN [N]

Takes the samples whose stack passes through FN (a substring) and whose leaf
is in BINARY, and resolves each leaf address with `atos -i`, so the time an
inlined helper takes shows under its own name and line. Build with line
tables (`CARGO_PROFILE_RELEASE_DEBUG=line-tables-only`) for lines.
"""
import collections
import subprocess
import sys
import xml.etree.ElementTree as ET


def main():
    if len(sys.argv) < 4:
        sys.exit(__doc__)
    path, binary, under = sys.argv[1:4]
    count = int(sys.argv[4]) if len(sys.argv) > 4 else 30
    ids = {}

    def resolve(element):
        if 'ref' in element.attrib:
            return ids[element.attrib['ref']]
        if 'id' in element.attrib:
            ids[element.attrib['id']] = element
        for child in element:
            resolve(child)
        return element

    addresses, load, elsewhere = collections.Counter(), None, 0
    for row in ET.parse(path).getroot().iter('row'):
        for cell in row:
            cell = resolve(cell)
            if cell.tag != 'tagged-backtrace':
                continue
            stack = cell.find('backtrace')
            stack = resolve(stack) if stack is not None else None
            if stack is None:
                continue
            frames = [resolve(f) for f in stack if f.tag == 'frame']
            if not frames or not any(under in f.attrib.get('name', '') for f in frames):
                continue
            image = frames[0].find('binary')
            image = resolve(image) if image is not None else None
            if image is not None and binary.split('/')[-1] in image.attrib.get('path', ''):
                load = image.attrib.get('load-addr', load)
                addresses[frames[0].attrib['addr']] += 1
            else:
                elsewhere += 1
    keys = list(addresses)
    if not keys:
        sys.exit(f'no samples under {under} in {binary} ({elsewhere} elsewhere)')
    # A return address points past its call: step back into it.
    out = subprocess.run(['atos', '-i', '-o', binary, '-l', load] + [hex(int(k, 16) - 1) for k in keys],
                         capture_output=True, text=True).stdout
    lines, functions = collections.Counter(), collections.Counter()
    # With -i, each address prints its inline chain, blocks apart.
    for key, block in zip(keys, out.strip().split('\n\n')):
        first = block.splitlines()[0]
        lines[first[first.rfind('(') + 1:-1] if '(' in first else first] += addresses[key]
        functions[first.split(' (in ')[0][:110]] += addresses[key]
    total = sum(addresses.values())
    print(f'samples {total}, outside the binary {elsewhere}')
    print('-- by inlined function')
    for name, n in functions.most_common(count):
        print(f'{n:5d} {100 * n / total:5.1f}% {name}')
    print('-- by line')
    for name, n in lines.most_common(count):
        print(f'{n:5d} {100 * n / total:5.1f}% {name}')


if __name__ == '__main__':
    main()
