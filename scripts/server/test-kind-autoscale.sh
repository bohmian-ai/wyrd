#!/usr/bin/env bash
# Local Kubernetes autoscaling journey for the official image (kind).
#
# Runnable on demand; not a default CI gate.
# Run it through the example task in deploy/kubernetes/kind/mise.local.toml.example.
#
#   1. One ready peer-mode Oracle-only replica (`wyrd-oracle`) beside the fixed
#      `all` anchor, which owns the only Scribe, on external Postgres, one
#      shared S3-compatible store, and the shared peer mTLS bundle.
#   2. Bounded successful reads (~14/s, at most 8 in flight, at most 240 s)
#      through the Oracle Service raise the first Oracle's own successful
#      `oracle_query_duration_seconds` count above the HPA target of 10/s; the
#      HorizontalPodAutoscaler alone grows the Oracle tier to two.
#   3. The new Oracle registered its Downward-API address in
#      `vala.cluster_nodes`, and a strict fused read through it executes on its
#      own Oracle while fetching the anchor's live Scribe tail over mTLS.
#
# Missing metrics, no scale-up, no join, execution only on the anchor, or a
# peer port that accepts a certificate-less client fails the journey. The kind cluster
# is deleted on success and failure.
set -Eeuo pipefail

readonly root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly manifests="$root/deploy/kubernetes/kind"
readonly cluster="wyrd-autoscale"
readonly ns=wyrd-kind
readonly owner_pw=owner_pw_kind app_pw=app_pw_kind platform_pw=platform_pw_kind
readonly deadline_scale=240 deadline_ready=300
mkdir -p "$root/target"
readonly work="$(mktemp -d "$root/target/wyrd-kind.XXXXXX")"
forwards=()

cleanup() {
  local status=$?
  kill "${forwards[@]}" 2>/dev/null || true
  if [[ $status -ne 0 ]]; then
    local log="$root/target/wyrd-kind-failure.log"
    {
      kubectl -n "$ns" get pods,hpa,jobs -o wide
      kubectl -n "$ns" describe hpa wyrd-oracle
      echo "--- wyrd-core-0"; kubectl -n "$ns" logs wyrd-core-0 --tail=200
      echo "--- wyrd-oracle"; kubectl -n "$ns" logs -l app=wyrd-oracle --prefix --tail=200
    } >"$log" 2>&1 || true
    echo "--- cluster state and pod logs: target/wyrd-kind-failure.log"
  fi
  kind delete cluster --name "$cluster" >/dev/null 2>&1 || true
  rm -rf "$work" "$root/binary"
  exit "$status"
}
trap cleanup EXIT

fail() { echo "FAIL: $*" >&2; exit 1; }
k() { kubectl -n "$ns" "$@"; }
psql_owner() { k exec -i deploy/postgres -- psql -v ON_ERROR_STOP=1 -qAt -U wyrd_owner -d wyrd "$@"; }

# Wait up to $1 seconds for "$2..." to succeed.
until_ok() {
  local seconds=$1; shift
  local end=$((SECONDS + seconds))
  until "$@" >/dev/null 2>&1; do
    ((SECONDS < end)) || return 1
    sleep 2
  done
}

# Port-forward pod $1: HTTP to an ephemeral port, public gRPC to 50051, the
# port the SDK derives from the server URL.
forward() {
  # Reap the previous forward so it has released local port 50051.
  kill "${forwards[@]}" 2>/dev/null || true
  wait "${forwards[@]}" 2>/dev/null || true
  forwards=()
  local log="$work/forward-$1.log"
  # kubectl directly, not the `k` function: a backgrounded function is a
  # subshell, and killing it would orphan the kubectl holding port 50051.
  kubectl -n "$ns" port-forward "pod/$1" :8080 50051:50051 >"$log" 2>&1 &
  forwards+=($!)
  until_ok 30 grep -q '50051 ->' "$log" || fail "port-forward to $1: $(cat "$log")"
  http="http://127.0.0.1:$(sed -n 's/.*127.0.0.1:\([0-9]*\) -> 8080.*/\1/p' "$log" | head -1)"
}

sdk_phase() {
  WYRD_SERVER_URL="$http" WYRD_API_KEY="$api_key" WYRD_STARTUP_STATE_DIR="$work" \
    cargo nextest run --locked -p wyrd-client --test startup_image_journey \
    --run-ignored=only -E "test(=$1)"
}

