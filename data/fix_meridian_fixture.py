#!/usr/bin/env python3
"""One-shot fixture repair for data/meridian_fixture.json.

Issues found in the original fixture (ref-integrity is fine, but realism is thin):
- 14 of 30 deployables had zero dependencies, including obvious consumers
  (mobile apps, admin panels, the data-warehouse loader, legacy CRM …).
- 2 naming collisions: deployable "Driver API" exposes service "Driver API"
  and deployable "Customer API Gateway" exposes a service of the same name —
  reads as a typo in the UI.
- 2 deployables that should clearly own an API exposed nothing (Customs
  Clearance Integration, Legacy CRM).
- Contract / SLA coverage was light — 7 contracts across 24 services.

This script edits the fixture in place. New uuids are minted for added
records; existing ids are preserved so loaders that key by id still work.
"""

from __future__ import annotations

import json
import uuid
from pathlib import Path

FIX_PATH = Path(__file__).parent / "meridian_fixture.json"


def load() -> dict:
    return json.loads(FIX_PATH.read_text())


def save(d: dict) -> None:
    FIX_PATH.write_text(json.dumps(d, indent=2) + "\n")


def by_name(records: list[dict], name: str) -> dict | None:
    for r in records:
        if r.get("name") == name:
            return r
    return None


