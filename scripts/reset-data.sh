#!/usr/bin/env bash
# Wipe and reseed the Manifold stack with the Meridian fixture.
#
#   1. docker-compose down -v   (drop volumes)
#   2. docker-compose up -d
#   3. wait for /health on all four services
#   4. python3 data/load_fixture.py
#
# Run from anywhere — script cd's to the repo root.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

ports=(3050 3051 3052 3053)
names=(groundwork union cityhall yard)

step() { printf "\n\033[1;34m▸\033[0m %s\n" "$*"; }
ok()   { printf "  \033[32m✓\033[0m %s\n" "$*"; }
fail() { printf "  \033[31m✗\033[0m %s\n" "$*" >&2; }

step "Stopping stack and wiping volumes"
docker compose down -v
ok "stack down, volumes removed"

step "Starting stack"
docker compose up -d
ok "compose up issued"

step "Waiting for health endpoints (max 60s)"
deadline=$((SECONDS + 60))
for i in "${!ports[@]}"; do
  port="${ports[$i]}"
  name="${names[$i]}"
  while :; do
    if curl -sf "http://localhost:${port}/health" 2>/dev/null | grep -q '"status":"ok"'; then
      ok "${name} (:${port}) ready"
      break
    fi
    if (( SECONDS >= deadline )); then
      fail "${name} (:${port}) did not become healthy within 60s"
      docker compose logs --tail=50 "${name}" || true
      exit 1
    fi
    sleep 2
  done
done

step "Loading Meridian fixture"
if ! python3 data/load_fixture.py; then
  fail "load_fixture.py exited non-zero"
  exit 1
fi
ok "fixture loaded"

step "Done"
echo "  Manifold stack reseeded with the Meridian dataset."
echo "  Groundwork :3050  Union :3051  Cityhall :3052  Yard :3053"
