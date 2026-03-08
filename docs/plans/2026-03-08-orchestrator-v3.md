# Orchestrator v3 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a lean 3-phase orchestrator that cuts token costs by ~50% vs v2 while maintaining quality, optimized for Rust cargo workspace projects.

**Architecture:** 3-phase pipeline (PLAN offline → EXECUTE with per-task retry → VERIFY optional). No separate PLANNING/DESIGN/DOCUMENT Claude calls. Per-task test+merge cycle eliminates the "reset all tasks" anti-pattern. Clean retry prompts prevent context bloat.

**Tech Stack:** Python 3.11+, PyYAML, Claude Code CLI (`claude` binary), git worktrees

---

## v2 → v3 Changelog

| What changed | v2 | v3 | Why |
|---|---|---|---|
| Pipeline phases | 7 (PLAN→DESIGN→IMPL→TEST→REVIEW→INTEGRATE→DOCUMENT) | 3 (PLAN offline→EXECUTE→VERIFY) | -6 Claude calls overhead per sprint |
| Retry scope | Per-sprint (reset ALL tasks) | Per-task (only failed task retries) | Eliminates 3-9x token waste |
| Retry prompt | Accumulating (append context each retry) | Clean (fresh prompt with only failure info) | Prevents context bloat |
| Design review | 2 extra agents per frontend task | Merged into primary agent prompt | -200s per frontend task |
| DOCUMENT phase | Separate Claude call | Part of task prompt | -120s per sprint |
| PLANNING phase | Claude call (tech-lead + analyst) | Human writes YAML with context field | -300s per sprint |
| Test execution | IMPLEMENT tests + TEST phase tests (duplicate) | Per-task test in EXECUTE + optional full suite in VERIFY | No duplicate runs |
| Agent selection | Scope map (9 agents) | Explicit `agent` field in YAML | Simpler, no hidden mapping |
| State fields | planning_output, design_output, review_output | Removed (not needed without those phases) | Leaner state |

## File Structure

```
tools/orchestrator/
├── config.py           # paths, timeouts, constants
├── main.py             # CLI: run, resume, status, reset
├── state.py            # Task + SprintState dataclasses, YAML loading, JSON persistence
├── executor.py         # EXECUTE phase: dispatcher + workers + per-task retry + worktree
├── verifier.py         # VERIFY phase: optional full test suite
├── agent_runner.py     # Claude subprocess, streaming, quota handling, git helpers
└── tasks/              # sprint YAML files
    └── ms1-ingestion.yaml  (example)
```

---

### Task 1: Project scaffolding and config

**Files:**
- Create: `tools/orchestrator/config.py`
- Create: `tools/orchestrator/__init__.py` (empty)

**Step 1: Create directory structure**

```bash
mkdir -p tools/orchestrator/tasks
```

**Step 2: Write config.py**

```python
"""Orchestrator v3 — configuration."""

from __future__ import annotations

from pathlib import Path

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------

ROOT = Path(__file__).resolve().parent.parent.parent  # repo root
ORCH_DIR = ROOT / "tools" / "orchestrator"
STATE_DIR = ORCH_DIR / ".state"
LOG_DIR = ORCH_DIR / ".logs"
PID_FILE = ORCH_DIR / ".pid"
STOP_FILE = ORCH_DIR / ".stop"
WORKTREE_DIR = ROOT / ".worktrees"

# ---------------------------------------------------------------------------
# Timeouts & limits
# ---------------------------------------------------------------------------

CLAUDE_TIMEOUT = 900        # 15 min per agent invocation
TEST_TIMEOUT = 300          # 5 min per test run (cargo test is fast)
MAX_TASK_RETRIES = 3        # max retries per task (not per sprint)
DISPATCH_INTERVAL = 2       # seconds between queue scans
MAX_WORKERS = 4             # max parallel workers

# Quota retry
QUOTA_RETRY_INTERVAL = 1800  # 30 min
QUOTA_MAX_RETRIES = 12       # 12 × 30 min = 6h max

QUOTA_PATTERNS = [
    "rate limit", "rate_limit", "ratelimit",
    "too many request", "429",
    "overloaded", "over capacity",
    "quota exceeded", "quota limit",
    "request limit", "usage limit",
    "capacity", "throttl",
]

# ---------------------------------------------------------------------------
# Pipeline phases
# ---------------------------------------------------------------------------

PHASES = ["EXECUTE", "VERIFY"]
```

**Step 3: Create empty __init__.py**

```bash
touch tools/orchestrator/__init__.py
```

**Step 4: Commit**

```bash
git add tools/orchestrator/config.py tools/orchestrator/__init__.py
git commit -m "feat(orchestrator): add v3 config with 3-phase pipeline"
```