# Sum of one Prometheus counter's samples on pod $1 whose labels contain $3
# (default: all), read from inside the pod network.
metric_sum() {
  local ip; ip="$(k get pod "$1" -o jsonpath='{.status.podIP}')"
  k exec probe -- curl -fsS "http://$ip:9464/metrics" \
    | awk -v name="$2" -v labels="${3:-}" '($1 == name || index($1, name "{") == 1) && index($1, labels) \
        { sum += $NF } END { printf "%d\n", sum }'
}
oracle_successes() { metric_sum "$1" oracle_query_duration_seconds_count 'outcome="success"'; }
oracle_executions() { metric_sum "$1" oracle_query_duration_seconds_count; }
tail_fences() { metric_sum "$1" bifrost_tail_fences_total 'outcome="acquired"'; }

# Per-pod read rate the HPA consumes, straight from the custom metrics API.
read_rate() {
  kubectl get --raw "/apis/custom.metrics.k8s.io/v1beta1/namespaces/$ns/pods/$1/wyrd_oracle_reads_per_second" \
    | python3 -c 'import json,sys; v=json.load(sys.stdin)["items"][0]["value"]; print(float(v[:-1])/1000 if v.endswith("m") else float(v))'
}

hpa_field() { k get hpa wyrd-oracle -o jsonpath="{$1}"; }
oracle_pods() { k get pods -l app=wyrd-oracle -o jsonpath='{range .items[*]}{.metadata.name}{"\n"}{end}'; }
hpa_desires() { [[ "$(hpa_field .status.desiredReplicas)" == "$1" ]]; }

echo "== build: server binary and the official image"
cargo build --locked -p wyrd-server --features cloud
mkdir -p "$root/binary"
cp "$root/target/debug/wyrd-server" "$root/binary/"
source_commit="$(git -C "$root" rev-parse HEAD)"
[[ -z "$(git -C "$root" status --porcelain --untracked-files=no)" ]] || source_commit+="+dirty"
image_id="$(docker build -q -f "$root/docker/official/Dockerfile" \
  --label "org.opencontainers.image.revision=$source_commit" -t wyrd:kind "$root")"
[[ $image_id == sha256:* ]] || fail "the official build reported no immutable image ID: $image_id"
# The config digest is the image ID containerd reports inside kind, whichever
# image store the host Docker uses.
config_id="sha256:$(docker save wyrd:kind | tar -xO manifest.json | python3 -c \
  'import json,sys; print(json.load(sys.stdin)[0]["Config"].rsplit("/",1)[-1].removesuffix(".json"))')"
echo "image: $image_id config: $config_id source: $source_commit" \
  | tee "$root/target/kind-image-provenance.txt"

echo "== kind cluster and test-local dependencies"
kind delete cluster --name "$cluster" >/dev/null 2>&1 || true
kind create cluster --name "$cluster" --wait 120s
kind load docker-image wyrd:kind --name "$cluster"
node_image_id() {
  docker exec "$cluster-control-plane" crictl inspecti -o go-template --template '{{.status.id}}' "$1"
}
[[ "$(node_image_id wyrd:kind)" == "$config_id" ]] || fail "kind did not load image $config_id"
# A pod reports the loaded image by its repo digest (`import-<date>@sha256:…`),
# which CRI records on the image but does not resolve as a name.
pinned_digest="$(docker exec "$cluster-control-plane" crictl inspecti -o go-template \
  --template '{{range .status.repoDigests}}{{.}}{{end}}' "$config_id")"
[[ $pinned_digest == *@sha256:* ]] || fail "image $config_id has no single repo digest: $pinned_digest"
# Every Wyrd pod must run exactly the pinned image; `imagePullPolicy: Never` alone
# would also accept a stale wyrd:kind left on the node.
require_pinned() {
  local pod ref
  for pod in "$@"; do
    ref="$(k get pod "$pod" -o jsonpath='{.status.containerStatuses[0].imageID}')"
    [[ $ref == "$pinned_digest" ]] || fail "$pod runs $ref, not $config_id ($pinned_digest)"
  done
}
kubectl apply -f "$manifests/infra.yaml" >/dev/null
k rollout status deploy/postgres deploy/rustfs deploy/prometheus deploy/custom-metrics-apiserver --timeout=300s
k run bucket --rm -i --restart=Never --quiet \
  --image=amazon/aws-cli@sha256:c14aafe81d3af04ea4e4ed91c1731308b8195881f774e1aaaa7a5f3d88da81df \
  --env=AWS_ACCESS_KEY_ID=wyrd-kind-key --env=AWS_SECRET_ACCESS_KEY=wyrd-kind-secret \
  --env=AWS_DEFAULT_REGION=us-east-1 \
  -- --endpoint-url http://rustfs.$ns.svc:9000 s3api create-bucket --bucket wyrd-kind >/dev/null

