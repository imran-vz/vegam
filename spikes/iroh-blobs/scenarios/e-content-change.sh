#!/bin/bash
# Scenario E: Sender content change must fail safely.
# The sender imports with TryReference (data referenced in place). Mutate the
# source file mid-transfer and verify the receiver fails with a verification
# error, never writes the destination, and reports cleanly.
source "$(dirname "$0")/lib.sh"

FILE="$WORK/src-e-2gb.bin"
dd if=/dev/urandom of="$FILE" bs=1m count=2048 2>/dev/null
SRC_SIZE=$(stat -f%z "$FILE")

rm -rf "$WORK/send-e" "$WORK/recv-e" "$WORK/dest-e.bin"
start_sender e "$FILE" || { fail "sender startup"; finish; }

"$BIN" recv --store "$WORK/recv-e" --ticket "$TICKET" --dest "$WORK/dest-e.bin" \
  > "$WORK/recv-e.out" 2> "$WORK/recv-e.err" &
RECV_PID=$!

MUTATE_AT=$((SRC_SIZE / 4))
if wait_for_progress "$WORK/recv-e.out" "$MUTATE_AT" 300; then
  # Overwrite bytes well ahead of the current transfer position.
  dd if=/dev/urandom of="$FILE" bs=1m count=64 seek=1536 conv=notrunc 2>/dev/null
  pass "source mutated mid-transfer (64MB at offset 1.5GB)"
else
  fail "never reached mutation threshold"; cleanup_pids "$RECV_PID" "$SENDER_PID"; finish
fi

for _ in $(seq 1 300); do kill -0 "$RECV_PID" 2>/dev/null || break; sleep 1; done
if kill -0 "$RECV_PID" 2>/dev/null; then
  fail "receiver still running 300s after content change"; cleanup_pids "$RECV_PID" "$SENDER_PID"; finish
fi
wait "$RECV_PID"; RECV_RC=$?

[ "$RECV_RC" -ne 0 ] && pass "receiver exited non-zero ($RECV_RC)" || fail "receiver exited 0 despite content change"
grep -q "^FAILED: " "$WORK/recv-e.out" && pass "receiver reported FAILED" || fail "no FAILED marker"
# Attribute the failure to the content change, not a coincidental transient
# error: the provider hits the mutated region and resets the stream, so the
# failure must be a peer reset AND the last observed position must sit below
# the mutated region (+1 throttle interval of slack).
MUTATE_OFFSET=$((1536 * 1024 * 1024))
LAST_PROGRESS=$(grep "^PROGRESS: " "$WORK/recv-e.out" | tail -1 | cut -d' ' -f2)
grep "^FAILED: " "$WORK/recv-e.out" | grep -qi "reset" \
  && pass "failure is a provider-side stream reset" \
  || fail "failure cause is not a peer reset: $(grep '^FAILED: ' "$WORK/recv-e.out")"
assert_lt "failure position below the mutated region (+slack)" "$LAST_PROGRESS" "$((MUTATE_OFFSET + 128 * 1024 * 1024))"
grep -q "^EXPORTED: " "$WORK/recv-e.out" && fail "export happened despite corruption" || pass "no export after corruption"
[ -f "$WORK/dest-e.bin" ] && fail "corrupt-looking file at destination" || pass "destination untouched"

cleanup_pids "$SENDER_PID"
finish
