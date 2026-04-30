#!/usr/bin/env python3
"""
Seed script for Manifold — Meridian Freight Solutions test dataset.

Company: Meridian Freight Solutions (~250 employees)
  Industry: Logistics / last-mile freight delivery
  Scale: 30 systems, 2 internal dev teams, 3 agencies

Run after `docker-compose up`:
  python3 data/seed_meridian.py

Idempotent: checks for existing data by name before creating.
"""

import json
import sys
import urllib.request
import urllib.error
from typing import Optional

# ── Service URLs ───────────────────────────────────────────────────────────────

GROUNDWORK = "http://localhost:3050"
UNION      = "http://localhost:3051"
CITYHALL   = "http://localhost:3052"
YARD       = "http://localhost:3053"

# ── Helpers ────────────────────────────────────────────────────────────────────

def post(base: str, path: str, payload: dict) -> dict:
    url = f"{base}{path}"
    data = json.dumps(payload).encode()
    req = urllib.request.Request(url, data=data, headers={"Content-Type": "application/json"}, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=10) as r:
            return json.loads(r.read())
    except urllib.error.HTTPError as e:
        body = e.read().decode()
        print(f"  ERROR {e.code} POST {url}: {body[:200]}", file=sys.stderr)
        raise

def get_all(base: str, path: str) -> list:
    url = f"{base}{path}"
    try:
        with urllib.request.urlopen(url, timeout=10) as r:
            return json.loads(r.read()) or []
    except Exception:
        return []

def create(base: str, path: str, payload: dict, name_key: str = "name") -> str:
    """Create entity, return its id. Skips if name already exists."""
    existing = get_all(base, path)
    name = payload.get(name_key, "")
    for e in existing:
        if e.get(name_key) == name:
            print(f"  skip (exists): {name}")
            return e["id"]
    result = post(base, path, payload)
    id_ = result["id"]
    print(f"  created: {name} → {id_}")
    return id_

def section(title: str):
    print(f"\n{'─'*60}")
    print(f"  {title}")
    print(f"{'─'*60}")

# ── IDs registry ───────────────────────────────────────────────────────────────
ids = {}

# ══════════════════════════════════════════════════════════════════════════════
# UNION — Teams & People
# ══════════════════════════════════════════════════════════════════════════════

section("UNION: Teams")

# Internal teams
ids["team-orion"] = create(UNION, "/team/api", {
    "name": "Team Orion",
    "kind": "product",
    "description": "Product engineering — customer-facing apps, routing, driver experience",
})
ids["team-anvil"] = create(UNION, "/team/api", {
    "name": "Team Anvil",
    "kind": "platform",
    "description": "Platform engineering — shared services, data, infrastructure",
})

# Agency teams
ids["apex-digital"] = create(UNION, "/team/api", {
    "name": "Apex Digital",
    "kind": "support",
    "description": "Agency — staff aug on OMS, customer portal, and tracking",
})
ids["northbridge-tech"] = create(UNION, "/team/api", {
    "name": "NorthBridge Tech",
    "kind": "support",
    "description": "Agency — staff aug on analytics, ETL, and data platform",
})
ids["devstar"] = create(UNION, "/team/api", {
    "name": "DevStar",
    "kind": "support",
    "description": "Agency — legacy CRM modernisation and old driver app sunset",
})

section("UNION: People")

people = [
    # Team Orion
    ("Sarah Chen",     "sarah.chen@meridianfreight.com",     "Tech Lead",       "team-orion"),
    ("Marcus Webb",    "marcus.webb@meridianfreight.com",     "Senior Engineer", "team-orion"),
    ("Priya Patel",    "priya.patel@meridianfreight.com",     "Engineer",        "team-orion"),
    ("Jake Torres",    "jake.torres@meridianfreight.com",     "Engineer",        "team-orion"),
    ("Emma Liu",       "emma.liu@meridianfreight.com",        "Engineer",        "team-orion"),
    # Team Anvil
    ("David Kim",      "david.kim@meridianfreight.com",       "Tech Lead",       "team-anvil"),
    ("Aisha Johnson",  "aisha.johnson@meridianfreight.com",   "Senior Engineer", "team-anvil"),
    ("Ryan Murphy",    "ryan.murphy@meridianfreight.com",     "Engineer",        "team-anvil"),
    ("Fatima Hassan",  "fatima.hassan@meridianfreight.com",   "Engineer",        "team-anvil"),
    # Apex Digital
    ("Chris Blake",    "cblake@apexdigital.io",              "Agency Lead",     "apex-digital"),
    ("Nina Kowalski",  "nkowalski@apexdigital.io",           "Senior Dev",      "apex-digital"),
    ("Omar Siddiqui",  "osiddiqui@apexdigital.io",           "Dev",             "apex-digital"),
    # NorthBridge Tech
    ("Laura Strand",   "lstrand@northbridgetech.com",        "Agency Lead",     "northbridge-tech"),
    ("Ben Nakamura",   "bnakamura@northbridgetech.com",      "Data Engineer",   "northbridge-tech"),
    ("Cleo Martins",   "cmartins@northbridgetech.com",       "Data Engineer",   "northbridge-tech"),
    # DevStar
    ("Alex Foster",    "afoster@devstar.co",                 "Agency Lead",     "devstar"),
    ("Mia Reyes",      "mreyes@devstar.co",                  "Senior Dev",      "devstar"),
]

for name, contact, role, team_key in people:
    slug = name.lower().replace(" ", "-")
    ids[f"person-{slug}"] = create(UNION, "/person/api", {
        "name": name, "contact": contact, "role": role,
    })

section("UNION: Team Members")

memberships = [
    # Orion
    ("person-sarah-chen",    "team-orion",        "Tech Lead"),
    ("person-marcus-webb",   "team-orion",        "Senior Engineer"),
    ("person-priya-patel",   "team-orion",        "Engineer"),
    ("person-jake-torres",   "team-orion",        "Engineer"),
    ("person-emma-liu",      "team-orion",        "Engineer"),
    # Anvil
    ("person-david-kim",     "team-anvil",        "Tech Lead"),
    ("person-aisha-johnson", "team-anvil",        "Senior Engineer"),
    ("person-ryan-murphy",   "team-anvil",        "Engineer"),
    ("person-fatima-hassan", "team-anvil",        "Engineer"),
    # Apex Digital
    ("person-chris-blake",   "apex-digital",      "Agency Lead"),
    ("person-nina-kowalski", "apex-digital",      "Senior Dev"),
    ("person-omar-siddiqui", "apex-digital",      "Dev"),
    # NorthBridge
    ("person-laura-strand",  "northbridge-tech",  "Agency Lead"),
    ("person-ben-nakamura",  "northbridge-tech",  "Data Engineer"),
    ("person-cleo-martins",  "northbridge-tech",  "Data Engineer"),
    # DevStar
    ("person-alex-foster",   "devstar",           "Agency Lead"),
    ("person-mia-reyes",     "devstar",           "Senior Dev"),
]

existing_members = get_all(UNION, "/team_member/api")
existing_pairs = {(m["person_id"], m["team_id"]) for m in existing_members}

for person_key, team_key, role in memberships:
    pid = ids[person_key]
    tid = ids[team_key]
    if (pid, tid) in existing_pairs:
        print(f"  skip (exists): {person_key} → {team_key}")
        continue
    result = post(UNION, "/team_member/api", {"person_id": pid, "team_id": tid, "role": role})
    print(f"  created: {person_key} → {team_key}")