def main() -> None:
    d = load()
    g = d["groundwork"]

    # ── 1. Resolve naming collisions ──────────────────────────────────────────
    # The deployable and service both called "Driver API" — rename the
    # deployable to "Driver Service" to clarify that the service IS the API
    # and the deployable is what runs it.
    drv_d = by_name(g["deployables"], "Driver API")
    if drv_d:
        drv_d["name"] = "Driver Service"
        if not drv_d.get("description"):
            drv_d["description"] = (
                "REST + push backend powering the driver mobile apps. Owns the "
                "Driver API contract."
            )

    # The deployable "Customer API Gateway" exposes a service of the same
    # name. Rename the service to make the deployable/service distinction
    # readable in the UI.
    cag_s = by_name(g["services"], "Customer API Gateway")
    if cag_s:
        cag_s["name"] = "Customer Gateway API"

    # ── 2. Add two services that obvious deployables lacked ──────────────────
    needed_new_services = [
        {
            "name": "Customs Clearance API",
            "type": "rest",
            "description": (
                "Submits export declarations, queries clearance status, and "
                "uploads supporting documents to customs brokers."
            ),
            "endpoint": "https://groundwork.meridianfreight.internal/customs",
        },
        {
            "name": "Legacy CRM API",
            "type": "soap",
            "description": (
                "Legacy customer record API. Read-only since Q1 — Hubspot is "
                "the new system of record. Maintained for historical reads."
            ),
            "endpoint": "https://groundwork.meridianfreight.internal/legacy-crm",
        },
    ]
    for s in needed_new_services:
        if by_name(g["services"], s["name"]) is None:
            s["id"] = str(uuid.uuid4())
            g["services"].append(s)

    # ── 3. Add the matching exposes ─────────────────────────────────────────
    new_exposes = [
        ("Customs Clearance Integration", "Customs Clearance API", "8080", "https"),
        ("Legacy CRM",                    "Legacy CRM API",        "8443", "https"),
    ]
    for dep_name, svc_name, port, proto in new_exposes:
        dep = by_name(g["deployables"], dep_name)
        svc = by_name(g["services"], svc_name)
        if not dep or not svc:
            continue
        already = any(
            e["deployable_id"] == dep["id"] and e["service_id"] == svc["id"]
            for e in g["exposes"]
        )
        if already:
            continue
        g["exposes"].append({
            "id": str(uuid.uuid4()),
            "deployable_id": dep["id"],
            "service_id": svc["id"],
            "port": port,
            "protocol": proto,
        })

    # ── 4. Backfill dependencies. Each entry: (consumer, [(svc, criticality, auth)]) ──
    new_dependencies = [
        # Mobile apps + legacy app — auth + the driver API
        ("Driver App Android", [
            ("Auth Service API", "high", "jwt", "https"),
            ("Driver API",       "high", "jwt", "https"),
        ]),
        ("Driver App iOS", [
            ("Auth Service API", "high", "jwt", "https"),
            ("Driver API",       "high", "jwt", "https"),
        ]),
        ("Old Driver App (v1)", [
            ("Auth Service API", "high", "jwt", "https"),
            ("Driver API",       "high", "basic", "https"),
        ]),
        # Admin panel pulls everything important
        ("Admin Panel", [
            ("Auth Service API",      "high",   "jwt",  "https"),
            ("OMS REST API",          "high",   "jwt",  "https"),
            ("Fleet Management API",  "medium", "jwt",  "https"),
            ("Carrier Integration API","medium","jwt",  "https"),
            ("Customer Gateway API",  "medium", "jwt",  "https"),
            ("Audit Log API",         "low",    "jwt",  "https"),
            ("Notification Service API","low",  "jwt",  "https"),
        ]),
        # Reporting + analytics consumers
        ("Reporting Dashboard", [
            ("Analytics Platform API", "high",  "jwt", "https"),
            ("ETL Pipeline API",       "low",   "jwt", "https"),
            ("Auth Service API",       "high",  "jwt", "https"),
        ]),
        # Customs clearance flow
        ("Customs Clearance Integration", [
            ("Auth Service API",   "high",   "jwt", "https"),
            ("OMS REST API",       "high",   "jwt", "https"),
            ("File Storage API",   "medium", "jwt", "https"),
            ("Carrier Integration API", "medium", "jwt", "https"),
        ]),
        # Data warehouse loader
        ("Data Warehouse Loader", [
            ("OMS Event Stream",        "high",   "mtls",  "tcp"),
            ("OMS REST API",            "medium", "jwt",   "https"),
            ("Carrier Integration API", "low",    "jwt",   "https"),
            ("Audit Log API",           "low",    "jwt",   "https"),
            ("ETL Pipeline API",        "high",   "jwt",   "https"),
        ]),
        # Legacy CRM (read-only — minimal)
        ("Legacy CRM", [
            ("Auth Service API",  "high",   "basic", "https"),
            ("Email Service API", "low",    "jwt",   "https"),
        ]),
        # Platform leaf services that previously had no dependencies
        ("Audit Log", [
            ("Auth Service API",   "high", "mtls", "https"),
            ("Config Service API", "low",  "jwt",  "https"),
        ]),
        ("Carrier Integration", [
            ("Auth Service API",     "high",   "mtls", "https"),
            ("Config Service API",   "medium", "jwt",  "https"),
            ("Customs Clearance API","low",    "jwt",  "https"),
        ]),
        ("Email Service", [
            ("Config Service API", "high",   "jwt", "https"),
            ("File Storage API",   "low",    "jwt", "https"),
        ]),
        ("Event Bus", [
            ("Config Service API", "high", "jwt", "https"),
        ]),
        ("File Storage", [
            ("Auth Service API",   "high",   "mtls", "https"),
            ("Config Service API", "medium", "jwt",  "https"),
        ]),
        ("Geocoding Service", [
            ("Config Service API", "medium", "jwt", "https"),
        ]),
        ("SMS Gateway", [
            ("Config Service API", "medium", "jwt", "https"),
        ]),
        # The Driver Service backend itself depends on Auth + OMS
        ("Driver Service", [
            ("Auth Service API",  "high",   "jwt",  "https"),
            ("OMS REST API",      "high",   "jwt",  "https"),
            ("OMS Event Stream",  "medium", "mtls", "tcp"),
            ("File Storage API",  "low",    "jwt",  "https"),
        ]),
        # Tracking Page — currently only OMS, add Auth (anonymous tracking still pings auth for anti-abuse)
        ("Tracking Page", [
            ("Geocoding Service API", "low", "jwt", "https"),
        ]),
        # Notification Service — depends on Auth too
        ("Notification Service", [
            ("Auth Service API", "high", "mtls", "https"),
        ]),
        # Warehouse Management — likely depends on OMS + auth + file storage
        ("Warehouse Management", [
            ("Auth Service API",  "high",   "jwt",  "https"),
            ("OMS REST API",      "high",   "jwt",  "https"),
            ("File Storage API",  "medium", "jwt",  "https"),
        ]),
        # Invoice Engine — auth + OMS
        ("Invoice Engine", [
            ("Auth Service API",   "high", "jwt", "https"),
            ("OMS Event Stream",   "high", "mtls","tcp"),
            ("Email Service API",  "low",  "jwt", "https"),
        ]),
        # Dispatch Console
        ("Dispatch Console", [
            ("Auth Service API",     "high",   "jwt", "https"),
            ("Fleet Management API", "high",   "jwt", "https"),
            ("Geocoding Service API","medium", "jwt", "https"),
            ("Driver API",           "medium", "jwt", "https"),
        ]),
        # ETL pipeline — also needs auth + audit log
        ("ETL Pipeline", [
            ("Auth Service API",    "medium", "mtls","https"),
            ("Audit Log API",       "low",    "jwt", "https"),
            ("OMS Event Stream",    "high",   "mtls","tcp"),
        ]),
    ]

    services_by_name = {s["name"]: s for s in g["services"]}
    deployables_by_name = {d_["name"]: d_ for d_ in g["deployables"]}

    for dep_name, edges in new_dependencies:
        dep = deployables_by_name.get(dep_name)
        if not dep:
            print(f"  WARN: deployable {dep_name!r} not found, skipping")
            continue
        for svc_name, criticality, auth, proto in edges:
            svc = services_by_name.get(svc_name)
            if not svc:
                print(f"  WARN: service {svc_name!r} not found, skipping for {dep_name}")
                continue
            already = any(
                x["deployable_id"] == dep["id"] and x["service_id"] == svc["id"]
                for x in g["dependencies"]
            )
            if already:
                continue
            g["dependencies"].append({
                "id": str(uuid.uuid4()),
                "deployable_id": dep["id"],
                "service_id": svc["id"],
                "protocol": proto,
                "auth_method": auth,
                "criticality": criticality,
            })

    # ── 5. Add a few more contracts + SLAs to cover the highest-traffic APIs ──
    extra_contracts = [
        ("Customer Portal API",     "1.0", "openapi",
         "https://specs.meridianfreight.internal/customer-portal/v1.0.yaml"),
        ("Email Service API",       "1.4", "openapi",
         "https://specs.meridianfreight.internal/email/v1.4.yaml"),
        ("Notification Service API","2.0", "openapi",
         "https://specs.meridianfreight.internal/notify/v2.0.yaml"),
        ("Geocoding Service API",   "1.1", "openapi",
         "https://specs.meridianfreight.internal/geocoding/v1.1.yaml"),
        ("Fleet Management API",    "1.6", "openapi",
         "https://specs.meridianfreight.internal/fleet/v1.6.yaml"),
        ("Customs Clearance API",   "0.9", "openapi",
         "https://specs.meridianfreight.internal/customs/v0.9.yaml"),
    ]
    contracts_by_svc = {c["service_id"]: c for c in g["contracts"]}
    new_contract_ids: dict[str, str] = {}
    for svc_name, ver, fmt, url in extra_contracts:
        svc = services_by_name.get(svc_name)
        if not svc or svc["id"] in contracts_by_svc:
            continue
        cid = str(uuid.uuid4())
        g["contracts"].append({
            "id": cid,
            "service_id": svc["id"],
            "spec_url": url,
            "version": ver,
            "format": fmt,
        })
        new_contract_ids[svc_name] = cid

    extra_slas = [
        ("Customer Portal API",     "p99_latency_ms", "300",  "1h"),
        ("Customer Portal API",     "availability",   "99.9", "30d"),
        ("Email Service API",       "p99_latency_ms", "1500", "1h"),
        ("Notification Service API","p99_latency_ms", "750",  "1h"),
        ("Notification Service API","availability",   "99.9", "30d"),
        ("Geocoding Service API",   "p99_latency_ms", "400",  "1h"),
        ("Fleet Management API",    "availability",   "99.95","30d"),
        ("Fleet Management API",    "p99_latency_ms", "250",  "1h"),
    ]
    contracts_by_svc_id_now = {c["service_id"]: c["id"] for c in g["contracts"]}
    for svc_name, metric, target, window in extra_slas:
        svc = services_by_name.get(svc_name)
        if not svc:
            continue
        cid = contracts_by_svc_id_now.get(svc["id"])
        if not cid:
            continue
        already = any(
            s["contract_id"] == cid and s.get("metric") == metric and s.get("window") == window
            for s in g["slas"]
        )
        if already:
            continue
        g["slas"].append({
            "id": str(uuid.uuid4()),
            "contract_id": cid,
            "metric": metric,
            "target": target,
            "window": window,
        })

    save(d)

    # ── Summary ─────────────────────────────────────────────────────────────
    print("Fixture rewritten:")
    for k in ("deployables", "services", "exposes", "dependencies", "contracts", "slas"):
        print(f"  {k:<14} {len(g[k])}")


if __name__ == "__main__":
    main()
