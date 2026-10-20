#!/usr/bin/env bash
# Own one Keycloak and Dex lifecycle for the invoking command: recreate both
# providers, seed the Keycloak users the identity journeys sign in as, run the
# command, and remove the providers however it ends.
set -Eeuo pipefail

if [[ $# -lt 2 || $1 != "--" ]]; then
  echo "usage: $0 -- command [args ...]" >&2
  exit 64
fi
shift

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

# Bring up Keycloak and Dex (Postgres is owned by the caller's lifecycle). Recreate them:
# Keycloak start-dev keeps its realm in an embedded H2 database inside the
# container filesystem, and H2's lock file names the container hostname that
# opened it. A run killed mid-flight leaves that lock behind, so reusing the
# stopped container made Keycloak resolve a hostname that no longer exists and
# exit before serving. A fresh container each run has no stale lock to inherit.
docker compose rm --force --stop --volumes keycloak dex
# Removed on every exit, including a failed seed, a failed suite, and an
# interrupt, so the next run never inherits a container this one left behind.
trap 'docker compose rm --force --stop --volumes keycloak dex' EXIT
docker compose up -d keycloak dex --wait

# Seed Keycloak users via admin REST API (realm import only defines clients/groups)
uv run python - <<'PYEOF'
import json, urllib.request, urllib.parse, time, sys

BASE = "http://localhost:8180"
REALM = "wyrd-test"

def get_admin_token():
    data = urllib.parse.urlencode({
        "grant_type": "password",
        "client_id": "admin-cli",
        "username": "admin",
        "password": "admin",
    }).encode()
    req = urllib.request.Request(
        f"{BASE}/realms/master/protocol/openid-connect/token", data=data
    )
    resp = json.loads(urllib.request.urlopen(req).read())
    return resp["access_token"]

def api(method, path, body=None, token=None, realm=REALM):
    url = f"{BASE}/admin/realms/{realm}{path}"
    data = json.dumps(body).encode() if body else None
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        r = urllib.request.urlopen(req)
        raw = r.read()
        return json.loads(raw) if raw else None
    except urllib.error.HTTPError as e:
        body = e.read()
        if e.code == 409:  # already exists — idempotent
            return None
        raise RuntimeError(f"API {method} {path} → {e.code}: {body}") from e

for attempt in range(30):
    try:
        token = get_admin_token()
        break
    except Exception:
        time.sleep(2)
else:
    sys.exit("Keycloak admin API not ready after 60s")

def create_user(username, email, password, group_path, realm=REALM):
    api("POST", "/users", {
        "username": username,
        "email": email,
        "firstName": username.capitalize(),
        "lastName": "Wyrd",
        "emailVerified": True,
        "enabled": True,
        "requiredActions": [],
    }, token, realm)
    users = api("GET", f"/users?username={username}", token=token, realm=realm) or []
    if not users:
        sys.exit(f"user {username} not found after creation")
    uid = users[0]["id"]
    api("PUT", f"/users/{uid}/reset-password", {
        "type": "password",
        "temporary": False,
        "value": password,
    }, token, realm)
    groups = api("GET", f"/groups?search={group_path.lstrip('/')}", token=token, realm=realm) or []
    if groups:
        api("PUT", f"/users/{uid}/groups/{groups[0]['id']}", token=token, realm=realm)

create_user("alice", "alice@wyrd.test", "alice-password", "wyrd-admins")
create_user("bob", "bob@wyrd.test", "wyrd-test", "wyrd-viewers")
create_user("carol", "carol@wyrd.test", "carol-password", "wyrd-editors")
# The second provider's alice shares the first's email: a provider switch must
# still make her a separate Wyrd user.
create_user("alice", "alice@wyrd.test", "alice-password", "wyrd-admins", realm="wyrd-test-2")
print("Keycloak users seeded")
PYEOF

"$@"