# ══════════════════════════════════════════════════════════════════════════════
# GROUNDWORK — Deployables
# ══════════════════════════════════════════════════════════════════════════════

section("GROUNDWORK: Deployables")

#
# Each entry: (slug, name, description, repo, team_key, release_cadence_note)
#
deployables = [
    # ── Core systems ─────────────────────────────────────────────────────────
    ("route-optimizer",
     "Route Optimizer",
     "Core routing engine — optimises last-mile delivery paths in real time. Weekly releases. Full test env stack.",
     "https://github.com/meridian-internal/route-optimizer",
     "team-orion"),

    ("order-management",
     "Order Management System",
     "Central OMS handling order intake, status, and fulfilment workflow. Bi-weekly releases. Dev + UAT envs.",
     "https://github.com/meridian-internal/order-management",
     "team-orion"),

    ("driver-api",
     "Driver API",
     "Mobile backend for driver apps — job assignment, proof of delivery, GPS telemetry. Weekly releases.",
     "https://github.com/meridian-internal/driver-api",
     "team-orion"),

    # ── Customer-facing ───────────────────────────────────────────────────────
    ("customer-portal",
     "Customer Portal",
     "Web portal for shippers — booking, tracking, invoices. Weekly releases. Dev + staging envs.",
     "https://github.com/meridian-internal/customer-portal",
     "team-orion"),

    ("tracking-page",
     "Tracking Page",
     "Public shipment tracking page (no auth). Weekly releases. No dedicated test env — tests against staging OMS.",
     "https://github.com/meridian-internal/tracking-page",
     "team-orion"),

    ("notification-service",
     "Notification Service",
     "Fan-out push notifications, email, and SMS alerts. Bi-weekly releases. Dev stub env only.",
     "https://github.com/meridian-internal/notification-service",
     "team-orion"),

    ("customer-api-gateway",
     "Customer API Gateway",
     "Rate-limited API gateway for third-party integrators. Bi-weekly releases. Dev + staging envs.",
     "https://github.com/meridian-internal/customer-api-gateway",
     "team-orion"),

    ("mobile-app-ios",
     "Driver App iOS",
     "iOS driver application. Bi-weekly App Store releases. Test via AWS Device Farm only.",
     "https://github.com/meridian-internal/driver-app-ios",
     "team-orion"),

    ("mobile-app-android",
     "Driver App Android",
     "Android driver application. Bi-weekly Play Store releases. Test via AWS Device Farm only.",
     "https://github.com/meridian-internal/driver-app-android",
     "team-orion"),

    # ── Internal ops ─────────────────────────────────────────────────────────
    ("dispatch-console",
     "Dispatch Console",
     "Internal dispatcher UI — assign jobs, monitor drivers, handle exceptions. Monthly releases. Dev stub only.",
     "https://github.com/meridian-internal/dispatch-console",
     "team-orion"),

    ("warehouse-mgmt",
     "Warehouse Management",
     "WMS — inbound receiving, pick/pack, outbound manifests. Quarterly releases. Shared UAT with fleet.",
     "https://github.com/meridian-internal/warehouse-mgmt",
     "team-anvil"),

    ("fleet-management",
     "Fleet Management",
     "Fleet tracker — vehicle assignments, maintenance schedules, fuel logs. Monthly releases. Shared UAT.",
     "https://github.com/meridian-internal/fleet-management",
     "team-anvil"),

    ("invoice-engine",
     "Invoice Engine",
     "Billing and invoice generation from completed orders. Monthly releases. UAT sandbox only.",
     "https://github.com/meridian-internal/invoice-engine",
     "team-anvil"),

    ("reporting-dashboard",
     "Reporting Dashboard",
     "BI dashboard — KPIs, SLA breach reports, driver performance. Quarterly releases. No test env; uses prod snapshots.",
     "https://github.com/meridian-internal/reporting-dashboard",
     "team-anvil"),

    ("admin-panel",
     "Admin Panel",
     "Internal admin for ops team — user management, config, manual overrides. Quarterly releases. No test env; feature-flagged in prod.",
     "https://github.com/meridian-internal/admin-panel",
     "team-orion"),

    # ── Platform services ─────────────────────────────────────────────────────
    ("auth-service",
     "Auth Service",
     "AuthN/AuthZ — JWT issuance, RBAC, MFA. Bi-weekly releases. Full test env stack; changes require security sign-off.",
     "https://github.com/meridian-internal/auth-service",
     "team-anvil"),

    ("event-bus",
     "Event Bus",
     "Kafka-backed async messaging backbone. Monthly releases. Dev sandbox env only.",
     "https://github.com/meridian-internal/event-bus",
     "team-anvil"),

    ("file-storage",
     "File Storage",
     "S3-backed object storage API — POD photos, documents. On-demand releases. No dedicated test env; mock via stub.",
     "https://github.com/meridian-internal/file-storage",
     "team-anvil"),

    ("audit-log",
     "Audit Log",
     "Append-only compliance audit trail. On-demand releases. No test env — append-only, production-validated.",
     "https://github.com/meridian-internal/audit-log",
     "team-anvil"),

    ("config-service",
     "Config Service",
     "Feature flags and runtime configuration. On-demand releases. Dev isolated env.",
     "https://github.com/meridian-internal/config-service",
     "team-anvil"),

    # ── Comms ─────────────────────────────────────────────────────────────────
    ("email-service",
     "Email Service",
     "Transactional email via SendGrid. Continuous CD. Mock-only env (Mailtrap).",
     "https://github.com/meridian-internal/email-service",
     "team-anvil"),

    ("sms-gateway",
     "SMS Gateway",
     "SMS and WhatsApp delivery via Twilio. Continuous CD. Mock-only env (Twilio test mode).",
     "https://github.com/meridian-internal/sms-gateway",
     "team-anvil"),

    # ── Data & Integration ────────────────────────────────────────────────────
    ("etl-pipeline",
     "ETL Pipeline",
     "Nightly ETL from operational DBs to data warehouse. Monthly releases. Dev isolated env only.",
     "https://github.com/meridian-internal/etl-pipeline",
     "team-anvil"),

    ("analytics-platform",
     "Analytics Platform",
     "ClickHouse-backed analytics for ops and executive dashboards. Quarterly releases. No test env — cost prohibitive.",
     "https://github.com/meridian-internal/analytics-platform",
     "team-anvil"),

    ("data-warehouse",
     "Data Warehouse Loader",
     "Snowflake loader — transforms and loads from ETL. Quarterly releases. No dedicated test env.",
     "https://github.com/meridian-internal/data-warehouse",
     "team-anvil"),

    ("carrier-integration",
     "Carrier Integration",
     "Third-party carrier APIs — FedEx, UPS, USPS rate shopping and tracking. Monthly releases. External sandbox envs.",
     "https://github.com/meridian-internal/carrier-integration",
     "team-orion"),

    ("geocoding-service",
     "Geocoding Service",
     "Google Maps wrapper — address validation, lat/lng lookup. On-demand releases. No test env; uses Google Maps test API key.",
     "https://github.com/meridian-internal/geocoding-service",
     "team-orion"),

    # ── Legacy ────────────────────────────────────────────────────────────────
    ("legacy-crm",
     "Legacy CRM",
     "Old customer relationship management system (being replaced by new CRM). On-demand patches only. No test env — too risky to replicate.",
     "https://github.com/meridian-internal/legacy-crm",
     "devstar"),

    ("old-driver-app",
     "Old Driver App (v1)",
     "Deprecated driver app — sunset in progress, migrating users to v2. No new releases. No test env.",
     "https://github.com/meridian-internal/driver-app-v1",
     "devstar"),

    ("customs-clearance",
     "Customs Clearance Integration",
     "External customs API for cross-border shipments. Quarterly releases. External vendor sandbox only.",
     "https://github.com/meridian-internal/customs-clearance",
     "team-anvil"),
]

