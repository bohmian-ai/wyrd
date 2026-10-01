#!/bin/bash
# Run wyrd-server, the Node BFF, and nginx; exit when any of them exits.
#
# `wyrd-server migrate` and `wyrd-server setup` run as one-off commands against
# the same image (`docker run ... wyrd-server migrate`), never from here.
set -Eeuo pipefail

wait_for_port() {
  local port=$1 name=$2
  for _ in $(seq 1 120); do
    if (exec 3<>"/dev/tcp/127.0.0.1/${port}") 2>/dev/null; then return 0; fi
    kill -0 "$3" 2>/dev/null || { echo "$name exited during startup" >&2; exit 1; }
    sleep 0.5
  done
  echo "$name did not listen on ${port}" >&2
  exit 1
}

# A fresh volume at /var/lib/wyrd (a Kubernetes PVC; only Docker copies the
# image's directories into a new named volume) lacks the local storage root.
mkdir -p /var/lib/wyrd/storage

pids=()
trap 'kill -TERM "${pids[@]}" 2>/dev/null; wait' TERM INT

/usr/local/bin/wyrd-server &
pids+=($!)
wait_for_port "${WYRD_SERVER_BIND##*:}" wyrd-server "${pids[0]}"

# The BFF reaches Rust through nginx's public listener, its default server URL.
(cd /app/ui && HOST=127.0.0.1 PORT=3000 NODE_ENV=production \
  exec node build/index.js) &
pids+=($!)
wait_for_port 3000 ui "${pids[1]}"

envsubst '${WYRD_SERVER_BIND} ${WYRD_GRPC_BIND}' \
  </etc/nginx/nginx.conf.template >/etc/nginx/nginx.conf
nginx -g 'daemon off;' &
pids+=($!)

set +e
wait -n "${pids[@]}"
status=$?
kill -TERM "${pids[@]}" 2>/dev/null
wait
exit "$status"
