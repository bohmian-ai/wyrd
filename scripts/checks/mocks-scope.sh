#!/usr/bin/env bash
# WHY THIS FILE EXISTS: mock libraries (mockall, wiremock, mockito) are
# test-only tools. Declared as a regular dependency, a mock crate becomes a
# transitive dependency of every downstream consumer and can reach a
# production build. Declared under [dev-dependencies], the compiler already
# refuses any non-test use, so source files need no allowlist.
#
# WHAT IT CHECKS: no workspace package other than wyrd-testing, the test
# harness crate, declares a mock crate as a normal or build dependency.
set -euo pipefail

command -v jq >/dev/null || { echo 'check:mocks-scope requires jq'; exit 1; }

leaks=$(cargo metadata --format-version=1 --no-deps --locked |
  jq -r '.packages[]
         | select(.name != "wyrd-testing")
         | .name as $package
         | .dependencies[]
         | select(.name | test("^(mockall|wiremock|mockito)$"))
         | select(.kind != "dev")
         | "\($package): \(.name) (\(.kind // "normal"))"')

if [[ -n "$leaks" ]]; then
  echo 'mock dependency declared outside [dev-dependencies]:'
  echo "$leaks"
  exit 1
fi
