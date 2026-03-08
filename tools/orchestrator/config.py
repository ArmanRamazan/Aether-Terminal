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
TEST_TIMEOUT = 300          # 5 min per test run
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
