#!/usr/bin/env bash
# Run multiple sprint YAML files sequentially.
#
# Usage:
#   ./run-pipeline.sh tasks/ms2-overview-tab.yaml tasks/ms2-vim-navigation.yaml
#   ./run-pipeline.sh --dry-run tasks/ms2-*.yaml
#   ./run-pipeline.sh --auto tasks/ms2-*.yaml    # never ask, always continue
#
# Stopping:
#   Option 1: Ctrl+C (sends SIGINT to pipeline + child python)
#   Option 2: touch tools/orchestrator/.stop  (from another terminal)
#   Option 3: ./run-pipeline.sh --stop  (kills running pipeline by PID)

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

PIPELINE_PID_FILE="$SCRIPT_DIR/.pipeline-pid"
STOP_FILE="$SCRIPT_DIR/.stop"
CHILD_PID=""

# ---------------------------------------------------------------
# --stop: kill a running pipeline from another terminal
# ---------------------------------------------------------------
if [[ "${1:-}" == "--stop" ]]; then
  if [[ -f "$PIPELINE_PID_FILE" ]]; then
    PID=$(cat "$PIPELINE_PID_FILE")
    if kill -0 "$PID" 2>/dev/null; then
      echo "Stopping pipeline (PID $PID) and all children..."
      kill -- -"$PID" 2>/dev/null || kill "$PID" 2>/dev/null
      rm -f "$PIPELINE_PID_FILE"
    else
      echo "Pipeline PID $PID is not running."
      rm -f "$PIPELINE_PID_FILE"
    fi
  fi
  touch "$STOP_FILE"
  echo "Stop signal sent. Pipeline will exit after current task finishes."
  exit 0
fi

# ---------------------------------------------------------------
# Cleanup: kill child python, remove PID file, remove stop file
# ---------------------------------------------------------------
cleanup() {
  if [[ -n "$CHILD_PID" ]] && kill -0 "$CHILD_PID" 2>/dev/null; then
    echo ""
    echo "  Stopping orchestrator (PID $CHILD_PID)..."
    kill -- -"$CHILD_PID" 2>/dev/null || kill "$CHILD_PID" 2>/dev/null
    wait "$CHILD_PID" 2>/dev/null
  fi
  rm -f "$PIPELINE_PID_FILE"
  rm -f "$STOP_FILE"
}

trap cleanup EXIT
trap 'echo ""; echo "  [Ctrl+C] Shutting down..."; exit 130' INT TERM HUP

# Write our PID so --stop can find us
echo $$ > "$PIPELINE_PID_FILE"

# ---------------------------------------------------------------
# Parse flags and files
# ---------------------------------------------------------------
AUTO=false
FLAGS=()
ORCH_FLAGS=()
FILES=()
MILESTONES=()
for arg in "$@"; do
  case "$arg" in
    --auto) AUTO=true ;;
    --dry-run|--skip-verify) FLAGS+=("$arg"); ORCH_FLAGS+=("$arg") ;;
    --ms[0-9]|--ms[0-9][0-9]|--p[0-9]|--p[0-9][0-9]) MILESTONES+=("$arg") ;;
    *) FILES+=("$arg") ;;
  esac
done

