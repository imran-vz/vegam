#!/bin/bash
# Scenario A: sanity — 100MB random file, single session, byte identity,
# Direct/Relayed reporting.
source "$(dirname "$0")/lib.sh"

FILE="$WORK/src-100mb.bin"
[ -f "$FILE" ] || dd if=/dev/urandom of="$FILE" bs=1m count=100 2>/dev/null

rm -rf "$WORK/send-a" "$WORK/recv-a" "$WORK/dest-100mb.bin"
start_sender a "$FILE" || { fail "sender startup"; finish; }

"$BIN" recv --store "$WORK/recv-a" --ticket "$TICKET" --dest "$WORK/dest-100mb.bin" \
  > "$WORK/recv-a.out" 2> "$WORK/recv-a.err"
RECV_RC=$?

assert_eq "receiver exit code" "$RECV_RC" "0"
grep -q "^EXPORTED: " "$WORK/recv-a.out" && pass "exported" || fail "no export"
cmp "$FILE" "$WORK/dest-100mb.bin" && pass "byte-identical" || fail "content differs"
grep -qE "^CONNECTION: (Direct|Relayed)" "$WORK/recv-a.out" && pass "connection path reported" || fail "no CONNECTION line"
grep "^CONNECTION: " "$WORK/recv-a.out" | sort -u

cleanup_pids "$SENDER_PID"
finish
