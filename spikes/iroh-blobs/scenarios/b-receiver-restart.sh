#!/bin/bash
# Scenario B: Receiver app-restart resume.
# Kill the receiver mid-transfer (SIGKILL), verify partial state survives in
# the managed resume area, restart, and verify the second session fetches only
# the missing bytes and the export matches the source.
source "$(dirname "$0")/lib.sh"

FILE="$WORK/src-5gb.bin"
[ -f "$FILE" ] || { echo "generating 5GB random source…"; dd if=/dev/urandom of="$FILE" bs=1m count=5120 2>/dev/null; }
SRC_SIZE=$(stat -f%z "$FILE")

rm -rf "$WORK/send-b" "$WORK/recv-b" "$WORK/dest-b.bin"
start_sender b "$FILE" || { fail "sender startup"; finish; }

"$BIN" recv --store "$WORK/recv-b" --ticket "$TICKET" --dest "$WORK/dest-b.bin" \
  > "$WORK/recv-b1.out" 2> "$WORK/recv-b1.err" &
RECV_PID=$!

KILL_AT=$((SRC_SIZE * 2 / 5))
if wait_for_progress "$WORK/recv-b1.out" "$KILL_AT" 300; then
  kill -9 "$RECV_PID"
  pass "receiver SIGKILLed past $KILL_AT bytes"
else
  fail "receiver never reached kill threshold"
  cleanup_pids "$RECV_PID" "$SENDER_PID"; finish
fi
wait "$RECV_PID" 2>/dev/null

# Partial state must be visible in the managed resume area.
"$BIN" status --store "$WORK/recv-b" --ticket "$TICKET" > "$WORK/recv-b-status.out" 2>/dev/null
LOCAL=$(grep "^LOCAL_BYTES: " "$WORK/recv-b-status.out" | cut -d' ' -f2)
COMPLETE=$(grep "^IS_COMPLETE: " "$WORK/recv-b-status.out" | cut -d' ' -f2)
assert_gt "partial bytes survive receiver kill" "$LOCAL" 0
assert_eq "blob not complete after kill" "$COMPLETE" "false"
grep "^RESUME_FILE: " "$WORK/recv-b-status.out" | sed 's/^/resume area: /'
[ -f "$WORK/dest-b.bin" ] && fail "dest written before verification" || pass "no file at destination before completion"

# Restart receiver: must resume, not restart.
"$BIN" recv --store "$WORK/recv-b" --ticket "$TICKET" --dest "$WORK/dest-b.bin" \
  > "$WORK/recv-b2.out" 2> "$WORK/recv-b2.err"
START_LOCAL=$(grep "^LOCAL_BYTES: " "$WORK/recv-b2.out" | head -1 | cut -d' ' -f2)
SESSION2=$(grep "^TOTAL_SESSION_PAYLOAD_BYTES: " "$WORK/recv-b2.out" | cut -d' ' -f2)
assert_gt "second session starts from partial state" "$START_LOCAL" 0
assert_lt "second session fetched only missing bytes" "$SESSION2" "$((SRC_SIZE - START_LOCAL + SRC_SIZE / 20))"
grep -q "^EXPORTED: " "$WORK/recv-b2.out" && pass "exported after completion" || fail "no export"
cmp "$FILE" "$WORK/dest-b.bin" && pass "export byte-identical to source (Content Identity)" || fail "export differs from source"

cleanup_pids "$SENDER_PID"
finish
