"""Orchestrator v3 — Claude Code agent subprocess runner."""

from __future__ import annotations

import json as _json
import os
import signal
import subprocess
import threading
import time
from pathlib import Path

from config import (
    CLAUDE_TIMEOUT,
    LOG_DIR,
    QUOTA_MAX_RETRIES,
    QUOTA_PATTERNS,
    QUOTA_RETRY_INTERVAL,
    ROOT,
    TEST_TIMEOUT,
)

# ---------------------------------------------------------------------------
# Process tracking
# ---------------------------------------------------------------------------

_child_procs: dict[str, subprocess.Popen] = {}
_child_procs_lock = threading.Lock()
_print_lock = threading.Lock()

shutdown_requested = False


def request_shutdown() -> None:
    global shutdown_requested
    shutdown_requested = True


def is_shutdown() -> bool:
    return shutdown_requested


# ---------------------------------------------------------------------------
# Output helpers
# ---------------------------------------------------------------------------

_events_file = None
_events_lock = threading.Lock()


def _init_events_log() -> None:
    global _events_file
    LOG_DIR.mkdir(parents=True, exist_ok=True)
    ts = time.strftime("%Y%m%d-%H%M%S")
    _events_file = open(LOG_DIR / f"events-{ts}.jsonl", "a")


def emit_event(
    event: str,
    *,
    task_id: str | None = None,
    agent: str | None = None,
    duration_s: float | None = None,
    exit_code: int | None = None,
    error: str | None = None,
    extra: dict | None = None,
) -> None:
    if _events_file is None:
        _init_events_log()
    record = {
        "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "event": event,
    }
    if task_id:
        record["task_id"] = task_id
    if agent:
        record["agent"] = agent
    if duration_s is not None:
        record["duration_s"] = round(duration_s, 1)
    if exit_code is not None:
        record["exit_code"] = exit_code
    if error:
        record["error"] = error[:500]
    if extra:
        record.update(extra)
    line = _json.dumps(record, ensure_ascii=False)
    with _events_lock:
        if _events_file:
            _events_file.write(line + "\n")
            _events_file.flush()


def log(msg: str, task_id: str | None = None) -> None:
    if task_id:
        msg = f"  [{task_id}] {msg.lstrip()}"
    with _print_lock:
        print(msg, flush=True)


# ---------------------------------------------------------------------------
# Process management
# ---------------------------------------------------------------------------

def kill_child(task_id: str | None = None) -> None:
    with _child_procs_lock:
        if task_id is not None:
            proc = _child_procs.get(task_id)
            if proc is None or proc.poll() is not None:
                _child_procs.pop(task_id, None)
                return
            targets = [(task_id, proc)]
        else:
            targets = [(tid, p) for tid, p in _child_procs.items()
                        if p.poll() is None]

    for tid, proc in targets:
        log(f"Killing child process (task {tid})...")
        try:
            pgid = os.getpgid(proc.pid)
            os.killpg(pgid, signal.SIGTERM)
            proc.wait(timeout=5)
        except (ProcessLookupError, subprocess.TimeoutExpired, OSError):
            try:
                proc.kill()
            except (ProcessLookupError, OSError):
                pass
        with _child_procs_lock:
            _child_procs.pop(tid, None)


def kill_all() -> None:
    kill_child(None)


# ---------------------------------------------------------------------------
# Subprocess streaming
# ---------------------------------------------------------------------------

def _stream_process(
    cmd: list[str] | str,
    cwd: str,
    timeout: int,
    *,
    shell: bool = False,
    task_id: str | None = None,
) -> tuple[int, str]:
    collected: list[str] = []
    start = time.time()
    prefix = f"  [{task_id}] |  " if task_id else "  |  "

    try:
        proc = subprocess.Popen(
            cmd, cwd=cwd,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            text=True, shell=shell, bufsize=1,
            start_new_session=True,
        )
        proc_key = task_id or "_default"
        with _child_procs_lock:
            _child_procs[proc_key] = proc

        assert proc.stdout is not None
        for line in proc.stdout:
            collected.append(line)
            display = line.rstrip("\n")[:200]
            with _print_lock:
                print(f"{prefix}{display}", flush=True)
            if shutdown_requested:
                kill_child(proc_key)
                break

        proc.wait(timeout=timeout)
        elapsed = time.time() - start
        output = "".join(collected)
        with _print_lock:
            print(f"{prefix}--- Done in {elapsed:.0f}s (exit={proc.returncode}) ---", flush=True)
        with _child_procs_lock:
            _child_procs.pop(proc_key, None)
        return proc.returncode, output

    except subprocess.TimeoutExpired:
        proc_key = task_id or "_default"
        kill_child(proc_key)
        output = "".join(collected)
        return 1, output + f"\nTIMEOUT after {timeout}s"

    except FileNotFoundError:
        proc_key = task_id or "_default"
        with _child_procs_lock:
            _child_procs.pop(proc_key, None)
        return 1, f"Command not found: {cmd[0] if isinstance(cmd, list) else cmd}"