for slug, name, desc, repo, team_key in deployables:
    ids[f"dep-{slug}"] = create(GROUNDWORK, "/deployable/api", {
        "name": name,
        "description": desc,
        "repo_url": repo,
        "team_id": ids[team_key],
    })

# ══════════════════════════════════════════════════════════════════════════════
# GROUNDWORK — Services (what each deployable exposes)
# ══════════════════════════════════════════════════════════════════════════════

section("GROUNDWORK: Services")

services = [
    ("svc-route-api",       "Route Optimizer API",         "rest",    "Optimisation endpoint — POST a delivery batch, get back ordered stops", "https://groundwork.meridianfreight.internal/route-optimizer"),
    ("svc-oms-api",         "OMS REST API",                "rest",    "Order CRUD, status transitions, fulfilment workflow",                   "https://groundwork.meridianfreight.internal/order-management"),
    ("svc-oms-events",      "OMS Event Stream",            "event",   "Order lifecycle events on the event bus",                               None),
    ("svc-driver-api",      "Driver API",                  "rest",    "Job assignment, GPS telemetry, POD capture",                            "https://groundwork.meridianfreight.internal/driver-api"),
    ("svc-portal-api",      "Customer Portal API",         "rest",    "Booking, tracking, and account endpoints for the portal SPA",           "https://groundwork.meridianfreight.internal/customer-portal"),
    ("svc-tracking-api",    "Tracking API",                "rest",    "Public shipment tracking — no auth required",                           "https://groundwork.meridianfreight.internal/tracking-page"),
    ("svc-notify-api",      "Notification Service API",    "rest",    "Fan-out trigger endpoint for push/email/SMS",                           "https://groundwork.meridianfreight.internal/notification-service"),
    ("svc-apigw",           "Customer API Gateway",        "rest",    "Rate-limited gateway for third-party integrators",                      "https://api.meridianfreight.com"),
    ("svc-auth-api",        "Auth Service API",            "rest",    "JWT issuance, token introspection, RBAC enforcement",                   "https://groundwork.meridianfreight.internal/auth-service"),
    ("svc-auth-events",     "Auth Event Stream",           "event",   "Login, logout, and permission change events",                           None),
    ("svc-event-bus",       "Event Bus API",               "rest",    "Topic management and health endpoint",                                  "https://groundwork.meridianfreight.internal/event-bus"),
    ("svc-files-api",       "File Storage API",            "rest",    "Object upload/download/presign",                                        "https://groundwork.meridianfreight.internal/file-storage"),
    ("svc-audit-api",       "Audit Log API",               "rest",    "Append-only audit event ingest and query",                              "https://groundwork.meridianfreight.internal/audit-log"),
    ("svc-config-api",      "Config Service API",          "rest",    "Feature flag evaluation and config fetch",                              "https://groundwork.meridianfreight.internal/config-service"),
    ("svc-email-api",       "Email Service API",           "rest",    "Send transactional email via SendGrid",                                 "https://groundwork.meridianfreight.internal/email-service"),
    ("svc-sms-api",         "SMS Gateway API",             "rest",    "Send SMS/WhatsApp via Twilio",                                          "https://groundwork.meridianfreight.internal/sms-gateway"),
    ("svc-warehouse-api",   "Warehouse Management API",    "rest",    "Inbound/outbound manifest and inventory endpoints",                     "https://groundwork.meridianfreight.internal/warehouse-mgmt"),
    ("svc-fleet-api",       "Fleet Management API",        "rest",    "Vehicle status, maintenance scheduling",                                "https://groundwork.meridianfreight.internal/fleet-management"),
    ("svc-invoice-api",     "Invoice Engine API",          "rest",    "Invoice generation and billing queries",                                "https://groundwork.meridianfreight.internal/invoice-engine"),
    ("svc-etl-api",         "ETL Pipeline API",            "rest",    "Trigger nightly runs and check job status",                             "https://groundwork.meridianfreight.internal/etl-pipeline"),
    ("svc-analytics-api",   "Analytics Platform API",      "graphql", "ClickHouse-backed analytics query API",                                 "https://groundwork.meridianfreight.internal/analytics-platform"),
    ("svc-carrier-api",     "Carrier Integration API",     "rest",    "Rate shopping and tracking across FedEx, UPS, USPS",                    "https://groundwork.meridianfreight.internal/carrier-integration"),
    ("svc-geocoding-api",   "Geocoding Service API",       "rest",    "Address validation and lat/lng lookup",                                 "https://groundwork.meridianfreight.internal/geocoding-service"),
    ("svc-dispatch-api",    "Dispatch Console API",        "rest",    "Internal dispatcher job management",                                    "https://groundwork.meridianfreight.internal/dispatch-console"),
]

for slug, name, type_, desc, endpoint in services:
    payload = {"name": name, "type": type_, "description": desc}
    if endpoint:
        payload["endpoint"] = endpoint
    ids[slug] = create(GROUNDWORK, "/service/api", payload)

# ══════════════════════════════════════════════════════════════════════════════
# GROUNDWORK — Exposes (deployable → service)
# ══════════════════════════════════════════════════════════════════════════════

section("GROUNDWORK: Exposes")

exposes_map = [
    ("dep-route-optimizer",    "svc-route-api",      "8080", "http"),
    ("dep-order-management",   "svc-oms-api",        "8080", "http"),
    ("dep-order-management",   "svc-oms-events",     None,   "kafka"),
    ("dep-driver-api",         "svc-driver-api",     "8080", "http"),
    ("dep-customer-portal",    "svc-portal-api",     "8080", "http"),
    ("dep-tracking-page",      "svc-tracking-api",   "443",  "https"),
    ("dep-notification-service","svc-notify-api",    "8080", "http"),
    ("dep-customer-api-gateway","svc-apigw",         "443",  "https"),
    ("dep-auth-service",       "svc-auth-api",       "8080", "http"),
    ("dep-auth-service",       "svc-auth-events",    None,   "kafka"),
    ("dep-event-bus",          "svc-event-bus",      "9092", "kafka"),
    ("dep-file-storage",       "svc-files-api",      "8080", "http"),
    ("dep-audit-log",          "svc-audit-api",      "8080", "http"),
    ("dep-config-service",     "svc-config-api",     "8080", "http"),
    ("dep-email-service",      "svc-email-api",      "8080", "http"),
    ("dep-sms-gateway",        "svc-sms-api",        "8080", "http"),
    ("dep-warehouse-mgmt",     "svc-warehouse-api",  "8080", "http"),
    ("dep-fleet-management",   "svc-fleet-api",      "8080", "http"),
    ("dep-invoice-engine",     "svc-invoice-api",    "8080", "http"),
    ("dep-etl-pipeline",       "svc-etl-api",        "8080", "http"),
    ("dep-analytics-platform", "svc-analytics-api",  "8080", "http"),
    ("dep-carrier-integration","svc-carrier-api",    "8080", "http"),
    ("dep-geocoding-service",  "svc-geocoding-api",  "8080", "http"),
    ("dep-dispatch-console",   "svc-dispatch-api",   "8080", "http"),
]