echo "== secrets: serving roles, signing key, and one dedicated peer CA"
psql_owner --set "app_password=$app_pw" --set "platform_admin_password=$platform_pw" \
  <"$root/crates/wyrd/wyrd-sql/bootstrap/roles.sql" >/dev/null
openssl genpkey -algorithm ed25519 -out "$work/signing.pem" 2>/dev/null
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -days 1 \
  -subj "/CN=Wyrd kind peer CA" -keyout "$work/ca.key" -out "$work/ca.crt" \
  -addext basicConstraints=critical,CA:TRUE -addext keyUsage=critical,keyCertSign,cRLSign 2>/dev/null
openssl req -new -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -subj "/CN=wyrd-peer" \
  -keyout "$work/tls.key" -out "$work/tls.csr" 2>/dev/null
openssl x509 -req -in "$work/tls.csr" -CA "$work/ca.crt" -CAkey "$work/ca.key" -CAcreateserial \
  -days 1 -out "$work/tls.crt" -extfile <(printf '%s\n' \
    'subjectAltName=DNS:wyrd-peer' 'keyUsage=critical,digitalSignature' \
    'extendedKeyUsage=serverAuth,clientAuth') 2>/dev/null
# The CA private key never leaves the script's work directory.
k create secret generic wyrd-peer-tls --from-file="$work/ca.crt" --from-file="$work/tls.crt" \
  --from-file="$work/tls.key" >/dev/null
k create secret generic wyrd-owner \
  --from-literal=database-url="postgres://wyrd_owner:$owner_pw@postgres.$ns.svc:5432/wyrd" >/dev/null
k create secret generic wyrd-secrets --from-file=signing.pem="$work/signing.pem" \
  --from-literal=database-url="postgres://wyrd_app:$app_pw@postgres.$ns.svc:5432/wyrd" \
  --from-literal=platform-database-url="postgres://wyrd_platform_admin:$platform_pw@postgres.$ns.svc:5432/wyrd" \
  >/dev/null

echo "== migrate, then one anchor and one autoscaled Oracle"
kubectl apply -f "$manifests/migrate.yaml" >/dev/null
k wait --for=condition=complete job/wyrd-migrate --timeout=300s || fail "migration: $(k logs job/wyrd-migrate)"
kubectl apply -f "$manifests/wyrd.yaml" >/dev/null
k rollout status statefulset/wyrd-core --timeout="${deadline_ready}s"
k rollout status deploy/wyrd-oracle --timeout="${deadline_ready}s"
k wait --for=condition=Ready pod/probe --timeout=120s >/dev/null
oracle0="$(oracle_pods)"
[[ $oracle0 != *$'\n'* && "$(k get deploy wyrd-oracle -o jsonpath='{.status.readyReplicas}')" == 1 ]] \
  || fail "the Oracle tier did not start with exactly one ready replica: $oracle0"
require_pinned wyrd-core-0 "$oracle0"

setup_out="$(k exec wyrd-core-0 -- wyrd-server setup --tenant acme)"
api_key="$(sed -n 's/^admin_credential: //p' <<<"$setup_out")"
[[ -n $api_key ]] || fail "setup disclosed no tenant credential: $setup_out"

echo "== seed through the anchor's Scribe"
forward wyrd-core-0
sdk_phase kind_seed
fqn="$(cat "$work/kind_table")"
token="$(curl -fsS -H 'content-type: application/json' \
  -d "{\"grant_type\":\"wyrd_api_key\",\"api_key\":\"$api_key\"}" "$http/auth/token" \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["access_token"])')"
body="{\"sql\":\"SELECT COUNT(*) FROM $fqn\",\"visibility\":\"fused\",\"freshness\":\"allow_degraded\"}"
k create secret generic wyrd-load --from-literal=token="$token" --from-literal=body="$body" >/dev/null

echo "== below threshold: the metric exists and the HPA holds one replica"
# An Oracle exports its execution count only once it has executed a read, so
# one read through the first Oracle creates the series the adapter rates.
forward "$oracle0"
curl -fsS -o /dev/null -X POST -H "x-wyrd-access-token: Bearer $token" \
  -H 'content-type: application/json' -d "$body" "$http/v1/query" \
  || fail "the first Oracle refused a read"
until_ok 120 read_rate "$oracle0" || fail "custom metric wyrd_oracle_reads_per_second never appeared"
rate="$(read_rate "$oracle0")"
python3 -c "import sys; sys.exit(0 if $rate < 10 else 1)" || fail "baseline read rate $rate is already above 10/s"
until_ok 60 hpa_desires 1 || fail "HPA did not settle at one replica"
[[ "$(hpa_field .status.currentReplicas)" == 1 ]] || fail "HPA does not see exactly one current replica"
echo "baseline rate ${rate}/s, desired replicas 1"

