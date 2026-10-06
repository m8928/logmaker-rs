#!/usr/bin/env sh
# Stops the LogMaker daemon started by startup.sh and waits until it exits
# (running logs and scenarios stop and Kafka senders flush first).

PID_FILE=logmaker.pid

if [ ! -f "$PID_FILE" ]; then
  echo "logmaker is not running (no $PID_FILE)"
  exit 0
fi

PID=$(cat "$PID_FILE")
if ! kill -0 "$PID" 2>/dev/null; then
  echo "removing stale $PID_FILE (pid $PID is not running)"
  rm -f "$PID_FILE"
  exit 0
fi

kill -15 "$PID"
i=0
while kill -0 "$PID" 2>/dev/null; do
  i=$((i + 1))
  if [ "$i" -ge 60 ]; then
    echo "logmaker (pid $PID) did not stop within 30 seconds" >&2
    exit 1
  fi
  sleep 0.5
done
rm -f "$PID_FILE"
echo "logmaker stopped (pid $PID)"