existing_exposes = get_all(GROUNDWORK, "/exposes/api")
existing_exposes_pairs = {(e["deployable_id"], e["service_id"]) for e in existing_exposes}

for dep_key, svc_key, port, protocol in exposes_map:
    did = ids[dep_key]
    sid = ids[svc_key]
    if (did, sid) in existing_exposes_pairs:
        print(f"  skip (exists): {dep_key} exposes {svc_key}")
        continue
    payload = {"deployable_id": did, "service_id": sid, "protocol": protocol}
    if port:
        payload["port"] = port
    result = post(GROUNDWORK, "/exposes/api", payload)
    print(f"  created: {dep_key} exposes {svc_key}")

# ══════════════════════════════════════════════════════════════════════════════
# GROUNDWORK — Dependencies (who depends on what)
# ══════════════════════════════════════════════════════════════════════════════

section("GROUNDWORK: Dependencies")

# (consumer_deployable, service_depended_on, protocol, auth_method, criticality)
dependencies = [
    # Auth service is a universal dependency
    ("dep-route-optimizer",     "svc-auth-api",      "http", "jwt",     "high"),
    ("dep-order-management",    "svc-auth-api",      "http", "jwt",     "high"),
    ("dep-driver-api",          "svc-auth-api",      "http", "jwt",     "high"),
    ("dep-customer-portal",     "svc-auth-api",      "http", "jwt",     "high"),
    ("dep-customer-api-gateway","svc-auth-api",      "http", "jwt",     "high"),
    ("dep-dispatch-console",    "svc-auth-api",      "http", "jwt",     "high"),
    ("dep-warehouse-mgmt",      "svc-auth-api",      "http", "jwt",     "medium"),
    ("dep-fleet-management",    "svc-auth-api",      "http", "jwt",     "medium"),
    ("dep-admin-panel",         "svc-auth-api",      "http", "jwt",     "medium"),

    # Route optimizer depends on geocoding
    ("dep-route-optimizer",     "svc-geocoding-api", "http", "api_key", "high"),

    # OMS emits events; various things consume them
    ("dep-notification-service","svc-oms-events",    "kafka","none",    "high"),
    ("dep-invoice-engine",      "svc-oms-events",    "kafka","none",    "high"),
    ("dep-tracking-page",       "svc-oms-api",       "http", "jwt",     "high"),
    ("dep-dispatch-console",    "svc-oms-api",       "http", "jwt",     "high"),
    ("dep-driver-api",          "svc-oms-api",       "http", "service_token", "high"),
    ("dep-reporting-dashboard", "svc-oms-api",       "http", "service_token", "low"),
    ("dep-etl-pipeline",        "svc-oms-api",       "http", "service_token", "low"),

    # Portal depends on OMS + carrier + geocoding
    ("dep-customer-portal",     "svc-oms-api",       "http", "jwt",     "high"),
    ("dep-customer-portal",     "svc-carrier-api",   "http", "api_key", "medium"),
    ("dep-customer-portal",     "svc-geocoding-api", "http", "api_key", "medium"),
    ("dep-customer-portal",     "svc-files-api",     "http", "jwt",     "low"),

    # API gateway fans out to OMS + tracking
    ("dep-customer-api-gateway","svc-oms-api",       "http", "jwt",     "high"),
    ("dep-customer-api-gateway","svc-tracking-api",  "http", "none",    "high"),

    # Notification service uses email + sms
    ("dep-notification-service","svc-email-api",     "http", "api_key", "medium"),
    ("dep-notification-service","svc-sms-api",       "http", "api_key", "medium"),

    # Route optimizer feeds into driver api
    ("dep-driver-api",          "svc-route-api",     "http", "service_token", "high"),

    # Warehouse depends on carrier for manifests
    ("dep-warehouse-mgmt",      "svc-carrier-api",   "http", "api_key", "medium"),

    # Audit log is consumed by almost everything (fire-and-forget)
    ("dep-order-management",    "svc-audit-api",     "http", "service_token", "low"),
    ("dep-auth-service",        "svc-audit-api",     "http", "service_token", "low"),
    ("dep-admin-panel",         "svc-audit-api",     "http", "service_token", "low"),

    # Config service — feature flags
    ("dep-route-optimizer",     "svc-config-api",    "http", "service_token", "medium"),
    ("dep-order-management",    "svc-config-api",    "http", "service_token", "medium"),
    ("dep-customer-portal",     "svc-config-api",    "http", "service_token", "low"),

    # Analytics depends on ETL outputs (via data warehouse)
    ("dep-analytics-platform",  "svc-etl-api",       "http", "service_token", "medium"),
    ("dep-reporting-dashboard", "svc-analytics-api", "graphql","service_token","medium"),

    # File storage is used for POD photos and invoice PDFs
    ("dep-driver-api",          "svc-files-api",     "http", "jwt",     "medium"),
    ("dep-invoice-engine",      "svc-files-api",     "http", "jwt",     "low"),
]

existing_deps = get_all(GROUNDWORK, "/dependency/api")
existing_dep_pairs = {(d["deployable_id"], d["service_id"]) for d in existing_deps}

for dep_key, svc_key, protocol, auth, criticality in dependencies:
    did = ids[dep_key]
    sid = ids[svc_key]
    if (did, sid) in existing_dep_pairs:
        print(f"  skip (exists): {dep_key} → {svc_key}")
        continue
    post(GROUNDWORK, "/dependency/api", {
        "deployable_id": did,
        "service_id": sid,
        "protocol": protocol,
        "auth_method": auth,
        "criticality": criticality,
    })
    print(f"  created: {dep_key} → {svc_key} ({criticality})")

# ══════════════════════════════════════════════════════════════════════════════
# GROUNDWORK — Contracts & SLAs (key services only)
# ══════════════════════════════════════════════════════════════════════════════

section("GROUNDWORK: Contracts & SLAs")

contracts = [
    ("svc-route-api",      "https://specs.meridianfreight.internal/route-optimizer/v2.3.yaml", "2.3", "openapi"),
    ("svc-oms-api",        "https://specs.meridianfreight.internal/order-management/v1.8.yaml","1.8", "openapi"),
    ("svc-driver-api",     "https://specs.meridianfreight.internal/driver-api/v3.1.yaml",      "3.1", "openapi"),
    ("svc-auth-api",       "https://specs.meridianfreight.internal/auth-service/v2.0.yaml",    "2.0", "openapi"),
    ("svc-apigw",          "https://specs.meridianfreight.internal/customer-api-gw/v1.2.yaml", "1.2", "openapi"),
    ("svc-analytics-api",  "https://specs.meridianfreight.internal/analytics/v1.0.graphql",    "1.0", "graphql"),
    ("svc-carrier-api",    "https://specs.meridianfreight.internal/carrier/v2.1.yaml",         "2.1", "openapi"),
]

slas = [
    # (contract_svc_key, metric, target, window)
    ("svc-route-api",    "p99_latency_ms", "800",  "1h"),
    ("svc-route-api",    "availability",   "99.9", "30d"),
    ("svc-oms-api",      "p99_latency_ms", "500",  "1h"),
    ("svc-oms-api",      "availability",   "99.9", "30d"),
    ("svc-driver-api",   "p99_latency_ms", "300",  "1h"),
    ("svc-driver-api",   "availability",   "99.95","30d"),
    ("svc-auth-api",     "p99_latency_ms", "100",  "1h"),
    ("svc-auth-api",     "availability",   "99.99","30d"),
    ("svc-apigw",        "p99_latency_ms", "200",  "1h"),
    ("svc-apigw",        "availability",   "99.9", "30d"),
    ("svc-carrier-api",  "p99_latency_ms", "2000", "1h"),
    ("svc-carrier-api",  "availability",   "99.5", "30d"),
]

