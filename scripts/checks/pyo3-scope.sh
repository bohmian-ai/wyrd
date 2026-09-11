#!/usr/bin/env bash
# WHY THIS FILE EXISTS: PyO3 links against libpython at compile time. If it
# leaks into wyrd-spec or non-approved shared crates, every consumer —
# including the Rust-only server, CLI, and Skald engine — transitively links
# libpython, making cross-compilation (musl, WASM) harder and adding a runtime
# dependency on the Python interpreter in environments that should be Python-
# free. The python feature gate is the containment boundary; these checks
# enforce that nothing bypasses it by importing PyO3 from an unapproved crate.
#
# WHAT IT CHECKS:
#   - wyrd-spec: zero PyO3 references (no python feature allowed)
#   - shared crates except approved Python owners: zero PyO3 references
#   - Skald engine crates: PyO3 only in the explicitly allowlisted files
#   - each approved Skald crate: python feature defined, PyO3 declared optional
#   - wyrd-utils: python feature defined, PyO3 optional, helpers cfg-gated
#   - wyrd-utils default build: PyO3 does not appear in the dep tree
set -eu

if rg -n 'pyo3|pymodule|pyclass|pymethods' crates/wyrd-spec; then
  echo 'PyO3 scope violation detected in wyrd-spec'
  exit 1
fi

if rg -n 'pyo3|pymodule|pyclass|pymethods' crates/shared \
  --glob '!crates/shared/wyrd-utils/**' \
  --glob '!crates/shared/wyrd-sdk/**'; then
  echo 'PyO3 scope violation detected in Python-free shared crates'
  exit 1
fi

if rg -n 'pyo3|pymodule|pyclass|pymethods|Python|Bound<|Py<|PyErr' crates/skald \
  --glob '!crates/skald/skald-observer/**' \
  --glob '!crates/skald/skald-prompt/**' \
  --glob '!crates/skald/skald-runtime/Cargo.toml' \
  --glob '!crates/skald/skald-runtime/src/lib.rs' \
  --glob '!crates/skald/skald-runtime/src/python.rs' \
  --glob '!crates/skald/skald-agent/Cargo.toml' \
  --glob '!crates/skald/skald-agent/src/agent.rs' \
  --glob '!crates/skald/skald-agent/src/lib.rs' \
  --glob '!crates/skald/skald-agent/src/python.rs' \
  --glob '!crates/skald/skald-agent/src/run.rs' \
  --glob '!crates/skald/skald-agent/src/session.rs' \
  --glob '!crates/skald/skald-tool/Cargo.toml' \
  --glob '!crates/skald/skald-tool/src/lib.rs' \
  --glob '!crates/skald/skald-tool/src/python.rs' \
  --glob '!crates/skald/skald-workflow/Cargo.toml' \
  --glob '!crates/skald/skald-workflow/src/lib.rs' \
  --glob '!crates/skald/skald-workflow/src/python.rs' \
  --glob '!crates/skald/skald-workflow/src/run.rs' \
  --glob '!crates/skald/skald-workflow/src/task.rs' \
  --glob '!crates/skald/skald-workflow/src/workflow_surface.rs'; then
  echo 'PyO3 scope violation detected in Python-free Skald engine crates'
  exit 1
fi

for crate in skald-observer skald-prompt skald-runtime skald-agent skald-tool skald-workflow; do
  manifest="crates/skald/$crate/Cargo.toml"
  if ! rg -q 'python = \[' "$manifest" || ! rg -q 'dep:pyo3' "$manifest"; then
    echo "$crate must gate PyO3 behind its python feature"
    exit 1
  fi

  if ! rg -q 'pyo3 = \{ workspace = true, optional = true \}' "$manifest"; then
    echo "$crate PyO3 dependency must remain optional"
    exit 1
  fi
done

if ! rg -qF 'python = ["dep:pyo3"]' crates/shared/wyrd-utils/Cargo.toml; then
  echo 'wyrd-utils must gate PyO3 behind its python feature'
  exit 1
fi

if ! rg -q 'pyo3 = \{ workspace = true, optional = true \}' crates/shared/wyrd-utils/Cargo.toml; then
  echo 'wyrd-utils PyO3 dependency must remain optional'
  exit 1
fi

if ! rg -q '#\[cfg\(feature = "python"\)\]' crates/shared/wyrd-utils/src/lib.rs; then
  echo 'wyrd-utils Python helpers must remain cfg-gated'
  exit 1
fi

if ! rg -q 'python = \[' crates/shared/wyrd-sdk/Cargo.toml || \
   ! rg -q 'dep:pyo3' crates/shared/wyrd-sdk/Cargo.toml; then
  echo 'wyrd-sdk must gate PyO3 behind its python feature'
  exit 1
fi

if ! rg -q 'pyo3 = \{ workspace = true, optional = true \}' crates/shared/wyrd-sdk/Cargo.toml; then
  echo 'wyrd-sdk PyO3 dependency must remain optional'
  exit 1
fi

if ! rg -q 'python = \[' crates/wyrd/wyrd-cli/Cargo.toml || \
   ! rg -q 'dep:pyo3' crates/wyrd/wyrd-cli/Cargo.toml || \
   ! rg -q 'pyo3 = \{ workspace = true, optional = true \}' crates/wyrd/wyrd-cli/Cargo.toml; then
  echo 'wyrd-cli must gate its optional Python adapter behind its python feature'
  exit 1
fi

forbidden_utils_default=$(cargo tree -p wyrd-utils --no-default-features -e normal | \
  rg '(^|[ ─└├])pyo3' || true)
if [ -n "$forbidden_utils_default" ]; then
  echo "FAIL: wyrd-utils pulls PyO3 without its python feature:"
  echo "$forbidden_utils_default"
  exit 1
fi
