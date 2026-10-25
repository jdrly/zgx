# Is the suite's short protocol reliable? (2026-10-25)

The same binaries (zgx `83e2bf…` and GPUI Fast `27ad2531…`, the GPUI Fast build
of the published heavy run `latest-macos-heavy-2026-10-25-282bfc0`), measured in
one quiet window:

- `suite/`: heavy under `run_macos_comparison.py --suite` (5 s warm-up, 10 s
  sampled), zgx and GPUI Fast.
- `full/`: GPUI Fast heavy-both under the full protocol (5 s warm-up, 15 s
  sampled), right after.
- `jump/`: the vlist jump mode, zgx and GPUI Fast, one trial each, with frame
  pacing and blank-row captures.

`reval-check-session.json` compares the suite's GPUI Fast heavy-both with
`full/`: CPU 18.46 against 17.57–18.32, GPU 22.02 against 21.94–22.18, memory
271 MB against 258–275 MB, all within the allowance. `reval-check-282.json`
compares the suite with the published run: 2 of 17 medians inside its ranges,
14 within the allowance. `full/` itself reads GPUI Fast's CPU at 18.17 against
the published 15.73–15.95, so the session differed, not the protocol.

An earlier attempt with a 3 s warm-up (not kept) read 5–10% more CPU for every
framework; the suite's warm-up is 5 s since.
