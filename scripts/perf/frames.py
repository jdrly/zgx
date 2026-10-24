#!/usr/bin/env python3
"""Stack frame size per function in an arm64 Mach-O binary, largest first.

    frames.py BINARY [SUBSTRING...]

Reads each function's prologue (`sub sp` and stack probes). A function that
grows its frame (a large struct kept by value) costs a probe and stack
traffic on every call; this finds them. Pipe names through rustfilt if
installed.
"""
import sys, subprocess, struct, re
binary = sys.argv[1]; subs = sys.argv[2:]
out = subprocess.run(['otool', '-l', binary], capture_output=True, text=True).stdout
m = re.search(r'sectname __text\n\s+segname __TEXT\n\s+addr (0x[0-9a-f]+)\n\s+size (0x[0-9a-f]+)\n\s+offset (\d+)', out)
addr, size, off = int(m[1], 16), int(m[2], 16), int(m[3])
data = open(binary, 'rb').read()
syms = []
for line in subprocess.run(['nm', '-n', binary], capture_output=True, text=True).stdout.splitlines():
    p = line.split()
    if len(p) == 3 and p[1] in 'tT':
        syms.append((int(p[0], 16), p[2]))
dem = {}
names = [s for _, s in syms if not subs or any(x in s for x in subs)]
res = []
for a, s in syms:
    if subs and not any(x in s for x in subs): continue
    o = a - addr + off
    words = struct.unpack('<24I', data[o:o+96]) if o + 96 <= len(data) else ()
    probe = None; plain = 0
    for w in words:
        if (w >> 23) == 0x1A2:
            sh = (w >> 22) & 1; imm = ((w >> 10) & 0xFFF) << (12 * sh); rn = (w >> 5) & 31; rd = w & 31
            if rn == 31 and rd == 9: probe = imm
            elif rn == 31 and rd == 31:
                if probe is not None and sh == 1 and imm == 0x1000: continue
                plain += imm
        if w == 0xd65f03c0: break  # ret
    res.append(((probe or 0) + plain, s))
res.sort(reverse=True)
filt = subprocess.run(['rustfilt'], input='\n'.join(s for _, s in res[:60]), capture_output=True, text=True).stdout.splitlines() if subprocess.run(['which','rustfilt'],capture_output=True).returncode==0 else None
for i, (f, s) in enumerate(res[:60]):
    name = filt[i] if filt else s
    print(f'{f:8d}  {name[:150]}')
