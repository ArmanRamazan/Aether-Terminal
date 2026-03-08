"""
Orchestrator v3 — Lean Sprint Pipeline.

3-phase: PLAN (offline YAML) → EXECUTE (parallel + per-task retry) → VERIFY (optional).

Usage:
    python main.py tasks/sprint.yaml              # run sprint
    python main.py tasks/sprint.yaml --dry-run    # preview
    python main.py tasks/sprint.yaml --skip-verify # skip VERIFY phase
    python main.py tasks/sprint.yaml --instance s1 # parallel instance
    python main.py --resume                        # continue
    python main.py --status                        # show progress
    python main.py --reset                         # clear state
"""

from __future__ import annotations

import argparse
import atexit
import os
import signal
import sys
from pathlib import Path

if not sys.stdout.line_buffering:
    sys.stdout.reconfigure(line_buffering=True)  # type: ignore[attr-defined]

from agent_runner import emit_event, kill_all, log, request_shutdown
from config import LOG_DIR, ORCH_DIR, PID_FILE, STATE_DIR, STOP_FILE
from executor import run_execute
from state import SprintFile, SprintState, set_state_dir
from verifier import run_verify

# ---------------------------------------------------------------------------
# Instance isolation
# ---------------------------------------------------------------------------

_pid_file = PID_FILE
_state_dir = STATE_DIR
_stop_file = STOP_FILE


def _init_instance(instance_id: str) -> None:
    global _pid_file, _state_dir, _stop_file
    _state_dir = ORCH_DIR / ".state" / instance_id
    _pid_file = ORCH_DIR / f".pid-{instance_id}"
    _stop_file = ORCH_DIR / f".stop-{instance_id}"
    _state_dir.mkdir(parents=True, exist_ok=True)
    set_state_dir(_state_dir)


# ---------------------------------------------------------------------------
# Signal handling
# ---------------------------------------------------------------------------

def _handle_signal(signum: int, _frame: object) -> None:
    name = signal.Signals(signum).name
    log(f"\n  [{name}] Shutdown requested. Finishing current tasks...")
    request_shutdown()


for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
    try:
        signal.signal(sig, _handle_signal)
    except OSError:
        pass


# ---------------------------------------------------------------------------
# PID management
# ---------------------------------------------------------------------------

def _write_pid() -> None:
    _pid_file.parent.mkdir(parents=True, exist_ok=True)
    _pid_file.write_text(str(os.getpid()))


def _check_pid() -> bool:
    if not _pid_file.exists():
        return False
    try:
        pid = int(_pid_file.read_text().strip())
        os.kill(pid, 0)
        return True
    except (ValueError, ProcessLookupError, PermissionError):
        _pid_file.unlink(missing_ok=True)
        return False


def _cleanup() -> None:
    kill_all()
    _pid_file.unlink(missing_ok=True)


atexit.register(_cleanup)


# ---------------------------------------------------------------------------
# Pipeline
# ---------------------------------------------------------------------------

def _run_pipeline(state: SprintState, skip_verify: bool = False) -> bool:
    """Execute: EXECUTE → VERIFY."""
    log(f"\n{'#' * 60}")
    log(f"  SPRINT: {state.phase}")
    log(f"  Tasks: {len(state.tasks)}")
    log(f"  Pipeline: EXECUTE{'' if skip_verify else ' → VERIFY'}")
    log(f"{'#' * 60}")
    emit_event("sprint_start", extra={"phase": state.phase, "tasks": len(state.tasks)})

    # Determine starting phase for resume
    start_phase = state.current_pipeline_phase or "EXECUTE"

    # EXECUTE
    if start_phase in ("EXECUTE", ""):
        if not run_execute(state):
            _print_report(state)
            return False

    # VERIFY
    if not skip_verify:
        if not run_verify(state):
            log("\n  VERIFY failed — but EXECUTE tasks are already merged.")
            log("  Fix issues manually or re-run affected tasks.")
            _print_report(state)
            return False

    # Done
    passed = sum(1 for t in state.tasks if t.status == "passed")
    failed = sum(1 for t in state.tasks if t.status == "failed")
    skipped = sum(1 for t in state.tasks if t.status == "skipped")
    emit_event("sprint_done", extra={
        "phase": state.phase, "passed": passed, "failed": failed, "skipped": skipped,
    })

    state.current_pipeline_phase = "DONE"
    state.save()

    log(f"\n{'#' * 60}")
    log(f"  SPRINT COMPLETE: {state.phase}")
    log(f"{'#' * 60}")
    _print_report(state)
    return True


