# Conduit — Director, Environment & Release Management Guide

**Role:** Director, Environment & Release Management (DIR)
**Policy reference:** [Environments & Release Management Policy](../environments-policy.md)
**Last updated:** 2026-04-16

## Your accountability in E&RM

You own this policy. You are the final decision-maker on clearing releases, resolving cross-program scheduling conflicts, and decommissioning unallocated environments. You are accountable for all advisory dispositions and responsible for approving cross-utility shared environments in coordination with the EA. Conduit is your primary operational tool — every surface is relevant to your role.

## What you do in Conduit

| Activity (RACI) | Where in Conduit | How |
|---|---|---|
| Clear release for deployment (A/R) | `#/release/{id}` | Review readiness record, resolve outstanding concerns, click Clear |
| Mark release complete (I) | `#/releases` | Informed once PO marks complete |
| Raise advisory — manual (A) | `#/advisories` | You are accountable for all manual advisories; raise when reactive policies miss a risk |
| Dismiss or resolve advisory (A) | `#/advisories` | Final disposition authority on all advisories |
| Declare or cancel environment window (C) | `#/system/{id}` | Consulted on windows with cross-program scheduling impact |
| Approve cross-utility shared environment (R) | `#/graph`, `#/system/{id}` | Responsible for the approval action; EA is accountable |
| Authorize scheduling exception — cross-program (A) | `#/schedule`, `#/advisories` | Adjudicate competing priority claims; document decision in an advisory |
| Decommission unallocated environment (A/R) | `#/system/{id}`, `#/advisories` | Identify via advisory, coordinate with DA/SA, remove environment |
| Request environment refresh — out-of-cycle (C) | `#/system/{id}` | Consulted when refresh requests have scheduling implications |
| Declare system / environment / dependency (I) | `#/systems` | Informed; intervene if domain governance appears to be failing |

## Typical workflows

### Clear a release for deployment

Scenario: a release has all readiness confirmations in place (or objections resolved) and requires your sign-off before deployment proceeds.

1. Navigate to `#/releases` — identify releases at status `proposed` where readiness has been fully worked through.
2. Click through to `#/release/{id}` — review the readiness section: confirm all enrolled systems are either `confirmed` or have recorded objection resolutions.
3. Check for open `critical` advisories on any enrolled system (`#/advisories`). A critical advisory should not be dismissed without documented rationale.
4. Confirm the release type against the policy priority order (regulatory > capex milestone > cross-program > program-internal > exploratory). If higher-priority work is contending for the same environment, resolve that first.
5. If satisfied: click **Clear Release**. Status moves to `cleared`. Deployment can proceed.
6. If not satisfied: raise a Manual advisory documenting what must be resolved before clearance.

### Adjudicate a cross-program scheduling conflict

Scenario: two programs are competing for the same environment or release slot and cannot resolve it themselves.

1. Navigate to `#/schedule` — identify the contending release windows on the Gantt.
2. Navigate to `#/advisories` — find the `ScheduleContention` advisory for the affected environment.
3. Apply the policy priority order: the higher-priority release wins. Confirm this with the release type and any capex milestone linkage.
4. Notify the lower-priority program: raise a Manual advisory on the affected release documenting the decision, the reason, and the rescheduling expectation.
5. If the situation is beyond your authority (cross-utility, regulatory, or General Counsel implications), escalate per the policy exceptions section.

### Review and act on advisories

Scenario: routine check of the advisory queue to ensure nothing is going stale or unaddressed.

1. Navigate to `#/advisories` — filter for `open` status. Sort by severity.
2. For each `critical` advisory: confirm the accountable DA is engaged. If it has been open more than a day without action, intervene directly.
3. For `warning` advisories older than the expected resolution time: contact the DA and ask for a disposition date.
4. For advisories that represent accepted ongoing conditions: dismiss with a clear, auditable reason. You are accountable for every dismissal.
5. For advisories that have been automatically resolved: verify the resolution is genuine (check the underlying system/environment record).

### Quarterly policy review using advisory data

Scenario: annual or material-change review of the E&RM policy, using Conduit data to surface patterns.

1. Navigate to `#/advisories` — filter by the past quarter. Review which advisory types are most frequent.
2. High frequency of `WatershedMismatch` → domain architects may need clearer topology guidance.
3. High frequency of `UndocumentedInterface` → provider capability declaration is not being done consistently; process enforcement is needed.
4. High frequency of `ScheduleContention` → the scheduling priority rules may need refinement, or programs are under-declaring their environment requirements.
5. High frequency of manual dismissals with thin rationale → the advisory is not useful; consider retiring or recalibrating the reactive policy threshold.
6. Navigate to `#/` (Dashboard) — review program-level cost totals. Flag any programs with environments that lack a clear payer (should have triggered `decommission` advisories already).
7. Update policy language where repeated exceptions indicate a rule that no longer fits practice. Document changes in the policy header.

## What to monitor

- **`#/advisories` — all open, especially `critical`** — nothing critical should be open for more than 24 hours without active engagement.
- **`#/releases`** — any release sitting at `proposed` with readiness requested but not progressing. These stall because a TL has not responded or an objection is unresolved.
- **`#/advisories` — `ScheduleContention`** — cross-program contention requires your direct involvement.
- **`#/` (Dashboard) — advisory counts trending up** — a rising count across programs signals systemic governance problems, not individual issues.
- **`#/schedule`** — before each planning cycle, ensure no unresolved contention exists going into the next release window.
- **`#/advisories` — unallocated environments** — any environment without a clear payer must be decommissioned. The advisory fires automatically; your job is to drive resolution.

## Related

- [Policy](../environments-policy.md)
- [Full RACI matrix in the policy](../environments-policy.md#raci-matrix)
