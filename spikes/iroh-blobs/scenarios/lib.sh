#!/bin/bash
# Shared helpers for the Phase 2 iroh-blobs production spike scenarios.
# Each scenario script prints PASS/FAIL lines; any FAIL exits non-zero.

set -u
SPIKE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$SPIKE_DIR/target/release/vegam-iroh-blobs-spike"
WORK="${SPIKE_WORK:-/tmp/vegam-spike}"
mkdir -p "$WORK"

FAILURES=0

pass() { echo "PASS: $*"; }
fail() { echo "FAIL: $*"; FAILURES=$((FAILURES + 1)); }

assert_eq() { # label actual expected
  if [ "$2" = "$3" ]; then pass "$1 ($2)"; else fail "$1: got '$2', want '$3'"; fi
}

assert_gt() { # label actual min
  if [ "$2" -gt "$3" ]; then pass "$1 ($2 > $3)"; else fail "$1: $2 not > $3"; fi
}

assert_lt() { # label actual max
  if [ "$2" -lt "$3" ]; then pass "$1 ($2 < $3)"; else fail "$1: $2 not < $3"; fi
}

finish() {
  if [ "$FAILURES" -eq 0 ]; then
    echo "SCENARIO_RESULT: PASS"
    exit 0
  else
    echo "SCENARIO_RESULT: FAIL ($FAILURES failures)"
    exit 1
  fi
}

# start_sender <name> <file> [extra args...] -> sets SENDER_PID, TICKET, SIZE
# If SENDER_RSS_FILE is set, RSS monitoring starts at spawn so the import/
# hashing phase is covered, not just the serving phase.
start_sender() {
  local name="$1" file="$2"
  shift 2
  "$BIN" send --store "$WORK/send-$name" --file "$file" "$@" \
    > "$WORK/send-$name.out" 2> "$WORK/send-$name.err" &
  SENDER_PID=$!
  if [ -n "${SENDER_RSS_FILE:-}" ]; then
    monitor_rss "$SENDER_PID" > "$SENDER_RSS_FILE" &
    SENDER_RSS_MON=$!
  fi
  for _ in $(seq 1 1200); do
    grep -q "^SERVING: 1" "$WORK/send-$name.out" 2>/dev/null && break
    if ! kill -0 "$SENDER_PID" 2>/dev/null; then
      echo "sender died during startup:" >&2
      cat "$WORK/send-$name.err" >&2
      return 1
    fi
    sleep 1
  done
  TICKET=$(grep "^TICKET: " "$WORK/send-$name.out" | cut -d' ' -f2)
  SIZE=$(grep "^SIZE: " "$WORK/send-$name.out" | cut -d' ' -f2)
  [ -n "$TICKET" ]
}

# wait_for_progress <recv_out_file> <bytes> [timeout_secs] -> 0 once a
# PROGRESS line exceeds <bytes>. PROGRESS prints at a 1s throttle, so the
# true position can be ~1.5s ahead of the observed line; assertions must use
# measured values, not the nominal threshold. Also returns 0 if the transfer
# completes before the threshold line appears (the caller's kill becomes a
# no-op and its own asserts decide the outcome).
wait_for_progress() {
  local out="$1" threshold="$2" timeout="${3:-120}"
  for _ in $(seq 1 $((timeout * 2))); do
    local last
    last=$(grep "^PROGRESS: " "$out" 2>/dev/null | tail -1 | cut -d' ' -f2)
    if [ -n "${last:-}" ] && [ "$last" -gt "$threshold" ]; then
      return 0
    fi
    grep -q "^COMPLETE: " "$out" 2>/dev/null && return 0
    sleep 0.5
  done
  return 1
}

# resume_point <recv_out_file> — the LOCAL_BYTES value the final (successful)
# session resumed from: the second-to-last LOCAL_BYTES line (the last one is
# printed when the loop re-checks and finds the blob complete).
resume_point() {
  grep "^LOCAL_BYTES: " "$1" | tail -2 | head -1 | cut -d' ' -f2
}

# max_rss_kb <pid> — sample RSS until process exits; echo max in KB
monitor_rss() {
  local pid="$1" max=0
  while kill -0 "$pid" 2>/dev/null; do
    local rss
    rss=$(ps -o rss= -p "$pid" 2>/dev/null | tr -d ' ')
    [ -n "${rss:-}" ] && [ "$rss" -gt "$max" ] && max=$rss
    sleep 1
  done
  echo "$max"
}

cleanup_pids() {
  for pid in "$@"; do
    kill "$pid" 2>/dev/null || true
  done
  wait 2>/dev/null || true
}
