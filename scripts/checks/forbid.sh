# Sourced by boundary checks; not a check itself.
#
# Fail the check when rg finds a forbidden match or cannot search (exit 2, e.g.
# a missing path). `! rg` would pass on a search error, and under `set -e` a
# negated command that fails does not stop the script, so only the last one
# ever decided the exit status.
forbid() {
    local status=0
    rg -n --no-heading "$@" || status=$?
    if [ "$status" -ne 1 ]; then
        echo "forbidden pattern found or search failed (rg exit $status)" >&2
        exit 1
    fi
}