echo "== bounded read traffic through the Oracle Service until the HPA wants two Oracles"
kubectl apply -f "$manifests/load.yaml" >/dev/null
until_ok "$deadline_scale" hpa_desires 2 \
  || fail "HPA never scaled: $(k describe hpa wyrd-oracle)"
scaled_rate="$(hpa_field '.status.currentMetrics[0].pods.current.averageValue')"
echo "HPA desired 2 Oracles at measured average $scaled_rate Oracle-executed reads/s"
k get events --field-selector involvedObject.name=wyrd-oracle,reason=SuccessfulRescale -o name \
  | grep -q . || fail "no SuccessfulRescale event from the HorizontalPodAutoscaler"
new_oracle() { oracle_pods | grep -vx "$oracle0"; }
until_ok 60 new_oracle || fail "the HPA created no second Oracle pod"
oracle1="$(new_oracle)"
until_ok "$deadline_ready" k wait --for=condition=Ready "pod/$oracle1" --timeout=10s \
  || fail "the second Oracle never became ready"
k delete job wyrd-read-load >/dev/null
k wait --for=delete pod -l job-name=wyrd-read-load --timeout=60s >/dev/null 2>&1 || true
require_pinned "$oracle1"

echo "== the new Oracle registered its own injected address"
ip0="$(k get pod "$oracle0" -o jsonpath='{.status.podIP}')"
ip1="$(k get pod "$oracle1" -o jsonpath='{.status.podIP}')"
[[ -n $ip1 && $ip0 != "$ip1" ]] || fail "replicas do not have distinct pod IPs ($ip0, $ip1)"
members() {
  psql_owner -c "SELECT role || ' ' || advertise_addr FROM vala.cluster_nodes \
    WHERE ready AND heartbeat_at > statement_timestamp() - interval '15 seconds' ORDER BY 1"
}
is_member() { members | grep -qx "$1"; }
until_ok 60 is_member "oracle https://$ip1:50052" \
  || fail "the second Oracle's address never registered: $(members)"
is_member "oracle https://$ip0:50052" || fail "the first Oracle left membership: $(members)"
members | grep -q "^scribe https://$ip1:" && fail "the Oracle tier unexpectedly runs a Scribe"
echo "$(members)"

echo "== mTLS peer port: a certificate-less client is refused, the cluster leaf is admitted"
k exec probe -- curl -sS -o /dev/null --max-time 10 --http2 --cacert /etc/wyrd/peer/ca.crt \
  --resolve "wyrd-peer:50052:$ip1" https://wyrd-peer:50052/ 2>/dev/null \
  && fail "the peer port accepted a client without a certificate"
k exec probe -- curl -sS -o /dev/null --max-time 10 --http2 --cacert /etc/wyrd/peer/ca.crt \
  --cert /etc/wyrd/peer/tls.crt --key /etc/wyrd/peer/tls.key \
  --resolve "wyrd-peer:50052:$ip1" https://wyrd-peer:50052/ \
  || fail "the peer port refused the cluster leaf"

echo "== the new Oracle executes a read over the anchor's remote Scribe tail"
# The first Oracle's last load reads have drained once two polls agree.
last_seen=-1
settled() {
  local now; now="$(oracle_executions "$oracle0")"
  [[ $now == "$last_seen" ]] && return 0
  last_seen=$now; return 1
}
until_ok 60 settled || fail "the first Oracle kept executing after the load stopped"
forward wyrd-core-0
sdk_phase kind_append
counts() { echo "$(oracle_executions wyrd-core-0) $(oracle_executions "$oracle0") $(tail_fences wyrd-core-0) $(oracle_successes "$oracle1")"; }
read -r anchor_before first_before fences_before new_before <<<"$(counts)"
forward "$oracle1"
sdk_phase kind_join_read
read -r anchor_after first_after fences_after new_after <<<"$(counts)"
((new_after > new_before)) || fail "the new Oracle executed no successful read ($new_before -> $new_after)"
((anchor_after == anchor_before && first_after == first_before)) \
  || fail "another Oracle executed the read (anchor $anchor_before -> $anchor_after, first $first_before -> $first_after)"
((fences_after > fences_before)) \
  || fail "the new Oracle's read never fenced the anchor's Scribe tail ($fences_before -> $fences_after)"
echo "new Oracle successful reads $new_before -> $new_after; anchor Scribe tail fences $fences_before -> $fences_after; anchor and first Oracle idle"

echo "kind autoscaling journey: PASS (Oracle HPA 1 -> 2 at $scaled_rate reads/s, joined https://$ip1:50052, remote Scribe tail over mTLS, image $image_id from $source_commit)"