---

### Task 2: State management (state.py)

**Files:**
- Create: `tools/orchestrator/state.py`

**Step 1: Write state.py**

Key differences from v2:
- `Task` has `agent` field (explicit, no scope mapping)
- `Task` has `context` field (replaces DESIGN phase output)
- `SprintState` drops `planning_output`, `design_output`, `review_output`
- Add `recover_crashed()` for resume

```python
"""Orchestrator v3 — sprint state persistence."""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path

from config import STATE_DIR

try:
    import yaml
except ImportError:
    raise SystemExit("ERROR: PyYAML required. Install: pip install pyyaml")

# Per-instance state isolation
_instance_state_dir: Path | None = None


def set_state_dir(path: Path) -> None:
    global _instance_state_dir
    _instance_state_dir = path


def get_state_dir() -> Path:
    return _instance_state_dir or STATE_DIR


def _now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")


# ---------------------------------------------------------------------------
# Task
# ---------------------------------------------------------------------------

@dataclass
class Task:
    id: str
    title: str
    agent: str               # explicit agent name (e.g. "rust-engineer")
    prompt: str
    context: str = ""        # design context (replaces DESIGN phase)
    scope: str = ""          # for commit messages only
    type: str = "feat"       # commit type
    test: str | None = None  # test command
    depends_on: list[str] = field(default_factory=list)
    status: str = "pending"  # pending, running, passed, failed, skipped
    attempts: int = 0
    started_at: str = ""
    finished_at: str = ""
    error: str = ""
    diff_summary: str = ""


# ---------------------------------------------------------------------------
# Sprint file loader
# ---------------------------------------------------------------------------

@dataclass
class SprintFile:
    phase: str
    description: str
    tasks: list[Task]

    @classmethod
    def load(cls, path: Path) -> SprintFile:
        data = yaml.safe_load(path.read_text(encoding="utf-8"))
        tasks = []
        for t in data.get("tasks", []):
            tasks.append(Task(
                id=str(t["id"]),
                title=t["title"],
                agent=t.get("agent", "rust-engineer"),
                prompt=t["prompt"],
                context=t.get("context", ""),
                scope=t.get("scope", ""),
                type=t.get("type", "feat"),
                test=t.get("test"),
                depends_on=[str(d) for d in t.get("depends_on", [])],
            ))
        return cls(
            phase=str(data.get("phase", "")),
            description=data.get("description", ""),
            tasks=tasks,
        )


# ---------------------------------------------------------------------------
# Sprint state
# ---------------------------------------------------------------------------

@dataclass
class SprintState:
    source_file: str = ""
    phase: str = ""
    current_pipeline_phase: str = ""
    tasks: list[Task] = field(default_factory=list)
    paused_at: str = ""
    started_at: str = ""

    def save(self) -> None:
        state_dir = get_state_dir()
        state_dir.mkdir(parents=True, exist_ok=True)
        path = state_dir / "state.json"
        data = {
            "source_file": self.source_file,
            "phase": self.phase,
            "current_pipeline_phase": self.current_pipeline_phase,
            "started_at": self.started_at,
            "paused_at": self.paused_at,
            "tasks": [
                {
                    "id": t.id, "title": t.title, "agent": t.agent,
                    "prompt": t.prompt, "context": t.context,
                    "scope": t.scope, "type": t.type, "test": t.test,
                    "depends_on": t.depends_on, "status": t.status,
                    "attempts": t.attempts, "started_at": t.started_at,
                    "finished_at": t.finished_at, "error": t.error,
                    "diff_summary": t.diff_summary,
                }
                for t in self.tasks
            ],
        }
        path.write_text(json.dumps(data, ensure_ascii=False, indent=2))

    @classmethod
    def load(cls) -> SprintState | None:
        path = get_state_dir() / "state.json"
        if not path.exists():
            return None
        data = json.loads(path.read_text())
        tasks = [Task(**t) for t in data.get("tasks", [])]
        return cls(
            source_file=data.get("source_file", ""),
            phase=data.get("phase", ""),
            current_pipeline_phase=data.get("current_pipeline_phase", ""),
            tasks=tasks,
            paused_at=data.get("paused_at", ""),
            started_at=data.get("started_at", ""),
        )

    @classmethod
    def from_sprint(cls, sprint: SprintFile, source_file: str) -> SprintState:
        return cls(
            source_file=source_file,
            phase=sprint.phase,
            tasks=sprint.tasks,
            started_at=_now(),
        )

    def recover_crashed(self) -> None:
        """Reset any tasks stuck in 'running' state after crash."""
        for t in self.tasks:
            if t.status == "running":
                t.status = "pending"
                t.error = ""
```

