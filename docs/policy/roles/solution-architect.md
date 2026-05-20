# Conduit — Solution Architect Guide

**Role:** Solution Architect (SA)
**Policy reference:** [Environments & Release Management Policy](../environments-policy.md)
**Last updated:** 2026-04-16

## Your accountability in E&RM

You own the accuracy of system metadata: what environments exist, what they depend on, what the system provides, and how those environments are costed and refreshed. You are the primary data entry role in Conduit for system and environment records, and you are responsible for raising advisories when you see risks the reactive system won't catch.

## What you do in Conduit

| Activity (RACI) | Where in Conduit | How |
|---|---|---|
| Declare system / environment / dependency (R) | `#/systems`, `#/system/{id}` | Register systems, add environments, declare dependency links |
| Configure environment economics (A) | `#/system/{id}` | Set cost per hour/day, refresh schedule, lead times; accountable for this data being correct |
| Confirm readiness — per system (C) | `#/release/{id}` | Provide architectural input when TL readiness is questioned |
| Raise advisory — manual (R) | `#/advisories` | Raise Manual advisories for architectural risks not covered by reactive policies |
| Dismiss or resolve advisory (R) | `#/advisories` | Resolve advisories for systems you own once the underlying condition is fixed |
| Declare or cancel environment window (A) | `#/system/{id}` | Declare planned maintenance or availability windows; accountable for window accuracy |
| Approve cross-utility shared environment (C) | `#/graph`, `#/system/{id}` | Provide input to EA on shared environment proposals |

## Typical workflows

### Register a new system

Scenario: a new system is being introduced to the program landscape and needs to be in Conduit.

1. Navigate to `#/systems`, click **Create System** (or use `#/upload` to extract details from a CSA/LSA document).
2. Fill in: name, program, domain, team, and criticality. Submit.
3. The system appears at `#/system/{id}`. Navigate there.
4. Add each environment: click **Add Environment**, specify name, `tier` (production/staging/development/test/dr), and `watershed` (prod/prod-like/path-to-prod).
5. Configure each environment's economics: cost per hour/day, refresh schedule, data configuration, scale notes.
6. Declare provider capabilities in the Capabilities section — this prevents `UndocumentedInterface` advisories on systems that depend on this one.
7. Submit a survey (`survey_submitted`) to mark the record as current.

### Declare system dependencies

Scenario: you need to record which upstream systems and environment tiers your system depends on during test or staging.

1. Navigate to `#/system/{id}` for your system.
2. Open the Dependencies section for the relevant environment.
3. For each dependency: select the upstream system, the tier you depend on (e.g., staging), and classify as hard or soft.
4. If the upstream system has no active environment at that tier, a `MissingEnvironment` advisory fires automatically.
5. If a prod-like environment depends on a path-to-prod tier, a `WatershedMismatch` advisory fires. Resolve the mismatch by adjusting the tier or the dependency classification.
6. Where a dependency is resolved by a mock or stub rather than a real environment, set the `resolution_type` accordingly (mocked/stubbed/null) to suppress false advisories.

### Configure environment cost and refresh

Scenario: cost allocation data or refresh cadences need updating (new contract, repricing, or schedule change).

1. Navigate to `#/system/{id}`, then to the specific environment.
2. Update the cost fields (cost per hour or per day), the consuming programs, and the refresh schedule.
3. If the environment is shared across programs, verify that the cost allocation basis is documented in the notes field — unallocated shared environments trigger decommissioning action.
4. Save. Changes take effect immediately in the dashboard cost rollups.

### Declare an environment window for planned maintenance

Scenario: a maintenance window is scheduled that will make an environment unavailable or locked.

1. Navigate to `#/system/{id}`, then to the specific environment.
2. In the Environment Windows section, click **Declare Window**.
3. Set `window_type` (available/locked/maintenance), purpose, start time, and end time.
4. Submit. Downstream dependents will see this window in the schedule view (`#/schedule`).
5. If plans change, return to the same view and cancel the window.

## What to monitor

- **`#/system/{id}` — your systems** — keep all environments and dependency records current. Stale data generates false advisories or misses real ones.
- **`#/advisories` — `UndocumentedInterface`** — means a system depending on yours can't validate the interface. Declare your provider capabilities to resolve it.
- **`#/advisories` — `MissingEnvironment`** — a dependency you declared can't be satisfied. Either the upstream system needs to add the environment or the dependency tier must change.
- **`#/advisories` — `WatershedMismatch`** — architectural risk requiring topology correction.
- **`#/` (Dashboard)** — confirm your systems' environments contribute to correct program-level cost totals.

## Related

- [Policy](../environments-policy.md)
- [Full RACI matrix in the policy](../environments-policy.md#raci-matrix)
