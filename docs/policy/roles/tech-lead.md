# Manifold — Tech Lead Guide

**Role:** Tech Lead (TL)
**Policy reference:** [Environments & Release Management Policy](../environments-policy.md)
**Last updated:** 2026-04-16

## Your accountability in E&RM

You do the operational work that keeps system records accurate and releases moving: you update environment economics, declare maintenance windows, confirm your system's readiness for releases, and raise advisories when you see technical blockers. The PO is accountable for release decisions; you are accountable for the technical facts that inform those decisions.

## What you do in Manifold

| Activity (RACI) | Where in Manifold | How |
|---|---|---|
| Configure environment economics (R) | `#/system/{id}` | Set and update cost per hour/day, refresh schedule, data config, scale notes |
| Request environment refresh — out-of-cycle (R) | `#/system/{id}` | Record the refresh request; PO is accountable for business justification |
| Confirm readiness — per system (R) | `#/release/{id}` | Confirm or object to readiness for each release your system is enrolled in |
| Mark release complete (C) | `#/release/{id}` | Consult with PO on technical completeness |
| Raise advisory — manual (R) | `#/advisories` | Raise advisories for technical blockers, integration risks, or gaps the reactive system misses |
| Dismiss or resolve advisory (C) | `#/advisories` | Provide input when DA or DIR is adjudicating advisories on your systems |
| Declare or cancel environment window (R) | `#/system/{id}` | Declare and manage maintenance or availability windows for your environments |

## Typical workflows

### Update environment cost and refresh configuration

Scenario: a vendor contract has changed, infrastructure costs have been repriced, or the standard refresh cadence needs adjustment.

1. Navigate to `#/system/{id}` for your system.
2. Open the relevant environment — click into it to see the configuration panel.
3. Update cost fields (per hour or per day), refresh schedule, data configuration notes, and scale details.
4. If the environment is shared across programs, verify the cost allocation basis is recorded in the notes. Unallocated environments are flagged for decommissioning.
5. Save. Dashboard cost rollups update immediately.

### Declare an environment window for planned maintenance

Scenario: you are scheduling downtime or a locked window and need other teams to know this environment is unavailable.

1. Navigate to `#/system/{id}`, then to the specific environment.
2. In the Environment Windows section, click **Declare Window**.
3. Set `window_type`: use `maintenance` for downtime, `locked` for change-freeze periods, `available` to explicitly advertise a slot.
4. Enter the purpose, start time, and end time.
5. Submit. The window appears in `#/schedule` for all teams to see.
6. If the window is cancelled, return to the same environment and click **Cancel Window**.

### Confirm readiness for a release

Scenario: the PO has requested readiness for a release that includes your system.

1. Navigate to `#/releases` — find the release in question, or follow a notification link directly to `#/release/{id}`.
2. Locate your system in the enrolled systems list. Status shows `readiness_requested`.
3. Assess your system's actual state: check active advisories (`#/advisories`), current blockers (system profile), and environment health.
4. If the system is ready: click **Confirm Readiness**. Status updates to `confirmed`.
5. If there is a blocker: click **Object**, enter the reason and severity. The PO sees this immediately. Do not confirm readiness you cannot stand behind — the release will be cleared by DIR based on this record.
6. Once the blocking condition is resolved (by any party), the PO or you can record the resolution and re-confirm.

### Raise an advisory for a technical blocker

Scenario: you have identified a technical risk that the reactive advisory system has not flagged — an undeclared integration, a known upstream instability, or a timing constraint.

1. Navigate to `#/advisories`, click **Raise Advisory**.
2. Select the affected environment (`env_id`), set `advisory_type` to `Manual`.
3. Write a clear `details` field: what the risk is, which systems are affected, and what would resolve it.
4. Set severity: `critical` if it blocks release, `warning` if it degrades confidence, `info` if informational only.
5. Submit. The advisory is visible immediately to the DA, PO, and DIR.
6. When the condition is resolved, return to the advisory and mark it resolved (or coordinate with DA who is accountable for disposition).

## What to monitor

- **`#/system/{id}` — your systems** — keep environment configs and blocker notes current. Stale blocker notes propagate as `BlockedUpstream` advisories to your dependents.
- **`#/advisories` — systems you own** — you are the first person who should know about a new advisory on your system.
- **`#/advisories` — `BlockedUpstream`** — if you have unresolved blockers, downstream teams are receiving automatic advisories. Resolve your blockers and update the notes to clear them.
- **`#/releases`** — watch for pending readiness requests. Leaving a readiness request unanswered holds up the release.
- **`#/schedule`** — verify your declared windows don't create unintended conflicts with releases that depend on your environments.

## Related

- [Policy](../environments-policy.md)
- [Full RACI matrix in the policy](../environments-policy.md#raci-matrix)
