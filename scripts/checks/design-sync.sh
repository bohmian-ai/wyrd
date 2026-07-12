#!/usr/bin/env bash
# WHY THIS FILE EXISTS: wyrd-design.md is the active design authority. When
# a seam reaches code before its design entry is written, the implementation
# becomes the de-facto spec — future contributors can't tell whether a behavior
# was intentional or accidental, and the design doc loses its authority. This
# script enforces that named seams in the design are present before (or at the
# same time as) the code that implements them.
#
# WHAT IT CHECKS: symbols required by the current set of tracked seams appear
# in wyrd-design.md. When the corresponding code exists, the design entry is
# mandatory; when the code is absent the check still runs so design can be
# written ahead of implementation.
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