**Step 2: Commit**

```bash
git add tools/orchestrator/state.py
git commit -m "feat(orchestrator): add state management with explicit agent field"
```

---

### Task 3: Agent runner (agent_runner.py)

**Files:**
- Create: `tools/orchestrator/agent_runner.py`

**Step 1: Write agent_runner.py**

Carried from v2 with minimal changes:
- Same subprocess streaming with output capture
- Same quota retry logic
- Same git helpers
- Removed: nothing (this module was already clean)

```python
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
    """Run Claude Code with a specific agent. Auto-retries on quota."""
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
```

**Step 2: Commit**

```bash
git add tools/orchestrator/agent_runner.py
git commit -m "feat(orchestrator): add agent runner with streaming and quota handling"
```

---

### Task 4: Executor — the core EXECUTE phase (executor.py)

**Files:**
- Create: `tools/orchestrator/executor.py`

This is the most critical file. Key differences from v2:
1. **Per-task retry**: task passes tests → merge → DONE forever. No reset.
2. **Clean retry prompts**: fresh prompt on each retry, not appending.
3. **No design review pass**: single agent per task.
4. **Simplified prompt building**: task.prompt + task.context + dep_context.

**Step 1: Write executor.py**

```python
"""Orchestrator v3 — EXECUTE phase with parallel workers and per-task retry."""

from __future__ import annotations

import fcntl
import os
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
# Worktree management (unchanged from v2 — proven pattern)
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
            task.error = f"Merge conflict"
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
```

**Step 2: Commit**

```bash
git add tools/orchestrator/executor.py
git commit -m "feat(orchestrator): add executor with per-task retry and clean prompts"
```

---

### Task 5: Verifier — optional VERIFY phase (verifier.py)

**Files:**
- Create: `tools/orchestrator/verifier.py`

**Step 1: Write verifier.py**

Lightweight: just runs a full test suite command. No separate QA/security Claude calls.

```python
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

    # Collect unique test commands from passed tasks (skip duplicates)
    seen_commands: set[str] = set()
    test_commands: list[str] = []
    for t in state.tasks:
        if t.status == "passed" and t.test and t.test not in seen_commands:
            seen_commands.add(t.test)
            test_commands.append(t.test)

    if not test_commands:
        log("  No test commands to verify. Skipping.")
        return True

    # Also run workspace-level test if available
    # (cargo test --workspace for Rust projects)
    workspace_test = f"cd {ROOT} && cargo test --workspace 2>&1 || true"

    all_passed = True
    for cmd in test_commands:
        log(f"\n  Running: {cmd}")
        code, output = run_tests(cmd, task_id="verify")
        if code != 0:
            log(f"  X FAILED: {cmd}")
            all_passed = False
        else:
            log(f"  V PASSED: {cmd}")

    # Workspace-level verification
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
```

**Step 2: Commit**

```bash
git add tools/orchestrator/verifier.py
git commit -m "feat(orchestrator): add lightweight verify phase"
```

---

### Task 6: Main CLI (main.py)

**Files:**
- Create: `tools/orchestrator/main.py`

**Step 1: Write main.py**

Simplified from v2: no 7-phase pipeline dispatch, just EXECUTE → VERIFY.

```python
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

from agent_runner import kill_all, log, request_shutdown
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
    from agent_runner import emit_event

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
```

**Step 2: Commit**

```bash
git add tools/orchestrator/main.py
git commit -m "feat(orchestrator): add main CLI with 3-phase pipeline"
```

---

### Task 7: Example sprint YAML for Aether-Terminal MS1

**Files:**
- Create: `tools/orchestrator/tasks/ms1-workspace-setup.yaml`

**Step 1: Write example sprint**