def _print_report(state: SprintState) -> None:
    log(f"\n  {'─' * 50}")
    for t in state.tasks:
        icon = {"passed": "V", "failed": "X", "skipped": "-",
                "running": "~", "pending": "."}.get(t.status, "?")
        retries = f" (x{t.attempts})" if t.attempts > 1 else ""
        error = f" — {t.error[:60]}" if t.error else ""
        log(f"  [{icon}] {t.id}: {t.title}{retries}{error}")

    passed = sum(1 for t in state.tasks if t.status == "passed")
    failed = sum(1 for t in state.tasks if t.status == "failed")
    skipped = sum(1 for t in state.tasks if t.status == "skipped")
    log(f"\n  Total: {len(state.tasks)} | Passed: {passed} | Failed: {failed} | Skipped: {skipped}")
    log(f"  {'─' * 50}")


# ---------------------------------------------------------------------------
# Commands
# ---------------------------------------------------------------------------

def cmd_run(yaml_path: str, dry_run: bool = False, skip_verify: bool = False) -> int:
    path = Path(yaml_path)
    if not path.exists():
        log(f"  X File not found: {path}")
        return 1
    if _check_pid():
        log("  X Another orchestrator is running. Use --reset to clear.")
        return 1

    _write_pid()
    LOG_DIR.mkdir(parents=True, exist_ok=True)

    sprint = SprintFile.load(path)
    state = SprintState.from_sprint(sprint, str(path))

    log(f"\n  Sprint: {sprint.phase}")
    log(f"  Description: {sprint.description}")
    log(f"  Tasks: {len(sprint.tasks)}")

    if dry_run:
        log("\n  [DRY RUN] Tasks:")
        for t in sprint.tasks:
            deps = f" (depends: {t.depends_on})" if t.depends_on else ""
            log(f"    [{t.id}] {t.title} — agent: {t.agent}{deps}")
        return 0

    success = _run_pipeline(state, skip_verify=skip_verify)
    return 0 if success else 1


def cmd_resume(skip_verify: bool = False) -> int:
    state = SprintState.load()
    if not state:
        log("  X No saved state. Run a sprint first.")
        return 1
    if _check_pid():
        log("  X Another orchestrator is running.")
        return 1

    _write_pid()
    state.recover_crashed()
    log(f"\n  Resuming: {state.phase} (phase: {state.current_pipeline_phase})")

    success = _run_pipeline(state, skip_verify=skip_verify)
    return 0 if success else 1


def cmd_status() -> int:
    state = SprintState.load()
    if not state:
        log("  No saved state.")
        return 0

    log(f"\n  Sprint: {state.phase}")
    log(f"  Phase: {state.current_pipeline_phase}")
    log(f"  Started: {state.started_at}")
    if state.paused_at:
        log(f"  Paused: {state.paused_at}")
    _print_report(state)

    running = _check_pid()
    log(f"  Running: {'yes' if running else 'no'}")
    return 0


def cmd_reset() -> int:
    state_file = _state_dir / "state.json"
    if state_file.exists():
        state_file.unlink()
        log("  State cleared.")
    _pid_file.unlink(missing_ok=True)
    _stop_file.unlink(missing_ok=True)
    log("  Reset complete.")
    return 0


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main() -> int:
    parser = argparse.ArgumentParser(
        description="Orchestrator v3 — Lean Sprint Pipeline",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("yaml_file", nargs="?", help="YAML sprint file")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--skip-verify", action="store_true", help="Skip VERIFY phase")
    parser.add_argument("--resume", action="store_true")
    parser.add_argument("--status", action="store_true")
    parser.add_argument("--reset", action="store_true")
    parser.add_argument("--instance", type=str, default="",
                        help="Instance ID for parallel execution")

    args = parser.parse_args()

    if args.instance:
        _init_instance(args.instance)

    if args.status:
        return cmd_status()
    if args.reset:
        return cmd_reset()
    if args.resume:
        return cmd_resume(skip_verify=args.skip_verify)
    if args.yaml_file:
        return cmd_run(args.yaml_file, dry_run=args.dry_run, skip_verify=args.skip_verify)

    parser.print_help()
    return 1


if __name__ == "__main__":
    sys.exit(main())
