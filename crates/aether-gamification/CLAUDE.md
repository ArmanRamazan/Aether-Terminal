# aether-gamification

## Purpose
RPG mechanics layer. Calculates HP for processes, tracks XP/ranks for user, manages achievements, persists to SQLite.

## Modules
- `hp.rs` — HpEngine: calculates HP deltas based on process metrics
- `xp.rs` — XpTracker: accumulates XP, determines rank
- `achievements.rs` — AchievementTracker: definition and unlock tracking
- `storage.rs` — SqliteStorage implementing Storage trait from aether-core

## HP Rules
| Condition | HP Delta per second |
|-----------|-------------------|
| Memory growth > 5%/min | -1.0 |
| CPU > 90% | -2.0 |
| Zombie state | instant 0 |
| Healthy (no anomalies) | +0.5 (regen, cap 100) |

## XP Sources
| Source | XP Amount |
|--------|-----------|
| System uptime | +1/min |
| Arbiter approved action | +50 |
| Zombie killed | +10 |
| Anomaly auto-resolved | +5 |

## Ranks
Novice (0) → Operator (100) → Engineer (500) → Architect (2000) → Aether Lord (10000)

## Rules
- HP calculations are pure functions — take ProcessNode + delta time, return HP change
- XP tracker is stateful but serializable
- SQLite path: `~/.aether-terminal/data.db`
- Use `rusqlite` with `bundled` feature (no system SQLite dependency)
- Achievements: check conditions on each tick, emit GameEvent::AchievementUnlocked
- NEVER block on SQLite writes — use async wrapper or spawn_blocking

## Testing
```bash
cargo test -p aether-gamification
```
Test HP calculation rules, XP thresholds, achievement unlock logic, SQLite round-trip.

## Key Dependencies
- aether-core (path dependency)
- rusqlite (bundled feature)
