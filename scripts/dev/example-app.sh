#!/usr/bin/env bash
# Prepare and run the canonical wyrd-example-py application for `dev:full`.
#
#   example-app.sh prepare   pinned checkout + this checkout's production wheel
#   example-app.sh setup     gateway deployment to MockServer, Cards, capture policy, the Cards board's fixtures
#   example-app.sh serve     the API on 127.0.0.1:${WYRD_EXAMPLE_PORT:-8000}
#   example-app.sh batch ARGS   one bounded workload batch (scripts/batch.py)
#   example-app.sh test ARGS    the app's pytest suite with the testing-enabled wheel,
#                               in an environment kept apart from the development one
#
# The checkout at target/dev/wyrd-example-py is the pinned upstream commit
# (wyrd-example-py's dev-workload branch). A checkout holding uncommitted
# changes is refused, never overwritten. The wheel is built from this repository
# without the testing feature, is the only file in the app's vendor index, and
# the installed native module must match its digest.
set -Eeuo pipefail

readonly root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly repo=https://github.com/bohmian-ai/wyrd-example-py
readonly base=ee83b2559fb66a89b673786d3bcbe7e65cd26c17
readonly app="$root/target/dev/wyrd-example-py"
readonly port=${WYRD_EXAMPLE_PORT:-8000}

# The tree id of the checkout's current files, honoring its .gitignore.
worktree_tree() {
  local index
  index=$(mktemp -u)
  GIT_INDEX_FILE=$index git -C "$app" add -A
  GIT_INDEX_FILE=$index git -C "$app" write-tree
  rm -f "$index"
}

checkout() {
  if [[ ! -d $app/.git ]]; then
    git clone --quiet "$repo" "$app"
  fi
  git -C "$app" cat-file -e "$base^{commit}" 2>/dev/null || git -C "$app" fetch --quiet origin "$base"
  local current
  current=$(worktree_tree)
  # Files identical to the pin (such as an old base-plus-patch checkout) only move HEAD.
  if [[ $current == "$(git -C "$app" rev-parse "$base^{tree}")" ]]; then
    git -C "$app" -c advice.detachedHead=false checkout --quiet --force "$base"
    return
  fi
  if [[ $current != "$(git -C "$app" rev-parse HEAD^{tree})" ]]; then
    echo "$app has uncommitted changes; move them aside and rerun" >&2
    exit 1
  fi
  git -C "$app" -c advice.detachedHead=false checkout --quiet "$base"
}

wheel() {
  rm -rf "$app/vendor"
  mkdir -p "$app/vendor"
  (cd "$root/sdks/wyrd-sdk-python" && uv run --quiet maturin build --quiet --out "$app/vendor")
  local built
  built=$(ls "$app"/vendor/*.whl)
  # The lock names the upstream author's vendor path and wheel, so everything
  # else installs frozen and this wheel is installed by its own path.
  (cd "$app" && uv sync --quiet --frozen --no-install-package wyrd-ai \
    && uv pip install --quiet --no-deps --reinstall "$built")
  local installed
  installed=$(cd "$app" && uv run --no-sync python -c 'import wyrd._wyrd as m; print(m.__file__)')
  if [[ $(unzip -p "$built" "wyrd/$(basename "$installed")" | sha256sum | cut -d' ' -f1) != $(sha256sum "$installed" | cut -d' ' -f1) ]]; then
    echo "the app imports $installed, not the native module of $built" >&2
    exit 1
  fi
  if (cd "$app" && uv run --no-sync python -c 'import wyrd.testing' 2>/dev/null); then
    echo "the app imports a testing-enabled Wyrd SDK" >&2
    exit 1
  fi
  printf 'wyrd-example-py %s, source tree %s, wheel %s (sha256 %s)\n' \
    "$base" "$(worktree_tree)" \
    "$(basename "$built")" "$(sha256sum "$built" | cut -c1-12)"
}

# Point the tenant's `openai` deployment at MockServer with a placeholder key,
# then declare the Cards and the capture policy as the app's deploy script does.
setup() {
  cd "$app"
  OPENAI_API_KEY=sk-mockserver uv run --no-sync wyrd gateway deployment put --file - <<'JSON'
{"credentials": [{"name": "openai-key", "provider": "openai", "source": {"local_env": "OPENAI_API_KEY"}}],
 "deployments": [{"name": "openai", "adapter": "openai", "auth": {"bearer": {"credential": "openai-key"}}}]}
JSON
  uv run --no-sync python -m scripts.deploy
  # The Cards board's representative registry: a Model with a file, two
  # versions of one Prompt and the Agent using the newer one.
  local card
  for card in declare_and_pull/support-model.yaml ui_cards/support-reply-1.0.0.yaml \
    ui_cards/support-reply-1.1.0.yaml ui_cards/reply-agent.yaml; do
    uv run --no-sync wyrd declare "$root/fixtures/cards/$card" >/dev/null
  done
}

# The app's tests start a WyrdTestServer, which only the testing wheel has.
# It lives outside the checkout and never replaces the production install.
test_env() {
  local wheels=$root/target/dev/testing-wheel
  rm -rf "$wheels"
  (cd "$root/sdks/wyrd-sdk-python" && uv run --quiet maturin build --quiet --features testing --out "$wheels")
  export UV_PROJECT_ENVIRONMENT=$root/target/dev/wyrd-example-py-test-venv
  (cd "$app" && uv sync --quiet --frozen --no-install-package wyrd-ai \
    && uv pip install --quiet --no-deps --reinstall --python "$UV_PROJECT_ENVIRONMENT/bin/python" "$wheels"/*.whl)
}

case ${1:-} in
  prepare) checkout && wheel ;;
  test)
    shift
    checkout
    test_env
    cd "$app"
    exec uv run --no-sync python -m pytest "$@" ;;
  setup) setup ;;
  serve)
    cd "$app"
    # Export metrics every 5 s instead of every 60 s so a short batch sees them.
    exec env OTEL_METRIC_EXPORT_INTERVAL=${OTEL_METRIC_EXPORT_INTERVAL:-5000} \
      uv run --no-sync uvicorn src.app:app --host 127.0.0.1 --port "$port" ;;
  batch)
    shift
    cd "$app"
    exec uv run --no-sync python -m scripts.batch --base-url "http://127.0.0.1:$port" "$@" ;;
  *) echo "usage: $0 prepare|setup|serve|batch [--requests N --concurrency N]|test [PYTEST ARGS]" >&2; exit 2 ;;
esac
