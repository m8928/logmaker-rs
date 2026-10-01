#!/usr/bin/env sh
# Starts LogMaker in the background from the directory containing the
# `logmaker` binary, keeping data and plugins next to it.
# Extra options can be passed in LOGMAKER_OPTS, e.g. LOGMAKER_OPTS="--port 8080".

LOGMAKER_OPTS=${LOGMAKER_OPTS:-}

nohup ./logmaker \
  --data-root ./data \
  --plugin-root ./plugins \
  ${LOGMAKER_OPTS} >/dev/null 2>&1 &

echo $! > logmaker.pid
