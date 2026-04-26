# Implementation plans

Four phase-plans for the 4-week demo build. Each produces a coherent
shippable state. Read in order; each depends on the prior.

| Plan | Weeks | Done state |
|---|---|---|
| [Week 1 — Foundations](2026-04-26-week1-foundations.md) | Days 1-7 | Agent + `get_camera_frame` tool + 4-quadrant dashboard + tuned tracker + embedding logging. Basic NL Q&A about live frames works. *No map, no zones yet.* |
| Week 2 — Memory and zones | Days 8-14 | `map.db` schema live; `set_zone`, `set_alert_rule`, zone-event detector, `query_metric`, `query_history` MCP tools. Front-door dwell zone fires real alerts. |
| Week 3 — Map renderer | Days 15-21 | SVG map with all persistent + live + historical layers wired. Acts 1-3 demonstrable end-to-end with the map visible. |
| Week 4 — Polish and rehearsal | Days 22-28 | Predictor-mismatch overlay, second physical camera + OAK fusion, `get_metric_distance`, recorded backups, three rehearsals. |

## Dependency graph

```
Week 1 (foundations)
    │
    ├──► Week 2 (memory, zones, alerts)
    │       │
    │       └──► Week 3 (map renderer consumes map.db + live tracks)
    │               │
    │               └──► Week 4 (Act 4 + polish + rehearsal)
    │
    └──► Day 1 embedding-logging starts here, accumulates throughout
         (P0 — every day delayed is a day of "today" history Act 3 lacks)
```

## Source-of-truth

- Spec: `../spec.md`
- Build schedule (high-level): `../build-schedule.md`
- These plans: per-task detail with TDD steps, exact commands, commit
  messages

## Execution

Each plan is structured for either inline execution or subagent dispatch
per `superpowers:executing-plans` / `superpowers:subagent-driven-development`.
Steps use `- [ ]` checkboxes.
