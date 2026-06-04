#!/usr/bin/env python3
"""Load data/meridian_fixture.json into a running Manifold stack.

Cross-references in the fixture point at fixture-internal UUIDs that are
regenerated on every load. We resolve them by name against whatever ids the
running stack actually assigns.

Idempotent — entities are skipped if a record with the same name (or other
natural key) already exists.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Any, Callable
from urllib import request, error


# Trusted-header identity for auth'd stacks (e.g. the public showcase, where
# the apps require X-Manifold-User-* and the loader must write as an admin).
# Unset (dev / no-auth) => no headers, preserving the original behaviour.
def _auth_headers() -> dict[str, str]:
    uid = os.environ.get("MANIFOLD_USER_ID")
    if not uid:
        return {}
    return {
        "X-Manifold-User-Id": uid,
        "X-Manifold-User-Groups": os.environ.get("MANIFOLD_USER_GROUPS", "admin"),
    }


_AUTH = _auth_headers()


# ── Envelope helper ─────────────────────────────────────────────────────────


def _payload(env: dict) -> dict:
    """Meshql REST flattens {id, ...payload}. Older endpoints nest under
    'payload'. Accept either; return the field map without 'id'."""
    if "payload" in env and isinstance(env["payload"], dict):
        return env["payload"]
    return {k: v for k, v in env.items() if k != "id"}


# ── HTTP helpers ────────────────────────────────────────────────────────────


def http_json(method: str, url: str, body: dict | None = None) -> tuple[int, Any]:
    data = None
    headers = {"Accept": "application/json", **_AUTH}
    if body is not None:
        data = json.dumps(body).encode()
        headers["Content-Type"] = "application/json"
    req = request.Request(url, data=data, method=method, headers=headers)
    try:
        with request.urlopen(req, timeout=15) as resp:
            raw = resp.read()
            text = raw.decode() if raw else ""
            parsed = json.loads(text) if text else None
            return resp.status, parsed
    except error.HTTPError as e:
        raw = e.read().decode(errors="replace") if e.fp else ""
        return e.code, raw


def get(url: str) -> Any:
    status, body = http_json("GET", url)
    if status != 200:
        raise RuntimeError(f"GET {url} -> {status}: {body!r}")
    return body


def post(url: str, payload: dict) -> dict:
    status, body = http_json("POST", url, payload)
    if status not in (200, 201):
        raise RuntimeError(f"POST {url} -> {status}: {body!r}\npayload={payload}")
    return body


def delete(url: str) -> bool:
    status, _ = http_json("DELETE", url)
    return status in (200, 204)


# ── Core loader ─────────────────────────────────────────────────────────────


class Loader:
    def __init__(self, urls: dict[str, str]) -> None:
        self.urls = urls
        # Per-entity id maps keyed by natural key, e.g.
        #   self.ids[("groundwork","deployables")][name] = id
        self.ids: dict[tuple[str, str], dict[str, str]] = {}
        self.created: dict[str, int] = {}
        self.skipped: dict[str, int] = {}

    # API surface

    def url(self, app: str, entity: str) -> str:
        return f"{self.urls[app]}/{entity}/api"

    def hydrate_existing(self, app: str, entity: str, key: str = "name") -> dict[str, str]:
        """List the running app's existing records and index them by `key`."""
        existing = get(self.url(app, entity)) or []
        index: dict[str, str] = {}
        for env in existing:
            payload = _payload(env)
            k = payload.get(key)
            if k:
                index[k] = env["id"]
        self.ids[(app, entity)] = index
        return index

    def lookup(self, app: str, entity: str, name: str | None) -> str | None:
        if not name:
            return None
        return self.ids.get((app, entity), {}).get(name)

    def insert(
        self,
        app: str,
        entity: str,
        natural_key: str,
        payload: dict,
        post_fn: Callable[[dict], dict] | None = None,
    ) -> str | None:
        """Create the record, or return the existing id if `natural_key` matches."""
        key_field = next(iter(natural_key.items())) if isinstance(natural_key, dict) else None
        if key_field is None:
            raise ValueError("natural_key must be a single-key dict")
        kf, kv = key_field
        existing = self.ids.get((app, entity), {})
        if kv in existing:
            self.skipped[entity] = self.skipped.get(entity, 0) + 1
            return existing[kv]
        url = self.url(app, entity)
        env = post_fn(payload) if post_fn else post(url, payload)
        new_id = env["id"]
        existing[kv] = new_id
        self.ids[(app, entity)] = existing
        self.created[entity] = self.created.get(entity, 0) + 1
        return new_id

    # ── per-app loaders ────────────────────────────────────────────────────

    def load_union(self, fixture: dict) -> None:
        # Teams first (referenced by many things)
        self.hydrate_existing("union", "team", key="name")
        for t in fixture.get("teams", []):
            payload = {k: v for k, v in t.items() if k != "id" and v is not None}
            self.insert("union", "team", {"name": t["name"]}, payload)

        # People
        self.hydrate_existing("union", "person", key="name")
        for p in fixture.get("people", []):
            payload = {k: v for k, v in p.items() if k != "id" and v is not None}
            self.insert("union", "person", {"name": p["name"]}, payload)

        # TeamMembers — natural key = (person_name, team_name)
        self.hydrate_existing_team_members()
        for tm in fixture.get("team_members", []):
            person_name = self.name_in(fixture, "people", tm.get("person_id"))
            team_name = self.name_in(fixture, "teams", tm.get("team_id"))
            person_id = self.lookup("union", "person", person_name)
            team_id = self.lookup("union", "team", team_name)
            if not (person_id and team_id):
                self.skipped["team_members"] = self.skipped.get("team_members", 0) + 1
                continue
            tm_key = f"{person_name}|{team_name}"
            payload = {
                "person_id": person_id,
                "team_id": team_id,
            }
            if tm.get("role"):
                payload["role"] = tm["role"]
            self.insert("union", "team_member", {"person_team": tm_key}, payload)

    def hydrate_existing_team_members(self) -> None:
        existing = get(self.url("union", "team_member")) or []
        people_by_id = {v: k for k, v in self.ids[("union", "person")].items()}
        teams_by_id = {v: k for k, v in self.ids[("union", "team")].items()}
        index: dict[str, str] = {}
        for env in existing:
            p = _payload(env)
            person_name = people_by_id.get(p.get("person_id"))
            team_name = teams_by_id.get(p.get("team_id"))
            if person_name and team_name:
                index[f"{person_name}|{team_name}"] = env["id"]
        self.ids[("union", "team_member")] = index

    def load_groundwork(self, fixture: dict, fixture_full: dict) -> None:
        # Delete probe-test if present.
        deps = get(self.url("groundwork", "deployable")) or []
        for env in deps:
            if _payload(env).get("name") == "probe-test":
                if delete(f"{self.url('groundwork', 'deployable')}/{env['id']}"):
                    print("  · deleted probe-test deployable")

        # Deployables
        self.hydrate_existing("groundwork", "deployable", key="name")
        for d in fixture.get("deployables", []):
            payload = {k: v for k, v in d.items() if k not in ("id", "team_id") and v is not None}
            team_name = self.name_in(fixture_full["union"], "teams", d.get("team_id"))
            team_id = self.lookup("union", "team", team_name) if team_name else None
            if team_id:
                payload["team_id"] = team_id
            self.insert("groundwork", "deployable", {"name": d["name"]}, payload)

        # Services
        self.hydrate_existing("groundwork", "service", key="name")
        for s in fixture.get("services", []):
            payload = {k: v for k, v in s.items() if k != "id" and v is not None}
            self.insert("groundwork", "service", {"name": s["name"]}, payload)

        # Exposes — natural key = (deployable_name, service_name)
        self.hydrate_exposes_or_dependencies("exposes")
        for e in fixture.get("exposes", []):
            d_name = self.name_in(fixture, "deployables", e.get("deployable_id"))
            s_name = self.name_in(fixture, "services", e.get("service_id"))
            d_id = self.lookup("groundwork", "deployable", d_name)
            s_id = self.lookup("groundwork", "service", s_name)
            if not (d_id and s_id):
                self.skipped["exposes"] = self.skipped.get("exposes", 0) + 1
                continue
            payload = {"deployable_id": d_id, "service_id": s_id}
            for k in ("port", "protocol"):
                if e.get(k) is not None:
                    payload[k] = e[k]
            self.insert("groundwork", "exposes", {"pair": f"{d_name}|{s_name}"}, payload)

        # Dependencies — natural key = (deployable_name, service_name)
        self.hydrate_exposes_or_dependencies("dependency")
        for dep in fixture.get("dependencies", []):
            d_name = self.name_in(fixture, "deployables", dep.get("deployable_id"))
            s_name = self.name_in(fixture, "services", dep.get("service_id"))
            d_id = self.lookup("groundwork", "deployable", d_name)
            s_id = self.lookup("groundwork", "service", s_name)
            if not (d_id and s_id):
                self.skipped["dependencies"] = self.skipped.get("dependencies", 0) + 1
                continue
            payload = {"deployable_id": d_id, "service_id": s_id}
            for k in ("protocol", "auth_method", "criticality"):
                if dep.get(k) is not None:
                    payload[k] = dep[k]
            self.insert(
                "groundwork",
                "dependency",
                {"pair": f"{d_name}|{s_name}|{payload.get('protocol','')}"},
                payload,
            )

        # Contracts — natural key = (service_name, version)
        self.hydrate_contracts()
        for c in fixture.get("contracts", []):
            s_name = self.name_in(fixture, "services", c.get("service_id"))
            s_id = self.lookup("groundwork", "service", s_name)
            if not s_id:
                self.skipped["contracts"] = self.skipped.get("contracts", 0) + 1
                continue
            version = c.get("version") or ""
            key = f"{s_name}|{version}"
            payload = {"service_id": s_id}
            for k in ("spec_url", "version", "format"):
                if c.get(k) is not None:
                    payload[k] = c[k]
            self.insert("groundwork", "contract", {"key": key}, payload)
            # Stash the (service_name+version) → contract_id for SLAs below.
            self._contract_lookup_by_fixture_id = getattr(
                self, "_contract_lookup_by_fixture_id", {}
            )
            self._contract_lookup_by_fixture_id[c["id"]] = self.ids[
                ("groundwork", "contract")
            ][key]

        # SLAs — resolve contract via fixture contract id → live contract id
        contract_map = getattr(self, "_contract_lookup_by_fixture_id", {})
        self.hydrate_slas()
        for sla in fixture.get("slas", []):
            live_contract_id = contract_map.get(sla.get("contract_id"))
            if not live_contract_id:
                self.skipped["slas"] = self.skipped.get("slas", 0) + 1
                continue
            payload = {"contract_id": live_contract_id}
            for k in ("metric", "target", "window"):
                if sla.get(k) is not None:
                    payload[k] = sla[k]
            sla_key = f"{live_contract_id}|{payload.get('metric','')}|{payload.get('window','')}"
            self.insert("groundwork", "sla", {"key": sla_key}, payload)

    def hydrate_exposes_or_dependencies(self, entity: str) -> None:
        existing = get(self.url("groundwork", entity)) or []
        deps_by_id = {v: k for k, v in self.ids[("groundwork", "deployable")].items()}
        svcs_by_id = {v: k for k, v in self.ids[("groundwork", "service")].items()}
        index: dict[str, str] = {}
        for env in existing:
            p = _payload(env)
            d_name = deps_by_id.get(p.get("deployable_id"))
            s_name = svcs_by_id.get(p.get("service_id"))
            if d_name and s_name:
                if entity == "dependency":
                    index[f"{d_name}|{s_name}|{p.get('protocol','')}"] = env["id"]
                else:
                    index[f"{d_name}|{s_name}"] = env["id"]
        self.ids[("groundwork", entity)] = index

    def hydrate_contracts(self) -> None:
        existing = get(self.url("groundwork", "contract")) or []
        svcs_by_id = {v: k for k, v in self.ids[("groundwork", "service")].items()}
        index: dict[str, str] = {}
        for env in existing:
            p = _payload(env)
            s_name = svcs_by_id.get(p.get("service_id"))
            if s_name:
                index[f"{s_name}|{p.get('version','')}"] = env["id"]
        self.ids[("groundwork", "contract")] = index

    def hydrate_slas(self) -> None:
        existing = get(self.url("groundwork", "sla")) or []
        index: dict[str, str] = {}
        for env in existing:
            p = _payload(env)
            key = f"{p.get('contract_id','')}|{p.get('metric','')}|{p.get('window','')}"
            index[key] = env["id"]
        self.ids[("groundwork", "sla")] = index

    def load_union_workorders(self, fixture: dict, fixture_full: dict) -> None:
        # Hydrate by summary; same summary twice = same WO conceptually
        self.hydrate_existing("union", "work_order", key="summary")
        for wo in fixture.get("work_orders", []):
            team_name = self.name_in(fixture_full["union"], "teams", wo.get("team_id"))
            team_id = self.lookup("union", "team", team_name)
            if not team_id:
                self.skipped["work_orders"] = self.skipped.get("work_orders", 0) + 1
                continue
            payload = {"team_id": team_id, "summary": wo["summary"]}
            for k in ("status", "priority", "completed_at", "story_points"):
                if wo.get(k) is not None:
                    payload[k] = wo[k]
            d_name = self.name_in(fixture_full["groundwork"], "deployables", wo.get("deployable_id"))
            d_id = self.lookup("groundwork", "deployable", d_name) if d_name else None
            if d_id:
                payload["deployable_id"] = d_id
            cr_summary = self.name_in(
                fixture_full["cityhall"],
                "change_requests",
                wo.get("change_request_id"),
                name_field="summary",
            )
            cr_id = self.lookup("cityhall", "change_request", cr_summary) if cr_summary else None
            if cr_id:
                payload["change_request_id"] = cr_id
            self.insert("union", "work_order", {"summary": wo["summary"]}, payload)

    def load_cityhall(self, fixture: dict, fixture_full: dict) -> None:
        # OrgNodes — depth-first by parent. Multiple passes until all roots placed.
        self.hydrate_existing("cityhall", "org_node", key="name")
        nodes = list(fixture.get("org_nodes", []))
        nodes_by_fid = {n["id"]: n for n in nodes}
        remaining = list(nodes)
        guard = 0
        while remaining and guard < 10:
            guard += 1
            still: list[dict] = []
            for n in remaining:
                parent_fid = n.get("parent_id")
                parent_name = nodes_by_fid.get(parent_fid, {}).get("name") if parent_fid else None
                # If the node has a parent we haven't loaded, defer.
                if parent_fid and not self.lookup("cityhall", "org_node", parent_name):
                    still.append(n)
                    continue
                payload = {"name": n["name"], "kind": n["kind"]}
                if parent_fid and parent_name:
                    payload["parent_id"] = self.lookup("cityhall", "org_node", parent_name)
                team_name = self.name_in(fixture_full["union"], "teams", n.get("team_id"))
                team_id = self.lookup("union", "team", team_name) if team_name else None
                if team_id:
                    payload["team_id"] = team_id
                self.insert("cityhall", "org_node", {"name": n["name"]}, payload)
            remaining = still
        if remaining:
            for n in remaining:
                self.skipped["org_nodes"] = self.skipped.get("org_nodes", 0) + 1

        # Bylaws — natural key = (org_node_name, gate_type, priority)
        self.hydrate_bylaws()
        for b in fixture.get("bylaws", []):
            n_name = self.name_in(fixture, "org_nodes", b.get("org_node_id"))
            n_id = self.lookup("cityhall", "org_node", n_name)
            if not n_id:
                self.skipped["bylaws"] = self.skipped.get("bylaws", 0) + 1
                continue
            payload = {"org_node_id": n_id, "gate_type": b["gate_type"]}
            for k in ("priority", "description", "conditions", "window", "quiesce_for", "approvers"):
                if b.get(k) is not None:
                    payload[k] = b[k]
            key = f"{n_name}|{b.get('gate_type','')}|{b.get('priority','')}"
            self.insert("cityhall", "bylaw", {"key": key}, payload)

        # Change requests — natural key = summary
        self.hydrate_existing("cityhall", "change_request", key="summary")
        for cr in fixture.get("change_requests", []):
            payload = {"summary": cr["summary"]}
            for k in ("description", "tier", "status", "target_versions"):
                if cr.get(k) is not None:
                    payload[k] = cr[k]
            person_name = self.name_in(fixture_full["union"], "people", cr.get("requested_by"))
            person_id = self.lookup("union", "person", person_name) if person_name else None
            if person_id:
                payload["requested_by"] = person_id
            # target_deployables: comma-separated list of fixture deployable IDs.
            tgt = cr.get("target_deployables") or ""
            if tgt:
                live_ids: list[str] = []
                for fid in [t.strip() for t in tgt.split(",") if t.strip()]:
                    d_name = self.name_in(fixture_full["groundwork"], "deployables", fid)
                    d_id = self.lookup("groundwork", "deployable", d_name) if d_name else None
                    if d_id:
                        live_ids.append(d_id)
                if live_ids:
                    payload["target_deployables"] = json.dumps(live_ids)
            self.insert("cityhall", "change_request", {"summary": cr["summary"]}, payload)

    def hydrate_bylaws(self) -> None:
        existing = get(self.url("cityhall", "bylaw")) or []
        nodes_by_id = {v: k for k, v in self.ids[("cityhall", "org_node")].items()}
        index: dict[str, str] = {}
        for env in existing:
            p = _payload(env)
            n_name = nodes_by_id.get(p.get("org_node_id"))
            if n_name:
                key = f"{n_name}|{p.get('gate_type','')}|{p.get('priority','')}"
                index[key] = env["id"]
        self.ids[("cityhall", "bylaw")] = index

    def load_yard(self, fixture: dict, fixture_full: dict) -> None:
        # Infrastructure
        self.hydrate_existing("yard", "test_infrastructure", key="name")
        for i in fixture.get("test_infrastructure", []):
            payload = {k: v for k, v in i.items() if k != "id" and v is not None}
            self.insert("yard", "test_infrastructure", {"name": i["name"]}, payload)

        # Mock sources
        self.hydrate_existing("yard", "mock_source", key="name")
        for m in fixture.get("mock_sources", []):
            payload = {k: v for k, v in m.items() if k != "id" and v is not None}
            self.insert("yard", "mock_source", {"name": m["name"]}, payload)

        # Data sources
        self.hydrate_existing("yard", "data_source", key="name")
        for ds in fixture.get("data_sources", []):
            payload = {k: v for k, v in ds.items() if k != "id" and v is not None}
            self.insert("yard", "data_source", {"name": ds["name"]}, payload)

        # Test environments and data syncs both reference envs by id, so load
        # envs first, then come back for data_syncs.
        self.hydrate_existing("yard", "test_environment", key="name")
        for env in fixture.get("test_environments", []):
            payload = {
                k: v
                for k, v in env.items()
                if k not in ("id", "deployable_id", "infrastructure_id", "mock_source_id")
                and v is not None
            }
            d_name = self.name_in(fixture_full["groundwork"], "deployables", env.get("deployable_id"))
            d_id = self.lookup("groundwork", "deployable", d_name) if d_name else None
            if d_id:
                payload["deployable_id"] = d_id
            i_name = self.name_in(fixture, "test_infrastructure", env.get("infrastructure_id"))
            i_id = self.lookup("yard", "test_infrastructure", i_name) if i_name else None
            if i_id:
                payload["infrastructure_id"] = i_id
            m_name = self.name_in(fixture, "mock_sources", env.get("mock_source_id"))
            m_id = self.lookup("yard", "mock_source", m_name) if m_name else None
            if m_id:
                payload["mock_source_id"] = m_id
            self.insert("yard", "test_environment", {"name": env["name"]}, payload)

        # Data syncs — natural key = (target_env_name, kind, source_ref)
        # Since DataSync has no `name`, we synthesise a stable key from refs.
        self.hydrate_data_syncs()
        for dsy in fixture.get("data_syncs", []):
            payload = {k: v for k, v in dsy.items() if k not in (
                "id", "target_env_id", "source_env_id", "source_data_id"
            ) and v is not None}
            tgt_env_name = self.name_in(
                fixture, "test_environments", dsy.get("target_env_id")
            )
            tgt_env_id = (
                self.lookup("yard", "test_environment", tgt_env_name)
                if tgt_env_name else None
            )
            if not tgt_env_id:
                self.skipped["data_syncs"] = self.skipped.get("data_syncs", 0) + 1
                continue
            payload["target_env_id"] = tgt_env_id

            src_env_name = self.name_in(
                fixture, "test_environments", dsy.get("source_env_id")
            )
            src_env_id = (
                self.lookup("yard", "test_environment", src_env_name)
                if src_env_name else None
            )
            if src_env_id:
                payload["source_env_id"] = src_env_id

            src_ds_name = self.name_in(
                fixture, "data_sources", dsy.get("source_data_id")
            )
            src_ds_id = (
                self.lookup("yard", "data_source", src_ds_name)
                if src_ds_name else None
            )
            if src_ds_id:
                payload["source_data_id"] = src_ds_id

            sync_key = (
                f"{tgt_env_name}|{payload.get('kind','')}"
                f"|{src_env_name or src_ds_name or ''}"
            )
            self.insert("yard", "data_sync", {"key": sync_key}, payload)
            # Stash fixture data_sync_id → live data_sync_id for SyncRun lookups.
            self._sync_lookup_by_fixture_id = getattr(
                self, "_sync_lookup_by_fixture_id", {}
            )
            self._sync_lookup_by_fixture_id[dsy["id"]] = self.ids[
                ("yard", "data_sync")
            ][sync_key]
            # Per-fixture-sync target/source for SyncRun resolution.
            self._sync_refs_by_fixture_id = getattr(
                self, "_sync_refs_by_fixture_id", {}
            )
            self._sync_refs_by_fixture_id[dsy["id"]] = {
                "target_env_id": tgt_env_id,
                "source_env_id": src_env_id,
                "source_data_id": src_ds_id,
            }

        # Sync runs — natural key = (live_sync_id, started_at). Each run
        # references its parent data_sync by fixture id; we resolve to the
        # live sync id and also denormalise target/source ids onto the run
        # for cheap UI queries (matches the schema, which permits both).
        self.hydrate_sync_runs()
        for run in fixture.get("sync_runs", []):
            fixture_sync_id = run.get("data_sync_id")
            sync_map = getattr(self, "_sync_lookup_by_fixture_id", {})
            refs_map = getattr(self, "_sync_refs_by_fixture_id", {})
            live_sync_id = sync_map.get(fixture_sync_id)
            refs = refs_map.get(fixture_sync_id) or {}
            target_env_id = refs.get("target_env_id")
            if not (live_sync_id and target_env_id):
                self.skipped["sync_runs"] = self.skipped.get("sync_runs", 0) + 1
                continue
            payload: dict = {
                "data_sync_id": live_sync_id,
                "target_env_id": target_env_id,
            }
            if refs.get("source_env_id"):
                payload["source_env_id"] = refs["source_env_id"]
            if refs.get("source_data_id"):
                payload["source_data_id"] = refs["source_data_id"]
            for k in (
                "status", "started_at", "finished_at",
                "duration_minutes", "triggered_by", "source_revision",
                "masking_summary", "error_message",
            ):
                if run.get(k) is not None:
                    payload[k] = run[k]
            run_key = f"{live_sync_id}|{payload.get('started_at','')}"
            self.insert("yard", "sync_run", {"key": run_key}, payload)

        # Test suites
        self.hydrate_existing("yard", "test_suite", key="name")
        for s in fixture.get("test_suites", []):
            payload = {
                k: v for k, v in s.items()
                if k not in ("id", "deployable_id") and v is not None
            }
            d_name = self.name_in(fixture_full["groundwork"], "deployables", s.get("deployable_id"))
            d_id = self.lookup("groundwork", "deployable", d_name) if d_name else None
            if d_id:
                payload["deployable_id"] = d_id
            self.insert("yard", "test_suite", {"name": s["name"]}, payload)

        # Test runs — natural key = (env_name, started_at) when started_at set
        self.hydrate_test_runs()
        for r in fixture.get("test_runs", []):
            env_name = self.name_in(fixture, "test_environments", r.get("test_environment_id"))
            env_id = self.lookup("yard", "test_environment", env_name)
            if not env_id:
                self.skipped["test_runs"] = self.skipped.get("test_runs", 0) + 1
                continue
            payload = {"test_environment_id": env_id}
            for k in ("started_at", "finished_at", "status", "duration_minutes", "cost_actual"):
                if r.get(k) is not None:
                    payload[k] = r[k]
            cr_summary_id = r.get("change_request_id")
            cr_summary = self.name_in(
                fixture_full["cityhall"], "change_requests", cr_summary_id, name_field="summary"
            )
            cr_id = self.lookup("cityhall", "change_request", cr_summary) if cr_summary else None
            if cr_id:
                payload["change_request_id"] = cr_id
            ts_name = self.name_in(fixture, "test_suites", r.get("test_suite_id"))
            ts_id = self.lookup("yard", "test_suite", ts_name) if ts_name else None
            if ts_id:
                payload["test_suite_id"] = ts_id
            team_name = self.name_in(fixture_full["union"], "teams", r.get("team_id"))
            team_id = self.lookup("union", "team", team_name) if team_name else None
            if team_id:
                payload["team_id"] = team_id
            run_key = f"{env_name}|{payload.get('started_at','')}"
            self.insert("yard", "test_run", {"key": run_key}, payload)

    def hydrate_data_syncs(self) -> None:
        existing = get(self.url("yard", "data_sync")) or []
        envs_by_id = {v: k for k, v in self.ids[("yard", "test_environment")].items()}
        ds_by_id = {v: k for k, v in self.ids.get(("yard", "data_source"), {}).items()}
        index: dict[str, str] = {}
        for env in existing:
            p = _payload(env)
            tgt_name = envs_by_id.get(p.get("target_env_id"))
            src_name = envs_by_id.get(p.get("source_env_id")) or ds_by_id.get(
                p.get("source_data_id")
            )
            if tgt_name:
                key = f"{tgt_name}|{p.get('kind','')}|{src_name or ''}"
                index[key] = env["id"]
        self.ids[("yard", "data_sync")] = index

    def compute_plans_and_gantts(self, cityhall_base: str) -> None:
        """For each ChangeRequest, trigger plan + gantt computation via the
        existing cityhall endpoints. Idempotent in the sense that we skip CRs
        which already have a plan."""
        existing_crs = get(f"{cityhall_base}/change_request/api") or []
        existing_plans = get(f"{cityhall_base}/deployment_plan/api") or []
        existing_gantts = get(f"{cityhall_base}/gantt_output/api") or []

        plans_by_cr: dict[str, str] = {}
        for env in existing_plans:
            p = _payload(env)
            cr_id = p.get("change_request_id")
            if cr_id:
                plans_by_cr[cr_id] = env["id"]

        gantts_by_plan: dict[str, str] = {}
        for env in existing_gantts:
            p = _payload(env)
            plan_id = p.get("deployment_plan_id")
            if plan_id:
                gantts_by_plan[plan_id] = env["id"]

        for cr_env in existing_crs:
            cr_id = cr_env["id"]
            if cr_id in plans_by_cr:
                self.skipped["deployment_plans"] = (
                    self.skipped.get("deployment_plans", 0) + 1
                )
                plan_id = plans_by_cr[cr_id]
            else:
                # Trigger plan compute. The tier comes from the CR payload.
                status, body = http_json(
                    "POST", f"{cityhall_base}/change_request/{cr_id}/plan", {}
                )
                if status not in (200, 201):
                    print(
                        f"  ! plan compute failed for {cr_id}: {status} {str(body)[:200]}"
                    )
                    continue
                plan_id = body["id"] if isinstance(body, dict) else None
                if not plan_id:
                    print(f"  ! plan compute returned no id for {cr_id}")
                    continue
                self.created["deployment_plans"] = (
                    self.created.get("deployment_plans", 0) + 1
                )

            if plan_id in gantts_by_plan:
                self.skipped["gantt_outputs"] = (
                    self.skipped.get("gantt_outputs", 0) + 1
                )
                continue
            status, body = http_json(
                "POST", f"{cityhall_base}/deployment_plan/{plan_id}/gantt", None
            )
            if status not in (200, 201):
                print(
                    f"  ! gantt render failed for plan {plan_id}: {status} {str(body)[:200]}"
                )
                continue
            self.created["gantt_outputs"] = (
                self.created.get("gantt_outputs", 0) + 1
            )

    def hydrate_test_runs(self) -> None:
        existing = get(self.url("yard", "test_run")) or []
        envs_by_id = {v: k for k, v in self.ids[("yard", "test_environment")].items()}
        index: dict[str, str] = {}
        for env in existing:
            p = _payload(env)
            e_name = envs_by_id.get(p.get("test_environment_id"))
            if e_name:
                key = f"{e_name}|{p.get('started_at','')}"
                index[key] = env["id"]
        self.ids[("yard", "test_run")] = index

    def hydrate_sync_runs(self) -> None:
        existing = get(self.url("yard", "sync_run")) or []
        index: dict[str, str] = {}
        for env in existing:
            p = _payload(env)
            key = f"{p.get('data_sync_id','')}|{p.get('started_at','')}"
            index[key] = env["id"]
        self.ids[("yard", "sync_run")] = index

    # Helper: look up a record's `name` (or other field) inside the fixture by id.
    @staticmethod
    def name_in(section: dict, entity: str, fid: str | None, name_field: str = "name") -> str | None:
        if not fid:
            return None
        for r in section.get(entity, []) or []:
            if r.get("id") == fid:
                return r.get(name_field)
        return None


