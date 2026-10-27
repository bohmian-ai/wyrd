# Shared by `test-startup-dev.sh` and `test-startup-prod.sh`: builds the
# official image from this checkout, starts a fresh external Postgres whose
# superuser plays the DBA, and cleans everything up on exit. Each script sets
# `serving_env` before calling `start_app`.
set -Eeuo pipefail

readonly root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly run_id="wyrd-startup-${startup_journey:?}-$$"
readonly tag="$run_id:test"
# Replaced by the immutable image ID the build produces; every container runs by ID.
image="$tag"
readonly net="$run_id" pg="$run_id-pg" app="$run_id-app" volume="$run_id-data" keys="$run_id-keys"
readonly owner_pw=owner_pw_startup
# Under the repository, not /tmp: VM-backed Docker (Colima, Docker Desktop)
# shares only the user's home into the VM, so /tmp bind mounts arrive empty.
mkdir -p "$root/target"
readonly work="$(mktemp -d "$root/target/$run_id.XXXXXX")"
passed=()

# Exits with the status it is given, else the status the EXIT trap fired with.
cleanup() {
  local status=${1:-$?}
  if [[ $status -ne 0 ]]; then
    docker logs "$app" >"$root/target/$run_id-app.log" 2>&1 || true
    echo "--- app logs (full log: target/$run_id-app.log)"; tail -n 80 "$root/target/$run_id-app.log"
  fi
  docker rm -f "$app" "$pg" >/dev/null 2>&1 || true
  docker volume rm -f "$volume" "$keys" >/dev/null 2>&1 || true
  docker network rm "$net" >/dev/null 2>&1 || true
  docker image rm -f "$tag" >/dev/null 2>&1 || true
  rm -rf "$work"
  exit "$status"
}
trap cleanup EXIT

fail() { echo "FAIL: $*" >&2; exit 1; }
# The container superuser plays the DBA; Wyrd never receives its credential.
psql_owner() { docker exec -i "$pg" psql -v ON_ERROR_STOP=1 -qAt -U wyrd_owner -d wyrd "$@"; }

serving_mounts=(-v "$volume:/var/lib/wyrd" -v "$keys:/run/wyrd:ro")

start_app() {
  docker run -d --name "$app" --network "$net" "${serving_env[@]}" "${serving_mounts[@]}" \
    -e "APP_ENV=$1" -p 127.0.0.1::8080 -p 127.0.0.1:50051:50051 "$image" >/dev/null
  [[ "$(docker inspect -f '{{.Image}}' "$app")" == "$image" ]] || fail "app is not running $image"
  http="http://$(docker port "$app" 8080/tcp | head -1)"
  for _ in $(seq 1 180); do
    curl -fsS "$http/readyz" >/dev/null 2>&1 && return 0
    docker inspect -f '{{.State.Running}}' "$app" | grep -q true || fail "app exited during start"
    sleep 1
  done
  fail "app never became ready through nginx: $(curl -sS "$http/readyz" 2>&1)"
}

# The production UI at nginx's public origin, as a browser uses it: the
# authorization server's device page answers there, and API-key recovery
# sign-in with tenant acme's key opens the tenant's page.
ui_sign_in() {
  local key=$1 jar="$work/ui-cookies" answer
  curl -fsS -H 'accept: text/html' "$http/" | grep -i '<html' >/dev/null || fail "UI root through nginx"
  curl -fsS "$http/auth/device?tenant=acme" | grep 'Approve a Wyrd CLI login' >/dev/null \
    || fail "device page at the UI origin through nginx"
  answer="$(curl -sS -o /dev/null -w '%{http_code} %{redirect_url}' -c "$jar" \
    -H "origin: $http" -H 'accept: text/html' --data-urlencode "apiKey=$key" \
    "$http/t/acme/login/api-key")"
  [[ $answer == "303 $http/t/acme" ]] || fail "API-key sign-in through nginx: $answer"
  [[ "$(curl -sS -o /dev/null -w '%{http_code}' -b "$jar" -H 'accept: text/html' "$http/t/acme")" == 200 ]] \
    || fail "signed-in tenant page through nginx"
}

# One child's unsolicited exit ends the container: the BFF is killed and the
# entrypoint must stop wyrd-server and nginx with it.
ui_exit_stops_app() {
  # The bracket keeps this probe's own command lines from matching.
  docker exec "$app" sh -c 'for p in /proc/[0-9]*; do
    tr "\0" " " <"$p/cmdline" 2>/dev/null | grep -q "ui/build/[i]ndex.js" && kill "${p#/proc/}"
  done; true'
  for _ in $(seq 1 60); do
    docker inspect -f '{{.State.Running}}' "$app" | grep -q false && return 0
    sleep 1
  done
  fail "the container kept running after its UI exited"
}

echo "== build: server binary and the official image"
image="$("$root/scripts/build.sh" image debian "$tag")"
[[ $image == sha256:* ]] || fail "the official build reported no immutable image ID: $image"
source_commit="$(docker inspect -f '{{index .Config.Labels "org.opencontainers.image.revision"}}' "$image")"
echo "image: $image source: $source_commit" | tee "$root/target/startup-$startup_journey-image-provenance.txt"
# The server reads only owner-only key files, so the signing key and the
# single-tenant deployment's file operator KEK are written into a volume as
# the image's serving user, the way mounted secrets are delivered.
docker volume create "$keys" >/dev/null
put_key() {
  docker run --rm -i -u root -v "$keys:/run/wyrd" "$image" sh -c \
    "mkdir -p /run/wyrd/kek && cat >/run/wyrd/$1 && chown -R wyrd:wyrd /run/wyrd && chmod 0600 /run/wyrd/$1"
}
openssl genpkey -algorithm ed25519 2>/dev/null | put_key signing.pem
openssl rand -base64 32 | tr -d '\n' | put_key kek/v1

echo "== fresh external Postgres"
docker network create "$net" >/dev/null
docker volume create "$volume" >/dev/null
docker run -d --name "$pg" --network "$net" -e POSTGRES_DB=wyrd -e POSTGRES_USER=wyrd_owner \
  -e "POSTGRES_PASSWORD=$owner_pw" --tmpfs /var/lib/postgresql/data postgres:16 >/dev/null
for _ in $(seq 1 60); do
  docker exec "$pg" pg_isready -h 127.0.0.1 -U wyrd_owner -d wyrd >/dev/null 2>&1 && break
  sleep 1
done

