# Conduit — Domain Architect Guide

**Role:** Domain Architect (DA)
**Policy reference:** [Environments & Release Management Policy](../environments-policy.md)
**Last updated:** 2026-04-16

## Your accountability in E&RM

You are accountable for the environment strategy and dependency correctness within your domain. You are the final decision-maker on system, environment, and dependency declarations for systems in your domain, and you own the resolution of advisories that affect those systems.

## What you do in Conduit

| Activity (RACI) | Where in Conduit | How |
|---|---|---|
| Declare system / environment / dependency (A) | `#/systems`, `#/system/{id}` | Review and approve what SAs register; ensure domain naming, tier, and watershed choices are correct |
| Configure environment economics (C) | `#/system/{id}` | Review cost, refresh cadence, and lead time settings that TLs/SAs set; flag discrepancies |
| Propose release (C) | `#/releases` | Consult on release scope and scheduling for systems in your domain |
| Confirm readiness — per system (C) | `#/release/{id}` | Provide input when TL readiness confirmation is unclear or contested |
| Raise advisory — manual (C) | `#/advisories` | Raise domain-level concerns that reactive policies won't catch |
| Dismiss or resolve advisory (A) | `#/advisories` | You are accountable for advisory disposition within your domain |
| Declare or cancel environment window (C) | `#/system/{id}` | Review maintenance windows declared by TLs for domain-level scheduling conflicts |
| Approve cross-utility shared environment (C) | `#/graph`, `#/system/{id}` | Provide domain-level input to EA on cross-utility sharing proposals |
| Authorize scheduling exception — within program (C) | `#/schedule` | Advise PO on domain implications |
| Authorize scheduling exception — cross-program (C) | `#/schedule`, `#/advisories` | Advise DIR; ensure domain systems are not silently displaced |
| Decommission unallocated environment (C) | `#/system/{id}`, `#/advisories` | Confirm which environments in your domain are safe to decommission |

## Typical workflows

### Review new system registrations in your domain

Scenario: an SA has registered a new system or added environments. You verify it meets domain standards before it is considered authoritative.

1. Navigate to `#/systems` — sort or search for recently added systems in your domain.
2. Click through to `#/system/{id}` — verify the `program`, `domain`, `criticality`, and `team` fields are correct.
3. Check the Environments section: confirm `tier` and `watershed` choices match domain topology conventions.
4. Check the Dependencies section: verify any declared dependencies target the correct tiers and are classified correctly (hard vs soft).
5. If corrections are needed, contact the SA. If registration is sound, no action is required — the record stands.

### Resolve advisories in your domain

Scenario: one or more advisories have been open in your domain and need disposition.

1. Navigate to `#/advisories` — filter or scan for open advisories on systems in your domain.
2. For each advisory, assess whether it represents a real problem or a known, accepted condition.
3. For real problems: coordinate with the SA or TL to fix the underlying condition (missing environment, undocumented interface, etc.). Once fixed, the advisory resolves automatically or click Resolve.
4. For accepted conditions: click Dismiss, and enter a clear reason. You are accountable for that record — make the reason auditable.

### Approve environment topology changes

Scenario: an SA wants to add a new tier, restructure environments, or propose a shared environment across programs.

1. Navigate to `#/system/{id}` for the affected system.
2. Review the current environment list, their `watershed` classification, and existing dependencies.
3. Navigate to `#/graph` to visualise downstream impact: which systems depend on this system's environments?
4. Check for open `MissingEnvironment`, `WatershedMismatch`, or `CircularDependency` advisories (`#/advisories`).
5. If cross-utility sharing is involved, escalate to EA for approval. Otherwise, confirm the change is acceptable and inform the SA.

### Confirm domain dependency correctness

Scenario: periodic check that all declared dependencies in your domain are accurate and resolved.

1. Navigate to `#/graph` — examine edges into and out of your domain's systems.
2. For each system with `UndocumentedInterface` or `MissingEnvironment` advisories, navigate to `#/system/{id}` and review the Dependencies section.
3. Coordinate with upstream/downstream SAs to get missing capabilities declared or missing environments added.
4. Dismiss or resolve advisories once the underlying data is corrected.

## What to monitor

- **`#/advisories` — your domain, all severities** — you are accountable for disposition. Don't let advisories go stale.
- **`#/advisories` — `CircularDependency`** — signals a structural dependency problem that may require topology redesign.
- **`#/advisories` — `UndocumentedInterface`** — indicates systems in your domain (or upstream) haven't declared provider capabilities.
- **`#/advisories` — `WatershedMismatch`** — prod-like environments depending on path-to-prod paths is a release risk.
- **`#/graph`** — regularly inspect the dependency topology for your domain; unexpected cross-domain links need investigation.
- **`#/systems`** — watch for new registrations that need review.

## Related

- [Policy](../environments-policy.md)
- [Full RACI matrix in the policy](../environments-policy.md#raci-matrix)
