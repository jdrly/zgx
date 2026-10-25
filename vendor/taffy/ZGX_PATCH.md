# zgx patch to taffy 0.9.0

Vendored from crates.io `taffy` 0.9.0 (MIT, `LICENSE.md` from the
repository's v0.9.0 tag, which the published crate leaves out) and
published as `zgx-taffy`, its library still `taffy`, so zgx's crates build
from crates.io without a `[patch]` section. The change is marked `zgx:` in
the source.

## Why

A flex line with an auto margin (`margin_left = auto` to push items to the
end, a toolbar's spacer) lost its gaps: where free space goes to auto
margins, upstream set each item's main-axis offset to the margin alone,
dropping the `gap` between items that every other path keeps. CSS keeps
the gaps.

## What

1. In `src/compute/flexbox.rs`, distributing free space to auto margins
   sets each item's offset to the gap before it (none before the first;
   the last in a reversed line), then adds its auto margins as before.

Drop the fork when a taffy release keeps the gaps (the zgx test is
`crates/zgx/tests/layout_gaps.rs`).