existing_contracts = get_all(GROUNDWORK, "/contract/api")
existing_contract_svc_ids = {c["service_id"] for c in existing_contracts}
contract_id_by_svc = {c["service_id"]: c["id"] for c in existing_contracts}

for svc_key, spec_url, version, format_ in contracts:
    sid = ids[svc_key]
    if sid in existing_contract_svc_ids:
        print(f"  skip (exists): contract for {svc_key}")
    else:
        result = post(GROUNDWORK, "/contract/api", {
            "service_id": sid, "spec_url": spec_url, "version": version, "format": format_,
        })
        contract_id_by_svc[sid] = result["id"]
        print(f"  created: contract for {svc_key}")

existing_slas = get_all(GROUNDWORK, "/sla/api")
existing_sla_pairs = {(s["contract_id"], s["metric"]) for s in existing_slas}

for svc_key, metric, target, window in slas:
    sid = ids[svc_key]
    cid = contract_id_by_svc.get(sid)
    if not cid:
        print(f"  skip (no contract): SLA for {svc_key}/{metric}")
        continue
    if (cid, metric) in existing_sla_pairs:
        print(f"  skip (exists): SLA {svc_key}/{metric}")
        continue
    post(GROUNDWORK, "/sla/api", {"contract_id": cid, "metric": metric, "target": target, "window": window})
    print(f"  created: SLA {svc_key} {metric}={target}/{window}")

# ══════════════════════════════════════════════════════════════════════════════
# CITYHALL — Org Structure
# ══════════════════════════════════════════════════════════════════════════════

section("CITYHALL: Org Nodes")

# enterprise → division → domain → product → team (leaf)
ids["org-meridian"] = create(CITYHALL, "/org_node/api", {
    "name": "Meridian Freight Solutions", "kind": "enterprise",
})
ids["org-engineering"] = create(CITYHALL, "/org_node/api", {
    "name": "Engineering", "kind": "division", "parent_id": ids["org-meridian"],
})
ids["org-product-domain"] = create(CITYHALL, "/org_node/api", {
    "name": "Product Engineering", "kind": "domain", "parent_id": ids["org-engineering"],
})
ids["org-platform-domain"] = create(CITYHALL, "/org_node/api", {
    "name": "Platform Engineering", "kind": "domain", "parent_id": ids["org-engineering"],
})
ids["org-agency-domain"] = create(CITYHALL, "/org_node/api", {
    "name": "Agency Partners", "kind": "domain", "parent_id": ids["org-engineering"],
})

# Product leaf nodes → Teams
ids["org-customer-exp"] = create(CITYHALL, "/org_node/api", {
    "name": "Customer Experience", "kind": "product",
    "parent_id": ids["org-product-domain"], "team_id": ids["team-orion"],
})
ids["org-platform-ops"] = create(CITYHALL, "/org_node/api", {
    "name": "Platform & Data", "kind": "product",
    "parent_id": ids["org-platform-domain"], "team_id": ids["team-anvil"],
})

# Agency leaf nodes → Teams
ids["org-apex"] = create(CITYHALL, "/org_node/api", {
    "name": "Apex Digital (Partner)", "kind": "product",
    "parent_id": ids["org-agency-domain"], "team_id": ids["apex-digital"],
})
ids["org-northbridge"] = create(CITYHALL, "/org_node/api", {
    "name": "NorthBridge Tech (Partner)", "kind": "product",
    "parent_id": ids["org-agency-domain"], "team_id": ids["northbridge-tech"],
})
ids["org-devstar"] = create(CITYHALL, "/org_node/api", {
    "name": "DevStar (Partner)", "kind": "product",
    "parent_id": ids["org-agency-domain"], "team_id": ids["devstar"],
})

section("CITYHALL: Bylaws")

bylaws = [
    # Enterprise-level: Q4 shipping season freeze
    {
        "org_node_id": ids["org-meridian"],
        "gate_type": "FreezePeriod",
        "priority": "1",
        "description": "Q4 peak-season deployment freeze — no prod changes Nov 15 through Jan 5",
        "window": "Nov-15 to Jan-05",
    },
    # Enterprise-level: all prod deployments need approval
    {
        "org_node_id": ids["org-meridian"],
        "gate_type": "ApprovalGate",
        "priority": "2",
        "description": "All production deployments require CTO or Engineering Director sign-off",
        "conditions": "tier == prod",
        "approvers": "cto,eng-director",
    },
    # Platform domain: auth changes need security review
    {
        "org_node_id": ids["org-platform-domain"],
        "gate_type": "ApprovalGate",
        "priority": "1",
        "description": "Any change touching auth-service or config-service requires security team approval",
        "conditions": "deployable in [auth-service, config-service]",
        "approvers": "security-team",
    },
    # Product domain: core systems need quiesce window
    {
        "org_node_id": ids["org-product-domain"],
        "gate_type": "QuiesceGate",
        "priority": "1",
        "description": "Core systems (route-optimizer, OMS, driver-api) require 30-min quiesce before UAT promotion",
        "conditions": "deployable in [route-optimizer, order-management, driver-api]",
        "quiesce_for": "30m",
    },
    # Product domain: UAT sign-off required for customer-facing systems
    {
        "org_node_id": ids["org-customer-exp"],
        "gate_type": "ApprovalGate",
        "priority": "2",
        "description": "Customer-facing systems need UAT sign-off from product owner before prod",
        "conditions": "tier == uat",
        "approvers": "product-owner",
    },
    # Platform: no deployments during weekend without on-call approval
    {
        "org_node_id": ids["org-platform-domain"],
        "gate_type": "WindowGate",
        "priority": "3",
        "description": "Platform changes must deploy Mon-Fri 09:00-17:00 ET unless on-call approved",
        "window": "Mon-Fri 09:00-17:00 ET",
    },
]

existing_bylaws = get_all(CITYHALL, "/bylaw/api")
existing_bylaw_descs = {b["description"] for b in existing_bylaws}

for bylaw in bylaws:
    if bylaw["description"] in existing_bylaw_descs:
        print(f"  skip (exists): {bylaw['description'][:60]}")
        continue
    result = post(CITYHALL, "/bylaw/api", bylaw)
    print(f"  created: {bylaw['gate_type']} — {bylaw['description'][:60]}")

section("CITYHALL: Change Requests")

