#!/usr/bin/env sh
# Stops the LogMaker instance started by startup.sh. SIGTERM lets running logs
# and scenarios stop and Kafka senders flush before exit.

if [ -f logmaker.pid ]; then
  kill -15 "$(cat logmaker.pid)" && rm -f logmaker.pid
else
  PID=$(pgrep -x logmaker)
  [ -n "$PID" ] && kill -15 $PID
fi
