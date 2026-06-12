#!/bin/bash
# Scenario F: 100 GB sparse-file-equivalent transfer with bounded memory.
# Creates a 100GB sparse source (random 1MB block every 4GB, rest holes),
# transfers it desktop-process to desktop-process, monitors max RSS of both
# processes, verifies completion + export, then cleans up the ~100GB of
# receiver-side data.
#
# Requires ~110GB free on the volume holding $SPIKE_WORK.
source "$(dirname "$0")/lib.sh"

GB=$((1024 * 1024 * 1024))
SRC_SIZE=$((100 * GB))
FILE="$WORK/src-100gb-sparse.bin"
RSS_LIMIT_KB=$((2 * 1024 * 1024)) # 2 GB

AVAIL_KB=$(df -k "$WORK" | tail -1 | awk '{print $4}')
if [ "$AVAIL_KB" -lt $((115 * 1024 * 1024)) ]; then
  fail "need ~115GB free on $WORK volume, have $((AVAIL_KB / 1024 / 1024))GB"
  finish
fi

if [ ! -f "$FILE" ]; then
  echo "creating 100GB sparse source…"
  dd if=/dev/zero of="$FILE" bs=1 count=0 seek="$SRC_SIZE" 2>/dev/null
  for i in $(seq 0 4 96); do
    dd if=/dev/urandom of="$FILE" bs=1m count=1 seek=$((i * 1024)) conv=notrunc 2>/dev/null
  done
fi
ACTUAL_BLOCKS_KB=$(du -k "$FILE" | cut -f1)
echo "sparse source: logical $(stat -f%z "$FILE") bytes, physical ${ACTUAL_BLOCKS_KB}KB"

rm -rf "$WORK/send-f" "$WORK/recv-f" "$WORK/dest-f.bin"
echo "importing (hashes all 100GB)…"
# SENDER_RSS_FILE makes start_sender attach the RSS monitor at spawn, so the
# measurement covers the 100GB import/hashing phase, not just serving.
SENDER_RSS_FILE="$WORK/send-f.rss" start_sender f "$FILE" || { fail "sender startup"; finish; }
SEND_RSS_MON=$SENDER_RSS_MON
assert_eq "ticket size is 100GB" "$SIZE" "$SRC_SIZE"

START_TS=$(date +%s)
"$BIN" recv --store "$WORK/recv-f" --ticket "$TICKET" --dest "$WORK/dest-f.bin" \
  --retries 5 --retry-delay-secs 2 \
  > "$WORK/recv-f.out" 2> "$WORK/recv-f.err" &
RECV_PID=$!
monitor_rss "$RECV_PID" > "$WORK/recv-f.rss" &
RECV_RSS_MON=$!

wait "$RECV_PID"; RECV_RC=$?
END_TS=$(date +%s)
kill "$SENDER_PID" 2>/dev/null
wait "$SEND_RSS_MON" "$RECV_RSS_MON" 2>/dev/null
SEND_MAX_RSS=$(cat "$WORK/send-f.rss")
RECV_MAX_RSS=$(cat "$WORK/recv-f.rss")

assert_eq "receiver exit code" "$RECV_RC" "0"
grep -q "^COMPLETE: $SRC_SIZE" "$WORK/recv-f.out" && pass "100GB blob complete and verified" || fail "no COMPLETE marker for $SRC_SIZE"
grep -q "^EXPORTED: " "$WORK/recv-f.out" && pass "exported to destination" || fail "no export"
DEST_SIZE=$(stat -f%z "$WORK/dest-f.bin" 2>/dev/null || echo 0)
assert_eq "destination size" "$DEST_SIZE" "$SRC_SIZE"
grep "^CONNECTION: " "$WORK/recv-f.out" | tail -1

assert_lt "sender max RSS bounded (KB)" "$SEND_MAX_RSS" "$RSS_LIMIT_KB"
assert_lt "receiver max RSS bounded (KB)" "$RECV_MAX_RSS" "$RSS_LIMIT_KB"
echo "sender max RSS: $((SEND_MAX_RSS / 1024))MB, receiver max RSS: $((RECV_MAX_RSS / 1024))MB"
echo "transfer wall time: $((END_TS - START_TS))s"

# Spot-check Content Identity: compare the random blocks and one hole region.
SPOT_FAIL=0
for i in 0 48 96; do
  off=$((i * 1024))
  cmp <(dd if="$FILE" bs=1m skip=$off count=1 2>/dev/null) \
      <(dd if="$WORK/dest-f.bin" bs=1m skip=$off count=1 2>/dev/null) || SPOT_FAIL=1
done
cmp <(dd if="$FILE" bs=1m skip=2048 count=4 2>/dev/null) \
    <(dd if="$WORK/dest-f.bin" bs=1m skip=2048 count=4 2>/dev/null) || SPOT_FAIL=1
assert_eq "spot-check random blocks + hole region" "$SPOT_FAIL" "0"
echo "note: full byte identity is already guaranteed by BLAKE3 verified streaming;"
echo "      the export came from a store that verified every 16KiB chunk."

echo "cleaning up ~200GB of scenario data…"
rm -rf "$WORK/recv-f" "$WORK/dest-f.bin" "$WORK/send-f"
finish
