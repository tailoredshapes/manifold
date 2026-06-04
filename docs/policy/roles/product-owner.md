# Manifold — Product Owner Guide

**Role:** Product Owner (PO)
**Policy reference:** [Environments & Release Management Policy](../environments-policy.md)
**Last updated:** 2026-04-16

## Your accountability in E&RM

You own the release lifecycle for your program: scheduling it, confirming business readiness, resolving objections, and marking it complete. You are also accountable for within-program scheduling exceptions and out-of-cycle refresh requests. Manifold is where you create and drive releases from start to finish.

## What you do in Manifold

| Activity (RACI) | Where in Manifold | How |
|---|---|---|
| Propose release (A/R) | `#/releases` | Schedule a new release, set type, window, and description |
| Request environment refresh — out-of-cycle (A) | `#/system/{id}` | Accountable for the business case; TL performs the request |
| Confirm readiness — per system (A) | `#/release/{id}` | You are accountable for the overall readiness picture; TL confirms per system |
| Clear release for deployment (C) | `#/release/{id}` | Consult with DIR; ensure all readiness confirmations and objection resolutions are in place |
| Mark release complete (A/R) | `#/release/{id}` | Mark the release as complete once deployment is done |
| Raise advisory — manual (C) | `#/advisories` | Flag business-level risks the reactive system won't catch |
| Authorize scheduling exception — within program (A) | `#/schedule`, `#/advisories` | Approve schedule changes within your program; document reasoning |
| Authorize scheduling exception — cross-program (C) | `#/schedule` | Provide program impact information to DIR |

## Typical workflows

### Schedule a new release

Scenario: you have a feature set ready for release and need to register it in Manifold to begin coordination.

1. Navigate to `#/releases`, click **Schedule Release**.
2. Fill in: release name, type (routine_maintenance/program_release/infrastructure_change/emergency_fix/vendor_driven), description, target window start, and target window end.
3. Submit. The release appears in the releases list at status `proposed`.
4. Navigate to `#/schedule` to verify the window doesn't conflict with other program releases. If a `ScheduleContention` advisory fires, coordinate with DIR before proceeding.

### Enroll systems and request readiness

Scenario: the release is scheduled; you now need to pull in the relevant systems and trigger the readiness process.

1. Navigate to `#/release/{id}`.
2. In the Enrolled Systems section, click **Enroll System** for each system that is part of this release. Systems can be withdrawn later if scope changes.
3. Once systems are enrolled, click **Request Readiness** for each enrolled system. This triggers a `readiness_requested` event to the system's TL.
4. Track readiness status on the release detail screen. Each system shows: pending / confirmed / objected.
5. If a system's TL raises an objection, you see the reason and severity on the release detail. Coordinate resolution.

### Resolve objections and confirm release readiness

Scenario: one or more systems have objected to readiness. You need to work through the blockers.

1. Navigate to `#/release/{id}` — locate systems with status `objected`.
2. Review the objection reason and severity. Determine whether to:
   - Fix the blocking condition and ask the TL to re-confirm, or
   - Accept the risk and proceed (only appropriate for low-severity objections with documented rationale).
3. Once the blocking condition is resolved (by TL, SA, or external party), click **Resolve Objection**, enter the resolution description.
4. When all systems are confirmed or objections resolved, the release is ready for DIR to clear.

### Mark release complete

Scenario: deployment has completed and the release needs to be closed in Manifold.

1. Navigate to `#/release/{id}` — confirm status is `cleared`.
2. Verify that deployment is done and the "used and useful" determination can be made (relevant to capex milestone releases).
3. Click **Mark Complete**. Status moves to `completed`.
4. The dashboard cost and count metrics update automatically. Any capex milestone evidence is now in the event record.

## What to monitor

- **`#/` (Dashboard)** — check your program's system, environment, and advisory counts. Rising advisory counts before a release window are a signal to investigate.
- **`#/releases`** — track the status of all releases in your program. Nothing should sit at `proposed` without an active readiness process.
- **`#/advisories` — `BlockedUpstream`** — if an upstream system is blocked, your release may be at risk even if your systems are ready.
- **`#/advisories` — `ScheduleContention`** — contention on environments your release depends on requires immediate engagement with DIR.
- **`#/schedule`** — Gantt view shows your release window against all other program windows. Use this to spot conflicts before they become exceptions.

## Related

- [Policy](../environments-policy.md)
- [Full RACI matrix in the policy](../environments-policy.md#raci-matrix)
