#!/bin/bash
# Scenario D: manual pause/resume on the Receiver via stdin commands.
# Pause mid-transfer, verify progress stops and partial state is preserved,
# resume, and verify completion + byte identity.
source "$(dirname "$0")/lib.sh"

FILE="$WORK/src-5gb.bin"
[ -f "$FILE" ] || { echo "generating 5GB random source…"; dd if=/dev/urandom of="$FILE" bs=1m count=5120 2>/dev/null; }
SRC_SIZE=$(stat -f%z "$FILE")

rm -rf "$WORK/send-d" "$WORK/recv-d" "$WORK/dest-d.bin"
start_sender d "$FILE" || { fail "sender startup"; finish; }

CMD_PIPE="$WORK/recv-d.cmds"
rm -f "$CMD_PIPE"; mkfifo "$CMD_PIPE"
"$BIN" recv --store "$WORK/recv-d" --ticket "$TICKET" --dest "$WORK/dest-d.bin" \
  --interactive < "$CMD_PIPE" \
  > "$WORK/recv-d.out" 2> "$WORK/recv-d.err" &
RECV_PID=$!
exec 9> "$CMD_PIPE"  # hold the pipe open

PAUSE_AT=$((SRC_SIZE / 4))
if wait_for_progress "$WORK/recv-d.out" "$PAUSE_AT" 300; then
  echo "pause" >&9
  pass "pause requested past $PAUSE_AT bytes"
else
  fail "never reached pause threshold"; cleanup_pids "$RECV_PID" "$SENDER_PID"; finish
fi

# Wait for the PAUSED marker, then verify no further progress while paused.
for _ in $(seq 1 60); do grep -q "^PAUSED: " "$WORK/recv-d.out" && break; sleep 0.5; done
grep -q "^PAUSED: " "$WORK/recv-d.out" && pass "receiver reported PAUSED" || fail "no PAUSED marker"
P1=$(grep "^PAUSED: " "$WORK/recv-d.out" | tail -1 | cut -d' ' -f2)
sleep 5
LINES_BEFORE=$(grep -c "^PROGRESS: " "$WORK/recv-d.out")
sleep 5
LINES_AFTER=$(grep -c "^PROGRESS: " "$WORK/recv-d.out")
assert_eq "no progress while paused" "$LINES_AFTER" "$LINES_BEFORE"
assert_gt "paused with partial bytes preserved" "$P1" 0
[ -f "$WORK/dest-d.bin" ] && fail "dest exists while paused" || pass "no file at destination while paused"

echo "resume" >&9
for _ in $(seq 1 600); do kill -0 "$RECV_PID" 2>/dev/null || break; sleep 1; done
exec 9>&-
if kill -0 "$RECV_PID" 2>/dev/null; then
  fail "receiver did not finish after resume"; cleanup_pids "$RECV_PID" "$SENDER_PID"; finish
fi
wait "$RECV_PID"; RECV_RC=$?

assert_eq "receiver exit code" "$RECV_RC" "0"
grep -q "^RESUMED: 1" "$WORK/recv-d.out" && pass "receiver reported RESUMED" || fail "no RESUMED marker"
grep -q "^EXPORTED: " "$WORK/recv-d.out" && pass "exported after resume" || fail "no export"
cmp "$FILE" "$WORK/dest-d.bin" && pass "export byte-identical to source" || fail "export differs"

cleanup_pids "$SENDER_PID"
finish
