#!/usr/bin/env bash
set -euo pipefail
source /tmp/ic-wire-standard-20261028-y6rOQ1/env.sh
for task_variant in baseline candidate; do
  task_checkout="/tmp/ic-wire-standard-20261028-y6rOQ1/$task_variant"
  task_output="/tmp/ic-wire-standard-20261028-y6rOQ1/results-$task_variant"
  export CARGO_TARGET_DIR="/tmp/ic-wire-standard-20261028-y6rOQ1/target-matrix-$task_variant"
  cd "$task_checkout"
  printf 'Starting maintained matrix: %s\n' "$task_variant"
  date -u '+%Y-%m-%dT%H:%M:%SZ' > "/tmp/ic-wire-standard-20261028-y6rOQ1/logs/$task_variant-matrix-started.txt"
  bash scripts/ci/evaluate-codec-matrix.sh "$task_output" > "/tmp/ic-wire-standard-20261028-y6rOQ1/logs/matrix-$task_variant.log" 2>&1
  export GIT_DIR="$(git rev-parse --absolute-git-dir)" GIT_WORK_TREE="$task_checkout"
  cd "$task_output/source"
  CODEC_SOURCE_SNAPSHOT="$task_output/source" CORPUS_FEATURE=hotspots bash scripts/ci/evaluate-codecs.sh "$task_output/hotspots" > "$task_output/hotspots.log" 2>&1
  unset GIT_DIR GIT_WORK_TREE
  date -u '+%Y-%m-%dT%H:%M:%SZ' > "/tmp/ic-wire-standard-20261028-y6rOQ1/logs/$task_variant-matrix-completed.txt"
  printf 'Completed matrix and hotspots: %s\n' "$task_variant"
done
