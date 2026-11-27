#!/usr/bin/env bash
# Temporary CI memory diagnosis: runs the workspace test build that fails on
# RunsOn and records every descendant process plus system memory every two
# seconds. With arguments, runs that command instead (recorder self-check).
#
# Output: $DIAG_OUT_DIR (default .dev/ci-memory-diagnosis/runs/<UTC time>):
#   meta.txt     source, host, toolchain, flags, target state
#   build.log    timestamped build output
#   samples.log  "sys" lines (system) and "proc" lines (one per descendant)
#   result.txt   exit status, or aborted-memory-pressure
#   oom.txt      Linux kernel OOM lines
# Exits with the build's status; 3 when aborted on macOS critical pressure.
set -uo pipefail

repo=$(git rev-parse --show-toplevel)
out=${DIAG_OUT_DIR:-$repo/.dev/ci-memory-diagnosis/runs/$(date -u +%Y%m%dT%H%M%SZ)}
export CARGO_TARGET_DIR=${DIAG_TARGET_DIR:-$repo/.dev/ci-memory-diagnosis/target}
export CARGO_INCREMENTAL=0
# Main's Cargo.toml value; a developer's ~/.cargo/config.toml can override it.
export CARGO_PROFILE_DEV_DEBUG=line-tables-only
mkdir -p "$out" "$CARGO_TARGET_DIR"
os=$(uname -s)

if [ $# -eq 0 ]; then
  set -- cargo build --locked --workspace --tests \
    --exclude wyrd-sdk-python --exclude wyrd-sdk-ts --exclude wyrd-sdk-ts-testing
fi

{
  echo "start=$(date -u +%FT%TZ)"
  echo "command=$*"
  echo "sha=$(git -C "$repo" rev-parse HEAD) dirty_files=$(git -C "$repo" status --porcelain | wc -l | tr -d ' ')"
  echo "host=$(uname -a)"
  if [ "$os" = Darwin ]; then
    echo "cpus=$(sysctl -n hw.ncpu) ram_bytes=$(sysctl -n hw.memsize) swap=$(sysctl -n vm.swapusage)"
    echo "installed_ld=$(ld -v 2>&1 | head -1)"
  else
    echo "cpus=$(nproc) $(free -b | awk '/^Mem/{print "ram_bytes="$2} /^Swap/{print "swap_bytes="$2}' | tr '\n' ' ')"
    # Installed tools only; the workflow confirms the linker from the binaries.
    echo "installed_ld_bfd=$(ld.bfd --version 2>/dev/null | head -1)"
    echo "installed_rust_lld=$("$(rustc --print target-libdir)/../bin/rust-lld" -flavor gnu --version 2>/dev/null)"
  fi
  echo "rustc=$(rustc -V) cargo=$(cargo -V)"
  for v in RUSTFLAGS CARGO_BUILD_JOBS CARGO_INCREMENTAL CARGO_PROFILE_DEV_DEBUG \
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER CARGO_TARGET_DIR CARGO_HOME; do
    echo "$v=${!v-<unset>}"
  done
  echo "user_cargo_profile=$(grep -A3 '^\[profile' "${CARGO_HOME:-$HOME/.cargo}/config.toml" 2>/dev/null | tr '\n' ';')"
  echo "target_initial_files=$(find "$CARGO_TARGET_DIR" -type f | wc -l | tr -d ' ') size=$(du -sh "$CARGO_TARGET_DIR" | cut -f1)"
} | tee "$out/meta.txt"

# Own process group, so an abort stops only this build's tree.
set -m
( "$@" ) 2>&1 | perl -MPOSIX -ne 'BEGIN { $| = 1 } print strftime("%H:%M:%S ", gmtime), $_' \
  | tee "$out/build.log" &
job=$!
pgid=$(ps -o pgid= -p "$job" | tr -d ' ')

sample() {
  local ts sys
  ts=$(date -u +%T)
  if [ "$os" = Darwin ]; then
    pressure=$(sysctl -n kern.memorystatus_vm_pressure_level)
    sys="pressure=$pressure free_pct=$(sysctl -n kern.memorystatus_level) swap_used=$(sysctl -n vm.swapusage | awk '{print $6}')"
    sys="$sys $(vm_stat | awk -v ps="$(sysctl -n hw.pagesize)" '/Pages free|Pages inactive|Pages speculative/{gsub("\\.","",$NF); a+=$NF} /Swapouts/{gsub("\\.","",$NF); so=$NF} END{printf "avail_mb=%d swapouts=%d", a*ps/1048576, so}')"
  else
    sys=$(free -m | awk '/^Mem/{printf "avail_mb=%d ", $7} /^Swap/{printf "swap_used_mb=%d", $3}')
    sys="$sys psi_mem=$(awk '/some/{print $2}' /proc/pressure/memory 2>/dev/null) psi_io=$(awk '/some/{print $2}' /proc/pressure/io 2>/dev/null)"
  fi
  sys="$sys load=$(uptime | sed 's/.*load average[s]*: //' | tr -d ',' | tr ' ' '/')"
  ps -ww -A -o pid=,ppid=,pgid=,rss=,pcpu=,etime=,args= | awk -v pg="$pgid" -v ts="$ts" -v sys="$sys" '
    $3 == pg {
      cmd = ""; for (i = 7; i <= NF; i++) cmd = cmd " " $i
      what = $7; sub(".*/", "", what)
      if (match(cmd, /--crate-name [^ ]+/)) what = what ":" substr(cmd, RSTART + 13, RLENGTH - 13)
      if (cmd ~ / --test( |$)/) what = what "[test]"
      if (what ~ /^(ld|ld\.bfd|ld\.lld|rust-lld|ld64|collect2|cc|clang)/) { link++; link_kb += $4 }
      if (what ~ /^rustc/) { rustc++; rustc_kb += $4 }
      n++; kb += $4
      lines = lines sprintf("proc %s pid=%s ppid=%s rss_mb=%d cpu=%s etime=%s %s\n", ts, $1, $2, $4 / 1024, $5, $6, what)
    }
    END {
      printf "sys %s %s procs=%d rss_sum_mb=%d rustc=%d/%dmb link=%d/%dmb\n", ts, sys, n, kb / 1024, rustc, rustc_kb / 1024, link, link_kb / 1024
      printf "%s", lines
    }'
}

critical=0
aborted=0
while kill -0 "$job" 2>/dev/null; do
  sample > "$out/last-sample"
  cat "$out/last-sample" >> "$out/samples.log"
  head -1 "$out/last-sample"
  if [ "$os" = Darwin ]; then
    if [ "$pressure" -ge 4 ]; then critical=$((critical + 1)); else critical=0; fi
    if [ "$critical" -ge 2 ]; then
      echo "critical memory pressure twice; stopping process group $pgid" | tee -a "$out/samples.log"
      kill -TERM -- "-$pgid" 2>/dev/null
      sleep 5
      kill -KILL -- "-$pgid" 2>/dev/null
      aborted=1
      break
    fi
  fi
  sleep 2
done
wait "$job"
status=$?

if [ "$os" = Linux ]; then
  sudo -n dmesg 2>/dev/null | grep -iE 'out of memory|oom-kill|killed process' > "$out/oom.txt"
  cat "$out/oom.txt"
fi
if [ "$aborted" = 1 ]; then
  echo "aborted-memory-pressure (pipeline status $status)" | tee "$out/result.txt"
  exit 3
fi
echo "exit=$status end=$(date -u +%FT%TZ)" | tee "$out/result.txt"
echo "output: $out"
exit "$status"