change_requests = [
    {
        "summary": "Route Optimizer v2.4 — dynamic re-routing on traffic events",
        "description": "Upgrade route-optimizer to v2.4 which adds real-time traffic integration via Google Maps Traffic API. Requires geocoding-service dependency bump. Blast radius: driver-api, customer-portal.",
        "target_deployables": f"{ids['dep-route-optimizer']},{ids['dep-geocoding-service']}",
        "target_versions": "route-optimizer:2.4.0,geocoding-service:1.6.2",
        "requested_by": ids["person-sarah-chen"],
        "tier": "uat",
        "status": "approved",
    },
    {
        "summary": "OMS — migrate payment gateway from Stripe v2 to v3",
        "description": "Stripe v2 API deprecated Q3. Migrating order-management to Stripe v3 SDK. Breaking change in webhook payload shape — notification-service and invoice-engine need coordinated update.",
        "target_deployables": f"{ids['dep-order-management']},{ids['dep-notification-service']},{ids['dep-invoice-engine']}",
        "target_versions": "order-management:1.9.0,notification-service:2.3.1,invoice-engine:3.1.0",
        "requested_by": ids["person-marcus-webb"],
        "tier": "dev",
        "status": "submitted",
    },
    {
        "summary": "Auth Service — MFA enforcement rollout (phase 2)",
        "description": "Phase 2 of MFA rollout: enforce TOTP for all internal users. Requires auth-service config change + customer-portal login flow update. Coordination with admin-panel for user onboarding flow.",
        "target_deployables": f"{ids['dep-auth-service']},{ids['dep-customer-portal']},{ids['dep-admin-panel']}",
        "target_versions": "auth-service:2.1.0,customer-portal:4.2.0,admin-panel:1.8.0",
        "requested_by": ids["person-david-kim"],
        "tier": "dev",
        "status": "draft",
    },
    {
        "summary": "ETL Pipeline — add carrier telemetry source",
        "description": "Extend ETL to ingest carrier tracking events from FedEx Firehose. New data source feeds analytics-platform and reporting-dashboard. NorthBridge leading implementation.",
        "target_deployables": f"{ids['dep-etl-pipeline']},{ids['dep-analytics-platform']},{ids['dep-carrier-integration']}",
        "target_versions": "etl-pipeline:2.2.0,analytics-platform:1.3.0,carrier-integration:2.2.0",
        "requested_by": ids["person-laura-strand"],
        "tier": "dev",
        "status": "draft",
    },
    {
        "summary": "Legacy CRM → Hubspot migration (phase 1: data export)",
        "description": "First phase of CRM sunset: export all customer and contact records from legacy-crm to Hubspot. DevStar leading. No downtime required — read-only migration script.",
        "target_deployables": f"{ids['dep-legacy-crm']}",
        "target_versions": "legacy-crm:0.9.1-export-patch",
        "requested_by": ids["person-alex-foster"],
        "tier": "prod",
        "status": "approved",
    },
]

existing_crs = get_all(CITYHALL, "/change_request/api")
existing_cr_summaries = {cr["summary"] for cr in existing_crs}
cr_ids = []

for cr in change_requests:
    if cr["summary"] in existing_cr_summaries:
        print(f"  skip (exists): {cr['summary'][:60]}")
        for e in existing_crs:
            if e["summary"] == cr["summary"]:
                cr_ids.append(e["id"])
        continue
    result = post(CITYHALL, "/change_request/api", cr)
    cr_ids.append(result["id"])
    print(f"  created: {cr['summary'][:60]}")

# ══════════════════════════════════════════════════════════════════════════════
# YARD — Test Infrastructure
# ══════════════════════════════════════════════════════════════════════════════

section("YARD: Test Infrastructure")

infra = [
    ("AWS ECS Cluster (eu-west-1)",  "aws_ecs",      "eu-west-1", "fargate",     "0.12", "Fargate-based isolated envs; spins up per-PR, tears down on merge"),
    ("Internal K8s (on-prem)",       "kubernetes",   "dc-london",  "k8s-shared",  "0.03", "Shared Kubernetes cluster; used for persistent sandbox envs"),
    ("AWS Device Farm",              "external_saas","us-east-1", "device-farm", "0.17", "Mobile device testing — iOS + Android physical device pool"),
]

for name, provider, region, instance_type, cost, notes in infra:
    ids[f"infra-{name[:20].strip().lower().replace(' ','_')}"] = create(YARD, "/test_infrastructure/api", {
        "name": name, "provider": provider, "region": region,
        "instance_type": instance_type, "cost_per_hour": cost, "notes": notes,
    })

ecs_id = ids.get("infra-aws_ecs_cluster_(eu")
k8s_id = ids.get("infra-internal_k8s_(on-pr")
device_farm_id = ids.get("infra-aws_device_farm")

# Easier slug lookup
for k, v in list(ids.items()):
    if "ecs" in k:
        ecs_id = v
    if "k8s" in k:
        k8s_id = v
    if "device" in k:
        device_farm_id = v

section("YARD: Mock Sources")

mock_sources = [
    ("Mailtrap (email sandbox)",    "https://github.com/meridian-internal/email-service", "fixtures/mailtrap_sink", "ruby",   "Captures outbound email; asserts subject/body in test suites"),
    ("Twilio Test Credentials",     "https://github.com/meridian-internal/sms-gateway",   "fixtures/twilio_test",   "python", "Twilio magic numbers for SMS delivery simulation"),
    ("Carrier Sandbox Accounts",    "https://github.com/meridian-internal/carrier-integration","fixtures/sandboxes","json", "FedEx/UPS/USPS sandbox API credentials and test shipment IDs"),
    ("WireMock OMS Stub",           "https://github.com/meridian-internal/tracking-page",  "fixtures/oms_stub",      "java",  "WireMock recording of OMS API responses for tracking-page tests"),
    ("Google Maps Test Key",        "https://github.com/meridian-internal/geocoding-service","fixtures/gcp_test",    "json",  "GCP test project API key with geocoding quota limits"),
]

for name, repo_url, path, lang, notes in mock_sources:
    ids[f"mock-{name[:20].lower().replace(' ','_').replace('(','').replace(')','').strip('_')}"] = create(YARD, "/mock_source/api", {
        "name": name, "repo_url": repo_url, "path": path, "language": lang, "notes": notes,
    })

section("YARD: Test Environments")

# ── Environments WITH full stacks ──────────────────────────────────────────────

