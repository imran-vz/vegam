#!/bin/bash
# Scenario G: temporary network loss with the Sender still alive.
# SIGSTOP the sender mid-transfer (the QUIC peer goes silent, like a dropped
# link), let the receiver error and retry, SIGCONT the sender, and verify the
# receiver resumes and completes without a full re-download.
source "$(dirname "$0")/lib.sh"

FILE="$WORK/src-5gb.bin"
[ -f "$FILE" ] || { echo "generating 5GB random source…"; dd if=/dev/urandom of="$FILE" bs=1m count=5120 2>/dev/null; }
SRC_SIZE=$(stat -f%z "$FILE")

rm -rf "$WORK/send-g" "$WORK/recv-g" "$WORK/dest-g.bin"
start_sender g "$FILE" || { fail "sender startup"; finish; }

"$BIN" recv --store "$WORK/recv-g" --ticket "$TICKET" --dest "$WORK/dest-g.bin" \
  --retries 60 --retry-delay-secs 2 \
  > "$WORK/recv-g.out" 2> "$WORK/recv-g.err" &
RECV_PID=$!

STALL_AT=$((SRC_SIZE / 3))
if wait_for_progress "$WORK/recv-g.out" "$STALL_AT" 300; then
  kill -STOP "$SENDER_PID"
  pass "sender SIGSTOPped past $STALL_AT bytes (link goes silent)"
else
  fail "never reached stall threshold"; cleanup_pids "$RECV_PID" "$SENDER_PID"; finish
fi

# Wait until the receiver notices the stall and starts retrying.
STALLED_OK=0
for _ in $(seq 1 240); do
  grep -q "^RETRYING: " "$WORK/recv-g.out" && { STALLED_OK=1; break; }
  kill -0 "$RECV_PID" 2>/dev/null || break
  sleep 1
done
[ "$STALLED_OK" -eq 1 ] && pass "receiver detected the stall and retried" || fail "receiver never reported RETRYING"

kill -CONT "$SENDER_PID"
pass "sender SIGCONTed (link restored)"

for _ in $(seq 1 600); do kill -0 "$RECV_PID" 2>/dev/null || break; sleep 1; done
if kill -0 "$RECV_PID" 2>/dev/null; then
  fail "receiver did not finish after link restore"; cleanup_pids "$RECV_PID" "$SENDER_PID"; finish
fi
wait "$RECV_PID"; RECV_RC=$?

assert_eq "receiver exit code" "$RECV_RC" "0"
# Bind to the measured resume point: the final successful session must fetch
# only (size - resume_point) bytes within 5% slack — a full re-download from
# zero cannot pass this since the stall fired past 1/3 of the file.
RESUME=$(resume_point "$WORK/recv-g.out")
TOTAL=$(grep "^TOTAL_SESSION_PAYLOAD_BYTES: " "$WORK/recv-g.out" | cut -d' ' -f2)
assert_gt "resume point past the stall threshold" "$RESUME" 0
assert_lt "final session fetched only the missing bytes" "$TOTAL" "$((SRC_SIZE - RESUME + SRC_SIZE / 20))"
grep -q "^EXPORTED: " "$WORK/recv-g.out" && pass "exported after stall recovery" || fail "no export"
cmp "$FILE" "$WORK/dest-g.bin" && pass "export byte-identical to source" || fail "export differs"

cleanup_pids "$SENDER_PID"
finish
