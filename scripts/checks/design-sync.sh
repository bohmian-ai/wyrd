#!/usr/bin/env bash
# Assert ValaQueryService and all four payload-read permissions are accepted in
# wyrd-design.md before the vala_query seam exists in code.
set -eu
DESIGN=architecture/wyrd-design.md
fail=0
for sym in ValaQueryService BifrostTracePayload BifrostLogPayload BifrostGenAiPayload BifrostAgentTracePayload; do
  if ! rg -q "$sym" "$DESIGN"; then
    echo "check:design-sync FAIL: '$sym' missing from $DESIGN"
    fail=1
  fi
done
# Only enforce the seam-exists check when the query seam is present in code
if rg -qr 'vala_query|ValaQueryService' crates/wyrd/wyrd-server/src/ 2>/dev/null || \
   rg -qr 'vala::api::Query' crates/wyrd-spec/src/vala/ 2>/dev/null; then
  if [ "$fail" -ne 0 ]; then
    echo "check:design-sync FAIL: vala_query seam exists in code but wyrd-design.md is missing required symbols"
    exit 1
  fi
fi
if [ "$fail" -ne 0 ]; then
  echo "check:design-sync FAIL: wyrd-design.md is missing required symbols (add the acceptance entry from Stage 4 task 01)"
  exit 1
fi
echo "check:design-sync OK"
