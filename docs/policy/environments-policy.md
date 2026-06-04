# Environment & Release Management Policy

**Owner:** Director, Environment & Release Management
**Status:** Draft
**Last reviewed:** 2026-04-16
**Review cadence:** Annual

## Purpose

This policy defines how environments are funded and how conflicting scheduling demands are resolved across our programs. It exists to protect regulatory compliance, ensure milestone delivery, and make cost and scheduling decisions predictable.

## Roles and Responsibilities

| Role | Primary responsibility |
|---|---|
| **Enterprise Architect (EA)** | Enterprise environment strategy, reference topology, cross-domain architectural exceptions. |
| **Domain Architect (DA)** | Environment strategy within the domain. Dependency correctness between systems in the domain. |
| **Solution Architect (SA)** | Environment requirements for each solution. Declares dependencies and provider capabilities. Keeps system metadata current. |
| **Product Owner (PO)** | Release priority and feature milestones. Business readiness confirmation. "Used and useful" determination against capex milestones. |
| **Tech Lead (TL)** | Requests environments, refreshes, and windows. Declares technical blockers and readiness. |
| **Stakeholders (SH)** | Business units, CIDOs, compliance, enterprise users. Consulted on affecting changes; informed via the dashboard. |
| **Director, E&RM (DIR)** | Policy owner. Adjudicates scheduling conflicts. Authorizes exceptions within authority; escalates beyond. |

## RACI Matrix

**R** = Responsible (does the work) · **A** = Accountable (final say, one per row) · **C** = Consulted · **I** = Informed

| Activity | EA | DA | SA | PO | TL | SH | DIR |
|---|---|---|---|---|---|---|---|
| Declare system / environment / dependency | — | A | R | C | C | I | I |
| Configure environment economics (cost, refresh, lead time) | — | C | A | C | R | I | I |
| Request environment refresh (out-of-cycle) | — | — | C | A | R | I | C |
| Propose release | — | C | C | A/R | C | I | I |
| Confirm readiness (per system) | — | C | C | A | R | — | I |
| Clear release for deployment | — | — | C | C | C | I | A/R |
| Mark release complete | — | — | — | A/R | C | I | I |
| Raise advisory (manual) | — | C | R | C | R | I | A |
| Dismiss or resolve advisory | — | C | R | C | C | I | A |
| Declare or cancel environment window | — | C | A | C | R | I | C |
| Approve cross-utility shared environment | A | C | C | C | — | I | R |
| Authorize scheduling exception (within program) | — | C | C | A | C | I | I |
| Authorize scheduling exception (cross-program) | C | C | — | C | — | I | A |
| Decommission unallocated environment | — | C | C | I | C | I | A/R |

## Who Pays for Environments

**Principle:** Environments are funded by the utility that benefits from them. Cross-utility subsidization is not permitted.

1. **Dedicated environments** — paid entirely by the program using them, charged to the program it serves.
2. **Shared environments** — costs allocated across consuming programs proportional to usage, then charged through to each program's utility.
3. **Environments serving multiple utilities** — require explicit cost-allocation agreement before provisioning. Any environment shared across regulated entities must have a documented allocation basis (usage-based, seat-based, or agreed split) and be auditable.
4. **Unallocated environments are not permitted.** Any environment without a clear payer is flagged and scheduled for decommissioning.

Cost allocation data is tracked in the Environment record: cost per hour/day, consuming programs, and utility attribution.

## How Scheduling Is Prioritized

**Principle:** Milestones that trigger revenue recognition under our capex agreements take priority. Schedule resolution favors features that must be "used and useful" by a contractual date.

Priority order, highest to lowest:

1. **Regulatory and safety-mandated releases** — non-negotiable deadlines imposed by regulators.
2. **Capex milestone releases** — releases tied to a capex agreement's recognition milestone. Slippage here is a direct revenue event.
3. **Cross-program integration releases** — where multiple programs depend on coordinated delivery.
4. **Program-internal releases** — within a single program's control.
5. **Opportunistic or exploratory work** — scheduled only when higher-priority work is not competing for the same environment.

When two requests for the same environment conflict, the higher-priority request wins. The lower-priority request is rescheduled, never silently displaced — the program owner is notified with advisory detail.

## Request Lead Times

| Request | Minimum lead time |
|---|---|
| Dedicated environment | 4 weeks |
| Environment data refresh (standard cadence) | Per system's refresh schedule |
| Environment data refresh (out-of-cycle) | 48 hours or per system vendor agreement, whichever is longer |
| Shared environment booking | 2 weeks |
| Emergency fix window | Best effort, coordinated through E&RM |

## Exceptions and Escalation

Exceptions require written justification. Approval authority:
- Minor exceptions (within a program, no cross-utility or cross-program impact): program CIDO
- Cross-program or cross-utility impact: Sr. Director, Integration Platforms
- Regulatory or compliance implications: General Counsel notification required

Repeated exceptions for the same situation indicate the policy needs revision, not more exceptions.

## Governance

Policy owned by the Director, Environment & Release Management. Reviewed annually or on material change to capex agreements, regulatory framework, or organizational structure. Advisory data and scheduling records from Manifold inform each review.
