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

cleanup() {
  local status=$?
  if [[ $status -ne 0 ]]; then
    docker logs "$app" >"$root/target/$run_id-app.log" 2>&1 || true
    echo "--- app logs (full log: target/$run_id-app.log)"; tail -n 80 "$root/target/$run_id-app.log"
  fi
  docker rm -f "$app" "$pg" >/dev/null 2>&1 || true
  docker volume rm -f "$volume" "$keys" >/dev/null 2>&1 || true
  docker network rm "$net" >/dev/null 2>&1 || true
  docker image rm -f "$tag" >/dev/null 2>&1 || true
  rm -rf "$work" "$root/binary"
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

echo "== build: server binary and the official image"
cargo build --locked -p wyrd-server --features cloud
mkdir -p "$root/binary"
cp "$root/target/debug/wyrd-server" "$root/binary/"
source_commit="$(git -C "$root" rev-parse HEAD)"
[[ -z "$(git -C "$root" status --porcelain --untracked-files=no)" ]] || source_commit+="+dirty"
image="$(docker build -q -f "$root/docker/official/Dockerfile" \
  --label "org.opencontainers.image.revision=$source_commit" -t "$tag" "$root")"
[[ $image == sha256:* ]] || fail "the official build reported no immutable image ID: $image"
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

