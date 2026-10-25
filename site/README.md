# Performance page

One static page of the latest accepted macOS comparison: zgx against GPUI and
GPUI Fast (QuickGUI in the text-and-list scene) on the heavy dashboard, the
text-and-list scene and the animation. `index.html`, `data.json` and
`data.csv` are written by the builder; `style.css` and `_headers` are
maintained by hand. Every trial stays in `docs/results`, which the page links.

Rebuild from the three runs of one commit (each accepted, full, clean):

    python3 scripts/build_site.py \
        docs/results/latest-macos-<date>-<commit> \
        docs/results/latest-macos-heavy-<date>-<commit> \
        docs/results/latest-macos-animate-<date>-<commit>

The builder recomputes every median and range from the runs' trials, checks
them against the runs' summaries, and checks each claim the headline makes
(it stops rather than publish one the numbers do not support).

Look at it with `python3 -m http.server 8788 --directory site`. Deploy with
`npx wrangler pages deploy site --project-name zgui-performance --branch main`
(the Cloudflare project keeps its name). Live: https://zgui-performance.pages.dev