# Expand --msN flags into matching YAML files (sorted)
for ms in "${MILESTONES[@]}"; do
  prefix="${ms#--}"  # e.g. "ms2"
  matched=()
  for f in tasks/${prefix}-*.yaml; do
    [[ -f "$f" ]] && matched+=("$f")
  done
  if [[ ${#matched[@]} -eq 0 ]]; then
    echo "  Warning: no files match tasks/${prefix}-*.yaml"
  else
    FILES+=("${matched[@]}")
  fi
done

if [[ ${#FILES[@]} -eq 0 ]]; then
  echo "Usage: $0 [--auto] [--dry-run] [--skip-verify] [--msN ...] [yaml ...]"
  echo ""
  echo "Flags:"
  echo "  --auto           Never ask on failure, always continue to next sprint"
  echo "  --dry-run        Preview without executing"
  echo "  --skip-verify    Skip VERIFY phase"
  echo "  --msN            Expand to all tasks/msN-*.yaml (e.g. --ms2 --ms3)"
  echo "  --pN             Expand to all tasks/pN-*.yaml (e.g. --p0 --p1)"
  echo ""
  echo "Stop a running pipeline:"
  echo "  $0 --stop                           # from another terminal"
  echo "  touch tools/orchestrator/.stop       # manual stop file"
  echo "  Ctrl+C                               # from same terminal"
  echo ""
  echo "Examples:"
  echo "  $0 --auto --p0 --p1 --p2             # all Phase 0 + 1 + 2 sprints"
  echo "  $0 --auto --ms2 --ms3               # all MS2 + MS3 sprints"
  echo "  $0 --auto tasks/ms2-*.yaml          # same with glob"
  echo "  $0 --dry-run --ms2                   # preview MS2"
  exit 1
fi

TOTAL=${#FILES[@]}
PASSED=0
FAILED=0

echo ""
echo "============================================================"
echo "  PIPELINE: $TOTAL sprints queued  (PID $$)"
echo "  Mode: $(if $AUTO; then echo "auto (no prompts)"; else echo "interactive"; fi)"
echo "  Flags: ${ORCH_FLAGS[*]:-none}"
echo "  Stop:  ./run-pipeline.sh --stop  OR  Ctrl+C"
echo "============================================================"
echo ""

# ---------------------------------------------------------------
# Run sprints
# ---------------------------------------------------------------
for i in "${!FILES[@]}"; do
  # Check stop file before each sprint
  if [[ -f "$STOP_FILE" ]]; then
    echo "  Stop file detected. Aborting pipeline."
    rm -f "$STOP_FILE"
    break
  fi

  FILE="${FILES[$i]}"
  NUM=$((i + 1))

  if [[ ! -f "$FILE" ]]; then
    echo "  X File not found: $FILE"
    FAILED=$((FAILED + 1))
    continue
  fi

  PHASE=$(grep '^phase:' "$FILE" | head -1 | sed 's/phase: *"\?\([^"]*\)"\?/\1/')

  echo ""
  echo "************************************************************"
  echo "  [$NUM/$TOTAL] $PHASE"
  echo "  File: $FILE"
  echo "************************************************************"
  echo ""

  # Reset state from previous sprint (suppress output)
  python3 main.py --reset >/dev/null 2>&1 || true

  # Run sprint in a new process group so we can kill it cleanly
  set -m
  python3 main.py "$FILE" "${ORCH_FLAGS[@]}" &
  CHILD_PID=$!
  set +m

  # Wait for child; if interrupted, cleanup trap handles it
  wait "$CHILD_PID"
  EXIT_CODE=$?
  CHILD_PID=""

  if [[ $EXIT_CODE -eq 0 ]]; then
    echo ""
    echo "  V [$NUM/$TOTAL] $PHASE — SUCCESS"
    PASSED=$((PASSED + 1))
  else
    echo ""
    echo "  X [$NUM/$TOTAL] $PHASE — FAILED (exit $EXIT_CODE)"
    FAILED=$((FAILED + 1))

    # If killed by signal, stop immediately
    if [[ $EXIT_CODE -gt 128 ]]; then
      echo "  Pipeline interrupted by signal."
      break
    fi

    # In auto mode — always continue, no questions
    if $AUTO; then
      echo "  [auto] Continuing to next sprint..."
      continue
    fi

    # Interactive mode — ask
    if [[ -t 0 ]] && [[ ! " ${ORCH_FLAGS[*]:-} " =~ " --dry-run " ]]; then
      echo ""
      read -rp "  Continue to next sprint? [Y/n] " answer
      case "$answer" in
        n|N|no|NO)
          echo "  Aborting pipeline."
          break
          ;;
      esac
    else
      echo "  Aborting pipeline (non-interactive mode)."
      break
    fi
  fi
done

echo ""
echo "============================================================"
echo "  PIPELINE COMPLETE"
echo "  Sprints: $TOTAL | Passed: $PASSED | Failed: $FAILED"
echo "============================================================"

[[ "$FAILED" -eq 0 ]]
