#!/bin/bash
# Scenario C: temporary network loss + Sender app restart.
# SIGKILL the sender mid-transfer (receiver sees a network error), restart the
# sender from the same spike store (same secret key => same EndpointId => the
# original ticket stays valid), and verify the receiver's retry loop resumes
# and completes.
source "$(dirname "$0")/lib.sh"

FILE="$WORK/src-5gb.bin"
[ -f "$FILE" ] || { echo "generating 5GB random source…"; dd if=/dev/urandom of="$FILE" bs=1m count=5120 2>/dev/null; }
SRC_SIZE=$(stat -f%z "$FILE")

rm -rf "$WORK/send-c" "$WORK/recv-c" "$WORK/dest-c.bin"
start_sender c "$FILE" || { fail "sender startup"; finish; }
TICKET1="$TICKET"

"$BIN" recv --store "$WORK/recv-c" --ticket "$TICKET1" --dest "$WORK/dest-c.bin" \
  --retries 30 --retry-delay-secs 2 \
  > "$WORK/recv-c.out" 2> "$WORK/recv-c.err" &
RECV_PID=$!

KILL_AT=$((SRC_SIZE / 3))
if wait_for_progress "$WORK/recv-c.out" "$KILL_AT" 300; then
  kill -9 "$SENDER_PID"
  pass "sender SIGKILLed past $KILL_AT bytes (simulates network loss + sender crash)"
else
  fail "receiver never reached kill threshold"
  cleanup_pids "$RECV_PID" "$SENDER_PID"; finish
fi
wait "$SENDER_PID" 2>/dev/null

sleep 5  # let the receiver notice and start retrying

# Restart the sender from the SAME store dir: same key, same blob.
start_sender c "$FILE" || { fail "sender restart"; cleanup_pids "$RECV_PID"; finish; }
TICKET2="$TICKET"
T1_ID=$(grep "^ENDPOINT_ID: " "$WORK/send-c.out" | tail -1 | cut -d' ' -f2)
pass "sender restarted with endpoint id $T1_ID"

# The receiver should resume against the original ticket and finish.
for _ in $(seq 1 600); do
  kill -0 "$RECV_PID" 2>/dev/null || break
  sleep 1
done
if kill -0 "$RECV_PID" 2>/dev/null; then
  fail "receiver did not finish after sender restart"
  cleanup_pids "$RECV_PID" "$SENDER_PID"; finish
fi
wait "$RECV_PID"; RECV_RC=$?

assert_eq "receiver exit code" "$RECV_RC" "0"
grep -q "^RETRYING: " "$WORK/recv-c.out" && pass "receiver observed the outage and retried" || fail "no retry observed"
grep -q "^EXPORTED: " "$WORK/recv-c.out" && pass "exported after resume" || fail "no export"
# Sessions that end in an error emit no SESSION/Stats line, so TOTAL counts
# only the successful final session. Bind it to the measured resume point:
# the final session must fetch (size - resume_point) bytes, within 5% slack.
RESUME=$(resume_point "$WORK/recv-c.out")
TOTAL=$(grep "^TOTAL_SESSION_PAYLOAD_BYTES: " "$WORK/recv-c.out" | cut -d' ' -f2)
assert_gt "resume point past the kill threshold" "$RESUME" 0
assert_lt "final session fetched only the missing bytes" "$TOTAL" "$((SRC_SIZE - RESUME + SRC_SIZE / 20))"
cmp "$FILE" "$WORK/dest-c.bin" && pass "export byte-identical to source" || fail "export differs"

cleanup_pids "$SENDER_PID"
finish