envs = [
    # route-optimizer: isolated PR env + staging sandbox
    {
        "name": "route-optimizer / dev (isolated)",
        "kind": "isolated",
        "deployable_id": ids["dep-route-optimizer"],
        "infrastructure_id": ecs_id,
        "cost_per_hour": "0.15",
        "spinup_minutes": "6",
        "teardown_policy": "on_finish",
        "max_duration_minutes": "60",
        "concurrency_limit": "5",
        "notes": "Spun up per-PR via GitHub Actions. Torn down on merge or 60-min timeout.",
    },
    {
        "name": "route-optimizer / staging (sandbox)",
        "kind": "sandbox",
        "deployable_id": ids["dep-route-optimizer"],
        "infrastructure_id": k8s_id,
        "cost_per_hour": "0.04",
        "spinup_minutes": "3",
        "teardown_policy": "never",
        "notes": "Persistent staging env. Redeployed on merge to main. Seeded with synthetic route data.",
    },

    # order-management: isolated dev + multi-tenant UAT
    {
        "name": "order-management / dev (isolated)",
        "kind": "isolated",
        "deployable_id": ids["dep-order-management"],
        "infrastructure_id": ecs_id,
        "cost_per_hour": "0.18",
        "spinup_minutes": "8",
        "teardown_policy": "on_finish",
        "max_duration_minutes": "90",
        "concurrency_limit": "3",
        "notes": "Isolated per-branch env. Heavy DB migrations mean longer spinup.",
    },
    {
        "name": "order-management / UAT (multi-tenant)",
        "kind": "multi-tenant",
        "deployable_id": ids["dep-order-management"],
        "infrastructure_id": k8s_id,
        "cost_per_hour": "0.06",
        "spinup_minutes": "5",
        "teardown_policy": "manual",
        "concurrency_limit": "2",
        "rate_limit": "2 concurrent UAT campaigns",
        "notes": "Shared UAT env. Apex Digital and Orion book time slots. Manual teardown only.",
    },

    # driver-api: isolated dev + staging sandbox
    {
        "name": "driver-api / dev (isolated)",
        "kind": "isolated",
        "deployable_id": ids["dep-driver-api"],
        "infrastructure_id": ecs_id,
        "cost_per_hour": "0.14",
        "spinup_minutes": "5",
        "teardown_policy": "on_finish",
        "max_duration_minutes": "60",
        "concurrency_limit": "4",
        "notes": "Isolated env with mock GPS telemetry injector. Used for unit + contract tests.",
    },
    {
        "name": "driver-api / staging (sandbox)",
        "kind": "sandbox",
        "deployable_id": ids["dep-driver-api"],
        "infrastructure_id": k8s_id,
        "cost_per_hour": "0.04",
        "spinup_minutes": "3",
        "teardown_policy": "never",
        "notes": "Persistent staging. Connected to route-optimizer staging for end-to-end flows.",
    },

    # auth-service: isolated dev + staging sandbox
    {
        "name": "auth-service / dev (isolated)",
        "kind": "isolated",
        "deployable_id": ids["dep-auth-service"],
        "infrastructure_id": ecs_id,
        "cost_per_hour": "0.10",
        "spinup_minutes": "4",
        "teardown_policy": "on_finish",
        "max_duration_minutes": "45",
        "concurrency_limit": "6",
        "notes": "Isolated per-PR. Security team reviews any test environment provisioning for auth-service.",
    },
    {
        "name": "auth-service / staging (sandbox)",
        "kind": "sandbox",
        "deployable_id": ids["dep-auth-service"],
        "infrastructure_id": k8s_id,
        "cost_per_hour": "0.04",
        "spinup_minutes": "3",
        "teardown_policy": "never",
        "notes": "Persistent staging env shared by all services that depend on auth. Treated as production-like.",
    },

    # customer-portal: isolated dev + staging sandbox
    {
        "name": "customer-portal / dev (isolated)",
        "kind": "isolated",
        "deployable_id": ids["dep-customer-portal"],
        "infrastructure_id": ecs_id,
        "cost_per_hour": "0.12",
        "spinup_minutes": "5",
        "teardown_policy": "on_finish",
        "max_duration_minutes": "60",
        "concurrency_limit": "4",
        "notes": "Per-PR isolated env. Points to auth-service staging and OMS dev.",
    },
    {
        "name": "customer-portal / staging (sandbox)",
        "kind": "sandbox",
        "deployable_id": ids["dep-customer-portal"],
        "infrastructure_id": k8s_id,
        "cost_per_hour": "0.04",
        "spinup_minutes": "3",
        "teardown_policy": "never",
        "notes": "Persistent staging. Apex Digital uses this for UAT validation runs.",
    },

    # customer-api-gateway: dev sandbox + staging sandbox
    {
        "name": "customer-api-gateway / dev (sandbox)",
        "kind": "sandbox",
        "deployable_id": ids["dep-customer-api-gateway"],
        "infrastructure_id": k8s_id,
        "cost_per_hour": "0.04",
        "spinup_minutes": "3",
        "teardown_policy": "on_idle",
        "notes": "Shared dev sandbox. Used for integration tests with Apex Digital integrators.",
    },
    {
        "name": "customer-api-gateway / staging (sandbox)",
        "kind": "sandbox",
        "deployable_id": ids["dep-customer-api-gateway"],
        "infrastructure_id": k8s_id,
        "cost_per_hour": "0.05",
        "spinup_minutes": "3",
        "teardown_policy": "never",
        "notes": "Persistent staging. External partners test against this endpoint.",
    },

    # ── Partial environments ────────────────────────────────────────────────────

    # notification-service: dev stub only
    {
        "name": "notification-service / dev (stub)",
        "kind": "stub",
        "deployable_id": ids["dep-notification-service"],
        "infrastructure_id": ecs_id,
        "mock_source_id": "1b24f363-b533-429c-968f-85f387cd8e44",  # Mailtrap
        "cost_per_hour": "0.08",
        "spinup_minutes": "4",
        "teardown_policy": "on_finish",
        "max_duration_minutes": "30",
        "notes": "Stub env with Mailtrap + Twilio test mode. No staging — relies on staging OMS events.",
    },

    # event-bus: dev sandbox only
    {
        "name": "event-bus / dev (sandbox)",
        "kind": "sandbox",
        "deployable_id": ids["dep-event-bus"],
        "infrastructure_id": k8s_id,
        "cost_per_hour": "0.05",
        "spinup_minutes": "6",
        "teardown_policy": "on_idle",
        "notes": "Shared Kafka dev sandbox. All services in dev consume from this topic namespace.",
    },

    # warehouse + fleet: shared UAT
    {
        "name": "ops-suite / UAT (multi-tenant, shared)",
        "kind": "multi-tenant",
        "deployable_id": ids["dep-warehouse-mgmt"],
        "infrastructure_id": k8s_id,
        "cost_per_hour": "0.08",
        "spinup_minutes": "10",
        "teardown_policy": "manual",
        "concurrency_limit": "1",
        "rate_limit": "1 UAT campaign at a time",
        "notes": "Shared UAT used by both warehouse-mgmt and fleet-management. Booked via shared calendar.",
    },

    # invoice-engine: UAT sandbox
    {
        "name": "invoice-engine / UAT (sandbox)",
        "kind": "sandbox",
        "deployable_id": ids["dep-invoice-engine"],
        "infrastructure_id": k8s_id,
        "cost_per_hour": "0.05",
        "spinup_minutes": "5",
        "teardown_policy": "manual",
        "notes": "UAT-only sandbox. Seeded with synthetic order data. No dev env — tested via OMS dev.",
    },

    # dispatch-console: dev stub
    {
        "name": "dispatch-console / dev (stub)",
        "kind": "stub",
        "deployable_id": ids["dep-dispatch-console"],
        "infrastructure_id": ecs_id,
        "mock_source_id": "4738b345-84c2-4f34-a0ec-f6497f5ee640",
        "cost_per_hour": "0.06",
        "spinup_minutes": "3",
        "teardown_policy": "on_finish",
        "notes": "Stub env against WireMock OMS. Playwright E2E tests run here.",
    },

    # etl-pipeline: isolated dev only
    {
        "name": "etl-pipeline / dev (isolated)",
        "kind": "isolated",
        "deployable_id": ids["dep-etl-pipeline"],
        "infrastructure_id": ecs_id,
        "cost_per_hour": "0.20",
        "spinup_minutes": "12",
        "teardown_policy": "on_finish",
        "max_duration_minutes": "120",
        "notes": "Isolated env with synthetic data fixtures. Long spinup due to DB snapshot restore.",
    },

    # config-service: isolated dev
    {
        "name": "config-service / dev (isolated)",
        "kind": "isolated",
        "deployable_id": ids["dep-config-service"],
        "infrastructure_id": ecs_id,
        "cost_per_hour": "0.08",
        "spinup_minutes": "3",
        "teardown_policy": "on_finish",
        "notes": "Per-PR isolated env. Tests flag evaluation logic in isolation.",
    },

    # ── Mock-only environments ─────────────────────────────────────────────────

    # email-service: mock only
    {
        "name": "email-service / mock (Mailtrap)",
        "kind": "mock",
        "deployable_id": ids["dep-email-service"],
        "mock_source_id": "1b24f363-b533-429c-968f-85f387cd8e44",
        "cost_per_hour": "0.00",
        "spinup_minutes": "1",
        "teardown_policy": "on_finish",
        "notes": "Mailtrap sandbox. All outbound email captured. No real infrastructure needed.",
    },

    # sms-gateway: mock only
    {
        "name": "sms-gateway / mock (Twilio test mode)",
        "kind": "mock",
        "deployable_id": ids["dep-sms-gateway"],
        "mock_source_id": "6ea62f6b-5913-4b81-88fc-19f9f914e6bb",
        "cost_per_hour": "0.00",
        "spinup_minutes": "1",
        "teardown_policy": "on_finish",
        "notes": "Twilio test credentials. Magic numbers for delivery/failure simulation.",
    },

    # carrier-integration: external sandboxes
    {
        "name": "carrier-integration / external (vendor sandboxes)",
        "kind": "external",
        "deployable_id": ids["dep-carrier-integration"],
        "cost_per_hour": "0.00",
        "spinup_minutes": "0",
        "teardown_policy": "never",
        "contractual_limit": "FedEx/UPS/USPS sandbox ToS — no production data, rate limited",
        "notes": "FedEx, UPS, USPS all provide sandbox API environments. No Meridian infra required.",
    },

    # mobile apps: device farm
    {
        "name": "mobile-apps / external (AWS Device Farm)",
        "kind": "external",
        "deployable_id": ids["dep-mobile-app-ios"],
        "infrastructure_id": device_farm_id,
        "cost_per_hour": "0.17",
        "spinup_minutes": "8",
        "teardown_policy": "on_finish",
        "contractual_limit": "400 device-minutes/month under current plan",
        "notes": "Physical device pool. iOS and Android. Runs Appium tests on real hardware.",
    },
]

