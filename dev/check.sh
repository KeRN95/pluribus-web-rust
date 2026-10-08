#!/bin/sh
# The checks, from WSL (or Linux). Given words, only the tests whose names hold them, for while a
# change is being made; without, what a commit needs: clippy over every target and every test,
# side by side, each in a target directory of its own so that neither waits for the other's lock.
#
#   sh dev/check.sh                  clippy, and every test
#   sh dev/check.sh heading sort     the tests with "heading" or "sort" in their names
#
# The target directories default to ones outside the repository, under ~/.cache, which build far
# faster than a Windows drive under WSL (/mnt/c); CHECK_TARGET and CLIPPY_TARGET change them.
# The database of compose.yaml must be up, its template1 migrated (dev/test_template.sh).
set -u
cd "$(dirname "$0")/.."
TESTS="${CHECK_TARGET:-$HOME/.cache/marketplace-target-test}"
LINTS="${CLIPPY_TARGET:-$HOME/.cache/marketplace-target}"
THREADS="${TEST_THREADS:-8}"

if [ "$#" -gt 0 ]; then
  for words in "$@"; do
    CARGO_TARGET_DIR="$TESTS" cargo test -q "$words" -- --test-threads="$THREADS" || exit 1
  done
  exit 0
fi

LOG="$(mktemp -d)"
CARGO_TARGET_DIR="$LINTS" cargo clippy -q --all-targets >"$LOG/clippy" 2>&1 &
CLIPPY=$!
CARGO_TARGET_DIR="$TESTS" cargo test -q -- --test-threads="$THREADS" >"$LOG/tests" 2>&1
TESTED=$?
wait "$CLIPPY"
LINTED=$?
echo "== clippy"
grep -E -A6 "^(warning|error)" "$LOG/clippy" | grep -E "^(warning|error)|-->" || echo "nothing"
echo "== tests"
grep -E "^test result|FAILED|panicked" "$LOG/tests"
if [ "$TESTED" -ne 0 ]; then
  echo "== failures"
  sed -n '/^failures:$/,$p' "$LOG/tests" | head -80
fi
rm -rf "$LOG"
[ "$TESTED" -eq 0 ] && [ "$LINTED" -eq 0 ]
