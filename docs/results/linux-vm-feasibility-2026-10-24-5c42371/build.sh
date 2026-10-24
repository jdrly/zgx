#!/bin/bash
# Runs inside the VM: dist builds of zgx's heavy/animate apps and GPUI's.
. ~/.cargo/env
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DIST_DEBUG=0
cd ~/zgx
echo "== $(date +%T) zgx"
timeout 3600 cargo build --profile dist --locked -p zgx-desktop --example heavy_workload --example animation_workload 2>&1 | tail -5
echo "   exit ${PIPESTATUS[0]}"
cd comparisons/gpui
echo "== $(date +%T) gpui"
timeout 3600 cargo build --profile dist --locked --bin heavy --bin animate 2>&1 | tail -15
echo "   exit ${PIPESTATUS[0]}"
echo "== $(date +%T) build done"
