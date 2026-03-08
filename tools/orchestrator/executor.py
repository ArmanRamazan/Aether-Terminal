"""Orchestrator v3 — EXECUTE phase with parallel workers and per-task retry."""

from __future__ import annotations

import fcntl
import queue
import subprocess
import threading
import time
from pathlib import Path

from agent_runner import (
    emit_event,
    has_git_changes,
    is_shutdown,
    log,
    run_agent,
    run_git,
    run_tests,
)
from config import (
    DISPATCH_INTERVAL,
    MAX_TASK_RETRIES,
    MAX_WORKERS,
    ORCH_DIR,
    ROOT,
    WORKTREE_DIR,
)
from state import SprintState, Task, _now

_state_lock = threading.Lock()
_merge_lock = threading.Lock()
_MERGE_LOCK_FILE = ORCH_DIR / ".merge.lock"
_SENTINEL = None


# ---------------------------------------------------------------------------
# Worktree management
# ---------------------------------------------------------------------------

def _create_worktree(task_id: str) -> Path:
    wt_path = WORKTREE_DIR / f"task-{task_id}"
    branch = f"orch/task-{task_id}"

    WORKTREE_DIR.mkdir(parents=True, exist_ok=True)

    # Clean stale worktree + branch
    subprocess.run(
        ["git", "worktree", "remove", "--force", str(wt_path)],
        cwd=str(ROOT), capture_output=True,
    )
    subprocess.run(
        ["git", "branch", "-D", branch],
        cwd=str(ROOT), capture_output=True,
    )

    result = subprocess.run(
        ["git", "worktree", "add", str(wt_path), "-b", branch, "HEAD"],
        cwd=str(ROOT), capture_output=True, text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(f"Worktree creation failed: {result.stderr}")
    return wt_path


def _merge_worktree(task_id: str) -> tuple[bool, str]:
    branch = f"orch/task-{task_id}"
    with _merge_lock:
        _MERGE_LOCK_FILE.parent.mkdir(parents=True, exist_ok=True)
        lock_fd = open(_MERGE_LOCK_FILE, "w")
        try:
            fcntl.flock(lock_fd, fcntl.LOCK_EX)

            stash_result = subprocess.run(
                ["git", "stash", "--include-untracked"],
                cwd=str(ROOT), capture_output=True, text=True,
            )
            stashed = "No local changes" not in stash_result.stdout

            result = subprocess.run(
                ["git", "merge", "--no-ff", "-m", f"merge: task {task_id}", branch],
                cwd=str(ROOT), capture_output=True, text=True,
            )
            if result.returncode != 0:
                subprocess.run(
                    ["git", "merge", "--abort"],
                    cwd=str(ROOT), capture_output=True,
                )
                if stashed:
                    subprocess.run(
                        ["git", "stash", "pop"],
                        cwd=str(ROOT), capture_output=True,
                    )
                return False, result.stderr + result.stdout

            if stashed:
                subprocess.run(
                    ["git", "stash", "pop"],
                    cwd=str(ROOT), capture_output=True,
                )
        finally:
            fcntl.flock(lock_fd, fcntl.LOCK_UN)
            lock_fd.close()
    return True, ""


def _cleanup_worktree(task_id: str) -> None:
    wt_path = WORKTREE_DIR / f"task-{task_id}"
    branch = f"orch/task-{task_id}"
    subprocess.run(
        ["git", "worktree", "remove", "--force", str(wt_path)],
        cwd=str(ROOT), capture_output=True,
    )
    subprocess.run(
        ["git", "branch", "-D", branch],
        cwd=str(ROOT), capture_output=True,
    )


def _has_commits(task_id: str, wt_path: Path) -> bool:
    if not wt_path.exists():
        return False
    result = subprocess.run(
        ["git", "log", "main..HEAD", "--oneline"],
        cwd=str(wt_path), capture_output=True, text=True,
    )
    return bool(result.stdout.strip())


def _preserve_worktree(task_id: str, wt_path: Path) -> None:
    branch = f"orch/task-{task_id}"
    log(f"  PRESERVED worktree: {wt_path}", task_id)
    log(f"  Branch: {branch} — inspect or cherry-pick manually", task_id)


def prune_worktrees() -> None:
    subprocess.run(["git", "worktree", "prune"], cwd=str(ROOT), capture_output=True)
    if WORKTREE_DIR.exists():
        for entry in WORKTREE_DIR.iterdir():
            if entry.is_dir() and entry.name.startswith("task-"):
                subprocess.run(
                    ["git", "worktree", "remove", "--force", str(entry)],
                    cwd=str(ROOT), capture_output=True,
                )
                tid = entry.name.removeprefix("task-")
                subprocess.run(
                    ["git", "branch", "-D", f"orch/task-{tid}"],
                    cwd=str(ROOT), capture_output=True,
                )


# ---------------------------------------------------------------------------
# Prompt building — CLEAN prompts, no accumulation
# ---------------------------------------------------------------------------

def _build_prompt(task: Task, dep_context: str = "") -> str:
    """Build clean implementation prompt. Called fresh for each attempt."""
    parts = [task.prompt.strip()]
    if task.context:
        parts.append(f"\n## Design Context\n{task.context.strip()}")
    if dep_context:
        parts.append(f"\n## Changes from dependency tasks\n{dep_context[:3000]}")
    return "\n\n".join(parts)


def _build_retry_prompt(task: Task, reason: str, details: str, attempt: int) -> str:
    """Build a CLEAN retry prompt — not appending to previous."""
    return f"""Fix the issue in this worktree and complete the task.

## Task
{task.title}

## Original requirement
{task.prompt.strip()[:800]}

## What went wrong (attempt {attempt})
{reason}

## Details
{details[-1000:]}

## Rules
- Do NOT rewrite from scratch — fix what exists
- Run `{task.test or 'cargo test'}` to verify
- Commit your changes with git
- Do NOT explain. WRITE CODE.
"""


def _get_dependency_context(task: Task, state: SprintState) -> str:
    parts = []
    task_map = {t.id: t for t in state.tasks}
    for dep_id in task.depends_on:
        dep = task_map.get(dep_id)
        if dep and dep.status == "passed" and dep.diff_summary:
            parts.append(f"### {dep.title} ({dep.id}):\n{dep.diff_summary}")
    return "\n\n".join(parts)


def _capture_diff_summary(task_id: str, wt_path: Path) -> str:
    _, base = run_git(["merge-base", "HEAD", "main"], cwd=wt_path)
    if base.strip():
        _, diff = run_git(["diff", "--stat", base.strip(), "HEAD"], cwd=wt_path)
        if diff.strip():
            return diff.strip()[:2000]
    _, log_out = run_git(["log", "main..HEAD", "--oneline", "--stat"], cwd=wt_path)
    return log_out.strip()[:2000]


def _commit_in_worktree(task: Task, wt_path: Path) -> None:
    """Commit any uncommitted changes (agent may have already committed)."""
    if not wt_path.exists():
        return
    subprocess.run(["git", "add", "-A"], cwd=str(wt_path), capture_output=True)
    check = subprocess.run(
        ["git", "diff", "--cached", "--quiet"],
        cwd=str(wt_path), capture_output=True,
    )
    if check.returncode != 0:
        scope = task.scope or "core"
        msg = f"{task.type}({scope}): {task.title.lower()}"
        subprocess.run(
            ["git", "commit", "-m", msg],
            cwd=str(wt_path), capture_output=True, text=True,
        )


# ---------------------------------------------------------------------------
# Single task execution — per-task retry loop
# ---------------------------------------------------------------------------

def _execute_task(task: Task, state: SprintState) -> None:
    tid = task.id

    # Create worktree
    try:
        wt_path = _create_worktree(tid)
    except RuntimeError as e:
        log(f"X Worktree failed: {e}", tid)
        task.status = "failed"
        task.error = str(e)
        task.finished_at = _now()
        with _state_lock:
            state.save()
        return

    log(f"Worktree ready, agent: {task.agent}", tid)
    task.started_at = _now()

    prompt = ""  # will be set in loop

    for attempt in range(1, MAX_TASK_RETRIES + 1):
        task.attempts = attempt
        task.status = "running"
        with _state_lock:
            state.save()

        if is_shutdown():
            with _state_lock:
                state.paused_at = _now()
                state.save()
            return

        # Build prompt — FRESH each attempt
        if attempt == 1:
            dep_context = _get_dependency_context(task, state)
            prompt = _build_prompt(task, dep_context)
        # else: prompt was set by retry logic below

        log(f"Attempt {attempt}/{MAX_TASK_RETRIES} (agent: {task.agent})...", tid)
        exit_code, output = run_agent(task.agent, prompt, cwd=wt_path, task_id=tid)

        if is_shutdown():
            with _state_lock:
                state.paused_at = _now()
                state.save()
            return

        # Timeout
        if exit_code != 0 and "TIMEOUT" in output:
            log("X TIMEOUT", tid)
            task.status = "failed"
            task.error = "Timeout"
            break

        # No code changes
        if not wt_path.exists() or not has_git_changes(wt_path):
            log("X NO CODE CHANGES", tid)
            task.error = "No code changes"
            if attempt < MAX_TASK_RETRIES:
                prompt = _build_retry_prompt(
                    task, "No code changes produced",
                    output[-1000:], attempt,
                )
                continue
            task.status = "failed"
            break

        # Commit uncommitted changes
        _commit_in_worktree(task, wt_path)

        # Run tests
        if task.test:
            log(f"Running tests: {task.test}", tid)
            test_code, test_output = run_tests(task.test, cwd=wt_path, task_id=tid)
            if test_code != 0:
                log("X Tests FAILED", tid)
                task.error = "Tests failed"
                if attempt < MAX_TASK_RETRIES:
                    prompt = _build_retry_prompt(
                        task, "Tests failed",
                        test_output[-1000:], attempt,
                    )
                    continue
                task.status = "failed"
                break

        # Capture diff for downstream tasks
        task.diff_summary = _capture_diff_summary(tid, wt_path)

        # Merge
        log("Merging to main...", tid)
        ok, err = _merge_worktree(tid)
        if ok:
            log("V Merged successfully", tid)
            _cleanup_worktree(tid)
            task.status = "passed"
            task.finished_at = _now()
            with _state_lock:
                state.save()
            return
        else:
            log(f"X Merge conflict: {err[:200]}", tid)
            task.error = "Merge conflict"
            if attempt < MAX_TASK_RETRIES:
                # Fresh worktree — must redo everything
                _cleanup_worktree(tid)
                try:
                    wt_path = _create_worktree(tid)
                except RuntimeError as e:
                    task.status = "failed"
                    task.error = str(e)
                    break
                dep_context = _get_dependency_context(task, state)
                prompt = _build_prompt(task, dep_context)
                continue
            task.status = "failed"
            break

    # Final cleanup
    if task.status != "passed":
        task.finished_at = _now()
        if wt_path.exists() and _has_commits(tid, wt_path):
            _preserve_worktree(tid, wt_path)
        else:
            _cleanup_worktree(tid)
        with _state_lock:
            state.save()


# ---------------------------------------------------------------------------
# Worker thread
# ---------------------------------------------------------------------------

def _worker(task_queue: queue.Queue, state: SprintState) -> None:
    while True:
        item = task_queue.get()
        if item is _SENTINEL:
            task_queue.task_done()
            break
        task: Task = item
        if is_shutdown():
            task_queue.task_done()
            break

        log(f">> Task {task.id}: {task.title} [agent: {task.agent}]", task.id)
        emit_event("task_start", task_id=task.id, extra={
            "title": task.title, "agent": task.agent,
        })
        t0 = time.time()
        _execute_task(task, state)
        elapsed = time.time() - t0
        emit_event("task_done", task_id=task.id, duration_s=elapsed, extra={
            "status": task.status, "attempts": task.attempts,
        }, error=task.error if task.status == "failed" else None)
        task_queue.task_done()


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------

def run_execute(state: SprintState) -> bool:
    """EXECUTE phase: parallel agents implement tasks in worktrees."""
    log(f"\n{'=' * 60}")
    log(f"  PHASE: EXECUTE")
    log(f"{'=' * 60}")

    state.current_pipeline_phase = "EXECUTE"
    state.save()

    tasks = state.tasks
    total = len(tasks)
    task_map = {t.id: t for t in tasks}

    # Worker count from independent tasks
    independent = [t for t in tasks if not t.depends_on and t.status == "pending"]
    workers = min(len(independent), MAX_WORKERS) if independent else 1
    log(f"  {total} tasks, {workers} workers")

    prune_worktrees()

    task_queue: queue.Queue = queue.Queue()
    enqueued: set[str] = set()

    # Start workers
    threads: list[threading.Thread] = []
    for i in range(workers):
        t = threading.Thread(
            target=_worker, args=(task_queue, state),
            daemon=True, name=f"worker-{i}",
        )
        t.start()
        threads.append(t)

    # Dispatcher loop
    try:
        while True:
            if is_shutdown():
                log("\n  STOP requested.")
                with _state_lock:
                    state.paused_at = _now()
                    state.save()
                break

            with _state_lock:
                for task in tasks:
                    if task.id in enqueued or task.status != "pending":
                        continue
                    deps_ok = all(
                        task_map[d].status == "passed"
                        for d in task.depends_on if d in task_map
                    )
                    deps_failed = any(
                        task_map[d].status in ("failed", "skipped")
                        for d in task.depends_on if d in task_map
                    )
                    if deps_failed:
                        task.status = "skipped"
                        task.error = f"Dependency failed: {task.depends_on}"
                        state.save()
                        continue
                    if deps_ok:
                        enqueued.add(task.id)
                        task_queue.put(task)

            all_done = all(
                t.status in ("passed", "failed", "skipped") for t in tasks
            )
            if all_done:
                break

            time.sleep(DISPATCH_INTERVAL)

    finally:
        for _ in threads:
            task_queue.put(_SENTINEL)
        for t in threads:
            t.join(timeout=10)

    passed = sum(1 for t in tasks if t.status == "passed")
    failed = sum(1 for t in tasks if t.status == "failed")
    skipped = sum(1 for t in tasks if t.status == "skipped")
    log(f"\n  EXECUTE results: {passed} passed, {failed} failed, {skipped} skipped")

    return failed == 0 and skipped == 0
