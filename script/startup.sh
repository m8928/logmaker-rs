#!/usr/bin/env sh
# Starts LogMaker as a daemon from the directory containing the `logmaker`
# binary, keeping data, plugins, logs and logmaker.pid next to it.
# Exits non-zero if the server cannot start (e.g. port in use, already running).
# Extra options can be passed in LOGMAKER_OPTS, e.g. LOGMAKER_OPTS="--port 8080".

LOGMAKER_OPTS=${LOGMAKER_OPTS:-}

exec ./logmaker --daemon \
  --pid-file logmaker.pid \
  --data-root ./data \
  --plugin-root ./plugins \
  ${LOGMAKER_OPTS}