# ── Main ────────────────────────────────────────────────────────────────────


def main(argv: list[str]) -> int:
    p = argparse.ArgumentParser(description="Load Meridian fixture into a Manifold stack.")
    p.add_argument("--base-url-groundwork", default="http://localhost:3050")
    p.add_argument("--base-url-union", default="http://localhost:3051")
    p.add_argument("--base-url-cityhall", default="http://localhost:3052")
    p.add_argument("--base-url-yard", default="http://localhost:3053")
    p.add_argument(
        "--fixture",
        default=str(Path(__file__).resolve().parent / "meridian_fixture.json"),
    )
    args = p.parse_args(argv)

    urls = {
        "groundwork": args.base_url_groundwork.rstrip("/"),
        "union": args.base_url_union.rstrip("/"),
        "cityhall": args.base_url_cityhall.rstrip("/"),
        "yard": args.base_url_yard.rstrip("/"),
    }

    fixture = json.loads(Path(args.fixture).read_text())
    print(f"Loading fixture for: {fixture.get('company','?')}")
    print(f"  groundwork: {urls['groundwork']}")
    print(f"  union     : {urls['union']}")
    print(f"  cityhall  : {urls['cityhall']}")
    print(f"  yard      : {urls['yard']}")

    L = Loader(urls)

    # Order: union teams + people first; groundwork; cityhall (needs union +
    # groundwork); union work_orders (need deployables and change_requests);
    # yard (needs groundwork + cityhall + union).
    print("\n[1/6] Union — teams, people, team_members")
    L.load_union(fixture["union"])

    print("\n[2/6] Groundwork — deployables → services → exposes/deps → contracts → slas")
    L.load_groundwork(fixture["groundwork"], fixture)

    print("\n[3/6] Cityhall — org_nodes → bylaws → change_requests")
    L.load_cityhall(fixture["cityhall"], fixture)

    print("\n[4/6] Union — work orders (need deployables + change_requests)")
    L.load_union_workorders(fixture["union"], fixture)

    print("\n[5/6] Yard — infra/mock/data sources/data syncs → envs → suites → runs")
    L.load_yard(fixture["yard"], fixture)

    print("\n[6/6] Cityhall — compute deployment plans + Gantt for each ChangeRequest")
    L.compute_plans_and_gantts(urls["cityhall"])

    # Summary
    print("\n── Load summary ──────────────────────────────────────────────")
    keys = sorted(set(L.created) | set(L.skipped))
    print(f"{'entity':<22}{'created':>10}{'skipped':>10}")
    for k in keys:
        print(f"{k:<22}{L.created.get(k,0):>10}{L.skipped.get(k,0):>10}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
