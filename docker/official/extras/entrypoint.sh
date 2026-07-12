#!/bin/bash
set -e

exec 2>&1

export WYRD_SERVER_PORT=${WYRD_SERVER_PORT:-8080}
export NGINX_PORT=${NGINX_PORT:-8000}

if ! [[ "$NGINX_PORT" =~ ^[0-9]+$ ]]; then
    echo "$(date): ERROR: NGINX_PORT must be numeric, got: $NGINX_PORT"
    exit 1
fi

WYRD_API_PID=""
SVELTEKIT_PID=""
NGINX_PID=""

cleanup() {
    echo "$(date): Received shutdown signal, cleaning up..."

    if [[ -n "$NGINX_PID" ]]; then
        echo "$(date): Stopping nginx..."
        kill -QUIT "$NGINX_PID" 2>/dev/null || true
    fi

    if [[ -n "$SVELTEKIT_PID" ]]; then
        echo "$(date): Stopping SvelteKit..."
        kill -TERM "$SVELTEKIT_PID" 2>/dev/null || true
    fi

    if [[ -n "$WYRD_API_PID" ]]; then
        echo "$(date): Stopping wyrd-server..."
        kill -TERM "$WYRD_API_PID" 2>/dev/null || true
    fi

    sleep 5

    for pid in "$NGINX_PID" "$SVELTEKIT_PID" "$WYRD_API_PID"; do
        if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
            echo "$(date): Force killing $pid"
            kill -KILL "$pid" 2>/dev/null || true
        fi
    done

    echo "$(date): Cleanup complete"
    exit 0
}

trap cleanup SIGTERM SIGINT

wait_for_service() {
    local port=$1
    local name=$2
    local max=30
    local i=1

    echo "$(date): Waiting for $name on port $port..."
    while [ $i -le $max ]; do
        if nc -z localhost "$port" 2>/dev/null; then
            echo "$(date): $name ready on port $port"
            return 0
        fi
        [ $((i % 5)) -eq 0 ] && echo "$(date): Still waiting for $name (attempt $i/$max)"
        sleep 2
        i=$((i + 1))
    done
    echo "$(date): ERROR: $name failed to start on port $port"
    return 1
}

echo "$(date): Configuring nginx..."
if ! envsubst '${NGINX_PORT}' < /etc/nginx/nginx.conf.template > /etc/nginx/nginx.conf; then
    echo "$(date): ERROR: Failed to process nginx template"
    exit 1
fi

echo "$(date): Starting wyrd-server on port ${WYRD_SERVER_PORT}..."
/usr/local/bin/wyrd-server &
WYRD_API_PID=$!

if ! wait_for_service "${WYRD_SERVER_PORT}" "wyrd-server"; then
    echo "$(date): Failed to start wyrd-server"
    exit 1
fi

echo "$(date): Starting SvelteKit on port 3000..."
cd /app/ui
PORT=3000 NODE_ENV=production node build/index.js &
SVELTEKIT_PID=$!

if ! wait_for_service 3000 "SvelteKit"; then
    echo "$(date): Failed to start SvelteKit"
    exit 1
fi

echo "$(date): Testing nginx configuration..."
if ! nginx -t; then
    echo "$(date): nginx configuration test failed"
    exit 1
fi

echo "$(date): Starting nginx on port ${NGINX_PORT}..."
nginx -g "daemon off;" &
NGINX_PID=$!

if ! wait_for_service "${NGINX_PORT}" "nginx"; then
    echo "$(date): Failed to start nginx"
    exit 1
fi

echo "$(date): All services started (wyrd-server=$WYRD_API_PID, SvelteKit=$SVELTEKIT_PID, nginx=$NGINX_PID)"

while true; do
    for entry in "WYRD_API_PID:wyrd-server" "SVELTEKIT_PID:SvelteKit" "NGINX_PID:nginx"; do
        var="${entry%%:*}"
        name="${entry##*:}"
        pid=$(eval echo "\$$var")
        if [[ -n "$pid" ]] && ! kill -0 "$pid" 2>/dev/null; then
            echo "$(date): ERROR: $name (PID $pid) died unexpectedly"
            cleanup
            exit 1
        fi
    done
    sleep 10
done
