#!/bin/zsh
# A Time Profiler recording of a command, or of a running process, exported
# for samples.py and lines.py. macOS (xctrace).
#
#   scripts/perf/record.sh out.trace 10 -- command args...   # launch it
#   scripts/perf/record.sh out.trace 10 --attach PID         # a running app
#
# Writes out.trace and out.xml (the time-profile table). Launch an app with
# a clean environment (env -i HOME=$HOME PATH=$PATH ...) by attaching to it:
# xctrace samples a launched `env` instead of what it runs.
set -e
out=$1 seconds=$2
shift 2
rm -rf $out
if [[ $1 == --attach ]]; then
  target=(--attach $2)
else
  [[ $1 == -- ]] && shift
  target=(--launch -- "$@")
fi
perl -e 'alarm shift; exec @ARGV' $((seconds + 60)) \
  xcrun xctrace record --template 'Time Profiler' --time-limit ${seconds}s --output $out $target >/dev/null 2>&1 || true
# xctrace leaves its kernel traces behind.
rm -f $(getconf DARWIN_USER_TEMP_DIR)/instruments*.ktrace(N)
xcrun xctrace export --input $out \
  --xpath '/trace-toc/run[@number="1"]/data/table[@schema="time-profile"]' > ${out%.trace}.xml
echo "${out%.trace}.xml: $(grep -c '<row' ${out%.trace}.xml) samples"
