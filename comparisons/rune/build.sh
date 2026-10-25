#!/bin/sh
# Builds the Rune adapters into bin/ with Rune's own benchmark profile
# (`--profile perf`; its bench scripts refuse other builds) against the
# pinned dev rev, checked out into rune-src/ with PATCH.md's Tx::wheel
# applied, and writes build.json, which the comparison runner copies into
# each run's audit.
#
#     cd comparisons/rune && ./build.sh
set -eu
cd "$(dirname "$0")"

RUNE_REV=998bef587f09e359d877b35c66e48b8f355e73cb
if [ ! -d rune-src/.git ]; then
	git clone --quiet https://github.com/anomalyco/rune.git rune-src
fi
if ! git -C rune-src cat-file -e "$RUNE_REV^{commit}" 2>/dev/null; then
	git -C rune-src fetch --quiet origin
fi
git -C rune-src checkout --quiet --force --detach "$RUNE_REV"
git -C rune-src apply ../rune-wheel.patch

target=${CARGO_TARGET_DIR:-target}
cargo build --profile perf --locked --bins
mkdir -p bin
for scene in heavy lab animate vlist; do
	cp "$target/perf/$scene" "bin/$scene"
done

cat >build.json <<JSON
{
  "rune_rev": "$RUNE_REV",
  "rune_branch": "dev",
  "rune_license": "MIT",
  "rune_source": "https://github.com/anomalyco/rune",
  "rune_patch": "PATCH.md: Tx::wheel, Rune's own wheel path (Runtime::wheel_xy) for the app (sha256 $(shasum -a 256 rune-wheel.patch | cut -c1-64))",
  "rustc": "$(rustc --version)",
  "profile": "perf (Rune's: release, opt-level 3, codegen-units 16, no LTO, no debug, not incremental)",
  "binaries": ["bin/heavy", "bin/lab", "bin/animate", "bin/vlist"],
  "built_utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
JSON
echo "built bin/{heavy,lab,animate,vlist} with Rune $RUNE_REV (patched, PATCH.md)"
