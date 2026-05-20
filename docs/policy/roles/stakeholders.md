# Conduit — Stakeholders Guide

**Role:** Stakeholders (SH) — Business unit leads, CIDOs, compliance, enterprise users
**Policy reference:** [Environments & Release Management Policy](../environments-policy.md)
**Last updated:** 2026-04-16

## Your accountability in E&RM

You are primarily informed of changes that affect your programs or business units. You are occasionally consulted when a proposed change has material impact on your area. Conduit gives you a read-only view of environment health, release schedules, and active risks — without requiring you to understand the underlying event model.

## What you do in Conduit

| Activity (RACI) | Where in Conduit | How |
|---|---|---|
| Declare system / environment / dependency (I) | `#/` (Dashboard) | Informed via dashboard counts; no action required |
| Propose release (I) | `#/releases`, `#/schedule` | View proposed and scheduled releases for your programs |
| Confirm readiness — per system (I) | `#/release/{id}` | Informed of readiness status; escalate to PO if business concerns arise |
| Clear release for deployment (I) | `#/release/{id}` | Informed once DIR clears; no action required |
| Mark release complete (I) | `#/releases` | Informed when releases complete |
| Raise advisory — manual (I) | `#/advisories` | Informed of open advisories; contact DA or DIR if a risk requires escalation |
| Approve cross-utility shared environment (I) | `#/graph` | Informed of topology; consult EA or DIR if you have concerns |

## Typical workflows

### Review dashboard for program status

Scenario: you want a current picture of environment health and outstanding risks across your program.

1. Navigate to `#/` — the dashboard shows cost rollups by program, plus counts of systems, environments, dependencies, and open advisories.
2. High advisory counts indicate active risks. Click through to `#/advisories` for detail.
3. If you see something unexpected, contact the relevant DA (for system/environment concerns) or PO (for release concerns).

### Check upcoming releases affecting your programs

Scenario: planning cycle or stakeholder briefing and you need to know what is scheduled.

1. Navigate to `#/releases` — view all proposed and scheduled releases.
2. Click a release to see `#/release/{id}`: which systems are enrolled, current readiness status, and any open objections.
3. Navigate to `#/schedule` — the Gantt view shows all releases and refresh windows across programs. Identify where your programs have upcoming activity.

### Review advisories affecting your systems

Scenario: you have been informed that advisories are open on systems in your area and want to understand the risk.

1. Navigate to `#/advisories` — review open advisories. Note the type, severity, and affected environment.
2. `critical` advisories need immediate attention from the accountable team. Contact the DA for the domain or DIR if the advisory has cross-program impact.
3. `warning` and `info` advisories are being tracked; no immediate action is required from you unless they affect a release in your business calendar.

## What to monitor

- **`#/` (Dashboard) — advisory count for your programs** — a rising count before a release window is a signal to ask questions of your DA and PO.
- **`#/advisories` — `critical` severity** — these represent active blockers or risks. If they affect systems your business depends on, engage the accountable team promptly.
- **`#/schedule`** — check before quarterly planning to understand environment availability and upcoming release contention in your programs.
- **`#/releases`** — monitor releases that affect your business milestones. If a capex-linked release is at risk, escalate to the PO.

## Related

- [Policy](../environments-policy.md)
- [Full RACI matrix in the policy](../environments-policy.md#raci-matrix)
