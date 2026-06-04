# Manifold — Enterprise Architect Guide

**Role:** Enterprise Architect (EA)
**Policy reference:** [Environments & Release Management Policy](../environments-policy.md)
**Last updated:** 2026-04-16

## Your accountability in E&RM

You own enterprise environment strategy and reference topology. The RACI assigns you as the single accountable voice on cross-utility shared environment approvals, and you are consulted on cross-program scheduling exceptions.

## What you do in Manifold

| Activity (RACI) | Where in Manifold | How |
|---|---|---|
| Approve cross-utility shared environment (A) | `#/graph` and `#/system/{id}` | Review dependency topology and environment allocation; confirm the cost-allocation basis is documented before approval |
| Authorize scheduling exception — cross-program (C) | `#/schedule` and `#/advisories` | Review the Schedule Contention advisory and Gantt context; provide written input to DIR |
| Declare system / environment / dependency (C) | `#/systems` and `#/system/{id}` | Review and comment on topology proposals raised by SAs and DAs |

## Typical workflows

### Review a proposed cross-utility shared environment

Scenario: a Domain or Solution Architect proposes that two programs share an environment. DIR asks for your approval.

1. Navigate to `#/graph` to view the current dependency topology. Look for the systems involved and confirm the sharing relationship is architecturally sound.
2. Navigate to `#/system/{id}` for each system in question. Check the Environment section: verify `tier`, `watershed`, and that a cost-allocation basis is noted in the environment notes.
3. Confirm no `WatershedMismatch` or `CircularDependency` advisories are open for the affected environments (`#/advisories`).
4. If satisfied, confirm approval to DIR in writing. If not, raise a Manual advisory at `#/advisories` documenting the concern.

### Review a cross-program scheduling exception request

Scenario: two programs are competing for the same environment slot and DIR seeks your input.

1. Navigate to `#/schedule` — find the conflicting programs on the Gantt. Identify the release windows in contention.
2. Navigate to `#/advisories` — filter for `ScheduleContention` advisories on the relevant environments.
3. Review the priority ranking from the policy (regulatory > capex milestone > cross-program integration > program-internal).
4. Provide written recommendation to DIR. If the exception is warranted, note the precedent risk.

### Quarterly review of enterprise topology

Scenario: routine quarterly hygiene to maintain an accurate picture of the enterprise environment landscape.

1. Navigate to `#/graph` — inspect the 3D dependency graph for unexpected cross-domain connections, isolated nodes, or dense clusters that suggest coupling risk.
2. Navigate to `#/` (Dashboard) — check advisory counts across programs. High advisory volume in a domain signals governance gaps.
3. Navigate to `#/advisories` — review any open `CircularDependency` or `WatershedMismatch` advisories that remain unresolved. Escalate to the relevant DA if stale.
4. Document findings and any reference topology updates in your architecture records.

## What to monitor

- **`#/graph`** — watch for emerging cross-domain dependency patterns that cross utility boundaries without a documented allocation.
- **`#/advisories` — `CircularDependency`** — any cycle involving systems from different utilities requires your attention.
- **`#/advisories` — `WatershedMismatch`** — prod-like environments depending on path-to-prod paths is an architectural risk.
- **`#/` (Dashboard) — advisory counts by program** — a spike in open advisories may signal a structural environment problem rather than a one-off issue.
- **`#/schedule`** — before each planning cycle, verify no cross-program contention is building without a resolution path.

## Related

- [Policy](../environments-policy.md)
- [Full RACI matrix in the policy](../environments-policy.md#raci-matrix)