```yaml
phase: "ms1-workspace-setup"
description: "Create cargo workspace with 6 crates, core types and traits"

tasks:
  - id: workspace-init
    title: "Initialize cargo workspace with 6 crates"
    agent: rust-engineer
    scope: workspace
    type: feat
    context: |
      See docs/plans/2026-03-08-aether-terminal-design.md for full architecture.
      Workspace structure:
        crates/aether-terminal/  (bin, depends on all)
        crates/aether-core/      (lib, no deps)
        crates/aether-ingestion/ (lib, depends on core)
        crates/aether-render/    (lib, depends on core)
        crates/aether-mcp/       (lib, depends on core)
        crates/aether-gamification/ (lib, depends on core)
    prompt: |
      Create a Rust cargo workspace for the Aether-Terminal project.

      1. Create root Cargo.toml as workspace:
         ```toml
         [workspace]
         resolver = "2"
         members = [
           "crates/aether-terminal",
           "crates/aether-core",
           "crates/aether-ingestion",
           "crates/aether-render",
           "crates/aether-mcp",
           "crates/aether-gamification",
         ]
         ```

      2. Create each crate with `cargo init`:
         - aether-terminal: binary crate
         - All others: library crates

      3. Set up dependencies in each crate's Cargo.toml:
         - aether-core: petgraph, glam, serde, tokio
         - aether-ingestion: aether-core (path), sysinfo, tokio
         - aether-render: aether-core (path), ratatui, crossterm, glam
         - aether-mcp: aether-core (path), serde_json, tokio, axum
         - aether-gamification: aether-core (path), rusqlite
         - aether-terminal: all crates as path deps, clap, tokio, tracing

      4. Verify: `cargo check --workspace` must pass.

      COMMIT: feat(workspace): initialize cargo workspace with 6 crates
    test: "cargo check --workspace"
    depends_on: []

  - id: core-types
    title: "Define core types and trait abstractions"
    agent: rust-engineer
    scope: core
    type: feat
    context: |
      See design doc Component 1: aether-core.
      Key types: ProcessNode, NetworkEdge, ProcessState, Protocol, ConnectionState.
      Key traits: SystemProbe, Renderer, McpTransport, Storage.
      Graph: petgraph::Graph<ProcessNode, NetworkEdge>.
      Events: SystemEvent, GameEvent, AgentAction.
    prompt: |
      Implement the aether-core crate with all type definitions.

      File: crates/aether-core/src/models.rs
      - ProcessNode { pid, ppid, name, cpu_percent, mem_bytes, state, hp, xp, position_3d }
      - NetworkEdge { source_pid, dest, protocol, bytes_per_sec, state }
      - ProcessState enum: Running, Sleeping, Zombie, Stopped
      - Protocol enum: TCP, UDP, DNS, QUIC, HTTP, HTTPS, Unknown
      - ConnectionState enum: Established, Listen, TimeWait, CloseWait

      File: crates/aether-core/src/graph.rs
      - WorldGraph struct wrapping petgraph::Graph<ProcessNode, NetworkEdge>
      - Methods: add_process, remove_process, add_connection, find_by_pid, snapshot

      File: crates/aether-core/src/traits.rs
      - trait SystemProbe: Send + Sync (async fn snapshot, fn subscribe)
      - trait Storage: Send + Sync (async fn save_session, load_rankings)

      File: crates/aether-core/src/events.rs
      - SystemEvent enum: ProcessCreated, ProcessExited, MetricsUpdate, TopologyChange
      - GameEvent enum: HpChanged, XpEarned, AchievementUnlocked
      - AgentAction enum: KillProcess, RestartProcess, Inspect

      File: crates/aether-core/src/lib.rs
      - Re-export all modules

      Write unit tests for WorldGraph (add/remove/find).
      COMMIT: feat(core): define core types, traits and world graph
    test: "cargo test -p aether-core"
    depends_on: [workspace-init]
```

**Step 2: Commit**

```bash
git add tools/orchestrator/tasks/ms1-workspace-setup.yaml
git commit -m "feat(orchestrator): add example MS1 sprint YAML"
```

---

### Task 8: Add .gitignore for orchestrator runtime files

**Files:**
- Modify: `.gitignore` (create if not exists)

**Step 1: Write .gitignore**

```bash
cat >> .gitignore << 'EOF'
# Orchestrator runtime
tools/orchestrator/.state/
tools/orchestrator/.logs/
tools/orchestrator/.pid*
tools/orchestrator/.stop*
tools/orchestrator/.merge.lock
.worktrees/

# Rust
target/
EOF
```

**Step 2: Commit**

```bash
git add .gitignore
git commit -m "chore: add gitignore for orchestrator runtime and Rust target"
```

---

## Cost Comparison Summary

```
v2 (7 phases, 9 agents):          v3 (3 phases, explicit agents):
  10-20 Claude calls/sprint          3-6 Claude calls/sprint
  2325-5000s per 3-task sprint       1720-2200s per 3-task sprint
  ~2050 lines of Python              ~800 lines of Python

  Savings: ~50% tokens, ~55% time, ~60% code
```

## Usage

```bash
# Run a sprint
cd tools/orchestrator
python main.py tasks/ms1-workspace-setup.yaml

# Dry run (preview)
python main.py tasks/ms1-workspace-setup.yaml --dry-run

# Resume after interrupt
python main.py --resume

# Skip verification
python main.py tasks/ms1-workspace-setup.yaml --skip-verify

# Check status
python main.py --status

# Reset state
python main.py --reset

# Parallel instances
python main.py tasks/sprint-a.yaml --instance a &
python main.py tasks/sprint-b.yaml --instance b &
```