# ---------------------------------------------------------------------------
# Quota handling
# ---------------------------------------------------------------------------

def _is_quota_error(exit_code: int, output: str) -> bool:
    if exit_code == 0:
        return False
    output_lower = output.lower()
    return any(p in output_lower for p in QUOTA_PATTERNS)


def _wait_for_quota(attempt: int) -> bool:
    minutes = QUOTA_RETRY_INTERVAL // 60
    next_try = time.strftime("%H:%M", time.localtime(time.time() + QUOTA_RETRY_INTERVAL))
    log(f"\n  {'!' * 50}")
    log(f"  QUOTA LIMIT HIT — waiting {minutes} min (attempt {attempt}/{QUOTA_MAX_RETRIES})")
    log(f"  Next retry at {next_try}. Press Ctrl+C to abort.")
    log(f"  {'!' * 50}\n")

    elapsed = 0
    while elapsed < QUOTA_RETRY_INTERVAL:
        if shutdown_requested:
            return False
        time.sleep(10)
        elapsed += 10
        remaining = (QUOTA_RETRY_INTERVAL - elapsed) // 60
        if elapsed % 300 == 0 and remaining > 0:
            log(f"  ... {remaining} min remaining until retry")

    return True


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------

def run_agent(
    agent_name: str,
    prompt: str,
    *,
    cwd: Path | None = None,
    task_id: str | None = None,
    timeout: int | None = None,
) -> tuple[int, str]:
    """Run Claude Code with a prompt. Auto-retries on quota."""
    work_dir = str(cwd or ROOT)
    cmd = [
        "claude",
        "--dangerously-skip-permissions",
        "-p", prompt,
    ]

    for quota_attempt in range(1, QUOTA_MAX_RETRIES + 1):
        log(f"+-- Agent '{agent_name}' starting...", task_id)
        emit_event("agent_start", task_id=task_id, agent=agent_name)
        t0 = time.time()
        code, output = _stream_process(
            cmd, work_dir, timeout or CLAUDE_TIMEOUT, task_id=task_id,
        )
        elapsed = time.time() - t0
        log(f"+-- Agent '{agent_name}' done (exit={code}, {elapsed:.0f}s)", task_id)
        emit_event(
            "agent_done", task_id=task_id, agent=agent_name,
            exit_code=code, duration_s=elapsed,
        )

        if not _is_quota_error(code, output):
            return code, output

        log(f"+-- QUOTA ERROR detected", task_id)
        if quota_attempt >= QUOTA_MAX_RETRIES:
            return code, output

        if not _wait_for_quota(quota_attempt):
            return code, output

    return code, output  # type: ignore[possibly-undefined]


def run_tests(
    command: str,
    *,
    cwd: Path | None = None,
    task_id: str | None = None,
) -> tuple[int, str]:
    work_dir = str(cwd or ROOT)
    log(f"+-- Tests: {command}", task_id)
    t0 = time.time()
    code, output = _stream_process(
        command, work_dir, TEST_TIMEOUT, shell=True, task_id=task_id,
    )
    elapsed = time.time() - t0
    status = "PASSED" if code == 0 else "FAILED"
    log(f"+-- Tests {status} ({elapsed:.0f}s)", task_id)
    emit_event(
        "test_done", task_id=task_id, exit_code=code,
        duration_s=elapsed, extra={"command": command, "status": status},
    )
    return code, output


def run_git(args: list[str], cwd: Path | None = None) -> tuple[int, str]:
    work_dir = str(cwd or ROOT)
    result = subprocess.run(
        ["git"] + args, cwd=work_dir, capture_output=True, text=True,
    )
    return result.returncode, result.stdout + result.stderr


def has_git_changes(cwd: Path | None = None) -> bool:
    """Check for uncommitted changes OR new commits on worktree branch."""
    code, output = run_git(["status", "--porcelain"], cwd=cwd)
    if output.strip():
        return True
    code, base = run_git(["merge-base", "HEAD", "main"], cwd=cwd)
    if code == 0 and base.strip():
        code2, diff = run_git(["diff", "--stat", base.strip(), "HEAD"], cwd=cwd)
        if diff.strip():
            return True
    code, log_out = run_git(["log", "main..HEAD", "--oneline"], cwd=cwd)
    return bool(log_out.strip())
