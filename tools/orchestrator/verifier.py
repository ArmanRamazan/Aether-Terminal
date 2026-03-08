"""Orchestrator v3 — VERIFY phase: optional full test suite."""

from __future__ import annotations

from agent_runner import emit_event, log, run_tests
from config import ROOT
from state import SprintState


def run_verify(state: SprintState) -> bool:
    """VERIFY phase: run full test suite across workspace."""
    log(f"\n{'=' * 60}")
    log(f"  PHASE: VERIFY")
    log(f"{'=' * 60}")

    state.current_pipeline_phase = "VERIFY"
    state.save()

    # Collect unique test commands from passed tasks
    seen_commands: set[str] = set()
    test_commands: list[str] = []
    for t in state.tasks:
        if t.status == "passed" and t.test and t.test not in seen_commands:
            seen_commands.add(t.test)
            test_commands.append(t.test)

    if not test_commands:
        log("  No test commands to verify. Skipping.")
        return True

    all_passed = True

    # Per-task test commands (deduplicated)
    for cmd in test_commands:
        log(f"\n  Running: {cmd}")
        code, output = run_tests(cmd, task_id="verify")
        if code != 0:
            log(f"  X FAILED: {cmd}")
            all_passed = False
        else:
            log(f"  V PASSED: {cmd}")

    # Workspace-level verification
    workspace_test = f"cd {ROOT} && cargo test --workspace 2>&1 || true"
    log(f"\n  Running workspace test...")
    code, output = run_tests(workspace_test, task_id="verify-workspace")
    if code != 0:
        log(f"  X Workspace test failed")
        all_passed = False
    else:
        log(f"  V Workspace test passed")

    status = "PASSED" if all_passed else "FAILED"
    log(f"\n  VERIFY: {status}")
    emit_event("verify_done", extra={"status": status})

    return all_passed