for env in envs:
    env_name = env["name"]
    existing_envs = get_all(YARD, "/test_environment/api")
    existing_env_names = {e["name"] for e in existing_envs}
    if env_name in existing_env_names:
        for e in existing_envs:
            if e["name"] == env_name:
                ids[f"env-{env_name[:30]}"] = e["id"]
        print(f"  skip (exists): {env_name}")
        continue
    result = post(YARD, "/test_environment/api", env)
    ids[f"env-{env_name[:30]}"] = result["id"]
    print(f"  created: {env_name}")

# ══════════════════════════════════════════════════════════════════════════════
# UNION — Work Orders (in-flight work)
# ══════════════════════════════════════════════════════════════════════════════

section("UNION: Work Orders")

work_orders = [
    # Active Orion work
    {
        "team_id": ids["team-orion"],
        "deployable_id": ids["dep-route-optimizer"],
        "summary": "Integrate Google Maps Traffic API for dynamic re-routing (CR-001)",
        "status": "in_progress",
        "priority": "high",
    },
    {
        "team_id": ids["apex-digital"],
        "deployable_id": ids["dep-order-management"],
        "summary": "Stripe v3 migration — webhook payload adapter (CR-002)",
        "status": "in_progress",
        "priority": "high",
    },
    {
        "team_id": ids["apex-digital"],
        "deployable_id": ids["dep-customer-portal"],
        "summary": "Update portal login flow for MFA phase 2 (CR-003)",
        "status": "proposed",
        "priority": "medium",
    },
    {
        "team_id": ids["team-orion"],
        "deployable_id": ids["dep-tracking-page"],
        "summary": "Add estimated delivery time to tracking page",
        "status": "in_progress",
        "priority": "medium",
    },
    {
        "team_id": ids["team-orion"],
        "deployable_id": ids["dep-notification-service"],
        "summary": "Add WhatsApp delivery channel for driver alerts",
        "status": "proposed",
        "priority": "low",
    },
    # Active Anvil work
    {
        "team_id": ids["team-anvil"],
        "deployable_id": ids["dep-auth-service"],
        "summary": "MFA enforcement rollout — TOTP for internal users (CR-003)",
        "status": "in_progress",
        "priority": "high",
    },
    {
        "team_id": ids["northbridge-tech"],
        "deployable_id": ids["dep-etl-pipeline"],
        "summary": "Add FedEx Firehose source to ETL pipeline (CR-004)",
        "status": "in_progress",
        "priority": "medium",
    },
    {
        "team_id": ids["northbridge-tech"],
        "deployable_id": ids["dep-analytics-platform"],
        "summary": "New carrier telemetry dashboard in analytics (CR-004)",
        "status": "proposed",
        "priority": "medium",
    },
    {
        "team_id": ids["team-anvil"],
        "deployable_id": ids["dep-event-bus"],
        "summary": "Upgrade Kafka from 3.4 to 3.8 — rolling restart plan",
        "status": "proposed",
        "priority": "low",
    },
    # DevStar legacy work
    {
        "team_id": ids["devstar"],
        "deployable_id": ids["dep-legacy-crm"],
        "summary": "Phase 1: Export all CRM records to Hubspot (CR-005)",
        "status": "in_progress",
        "priority": "high",
    },
    {
        "team_id": ids["devstar"],
        "deployable_id": ids["dep-old-driver-app"],
        "summary": "Sunset old driver app — push final users to v2 app",
        "status": "in_progress",
        "priority": "high",
    },
]

existing_wos = get_all(UNION, "/work_order/api")
existing_wo_summaries = {w["summary"] for w in existing_wos}

for wo in work_orders:
    if wo["summary"] in existing_wo_summaries:
        print(f"  skip (exists): {wo['summary'][:60]}")
        continue
    post(UNION, "/work_order/api", wo)
    print(f"  created: {wo['summary'][:60]}")

# ══════════════════════════════════════════════════════════════════════════════
# Done
# ══════════════════════════════════════════════════════════════════════════════

section("Done ✓")
print("""
  Meridian Freight Solutions seed complete.

  Summary:
    Union:      5 teams, 17 people, 5 team kinds (product/platform/3x support)
    Groundwork: 30 deployables, 24 services, 37 dependencies, 7 contracts, 12 SLAs
    Cityhall:   10 org nodes, 6 bylaws, 5 change requests
    Yard:       3 infra, 5 mock sources, 19 test environments

  Release cadences across the 30 systems:
    Continuous    — email-service, sms-gateway
    Weekly        — route-optimizer, driver-api, customer-portal, tracking-page
    Bi-weekly     — order-management, notification-service, customer-api-gateway,
                    auth-service, mobile-app-ios, mobile-app-android
    Monthly       — dispatch-console, fleet-management, invoice-engine, event-bus,
                    etl-pipeline, carrier-integration
    Quarterly     — warehouse-mgmt, reporting-dashboard, admin-panel,
                    analytics-platform, data-warehouse, customs-clearance
    On-demand     — file-storage, audit-log, config-service, geocoding-service,
                    legacy-crm (patches only), old-driver-app (sunset)

  Test environment coverage:
    Full stack    — route-optimizer, order-management, driver-api, auth-service,
                    customer-portal, customer-api-gateway
    Partial       — notification-service (dev stub), event-bus (dev only),
                    warehouse-mgmt (shared UAT), fleet-management (shared UAT),
                    invoice-engine (UAT only), dispatch-console (dev stub),
                    etl-pipeline (dev only), config-service (dev only)
    Mock/external — email-service, sms-gateway, carrier-integration, mobile apps
    None          — tracking-page, reporting-dashboard, admin-panel, file-storage,
                    audit-log, analytics-platform, data-warehouse, geocoding-service,
                    legacy-crm, old-driver-app, customs-clearance
""")
