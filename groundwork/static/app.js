// groundwork — service catalog UI
// Vanilla JS ES module. No build step. No framework.
//
// Architecture: the deployable is the unit of attention. The home screen
// (Catalog) is a deployable card grid; clicking a card opens a Detail page
// that aggregates everything related to that deployable — services it
// exposes, what it depends on, what depends on it, its test environments
// (federated from yard), and a focused 1-hop subgraph. The full dependency
// graph is its own tab. The other entity types (services, exposes,
// dependencies, contracts, slas) live behind an Admin menu for direct CRUD.

import {
  esc,
  apiFetch, gqlQuery,
  loadManifoldConfig, getManifoldConfig, crossLink,
  setStatus, setError, updateFooterMeta,
  openModal,
} from '/static/manifold-ui.js';

// ── Entity config ─────────────────────────────────────────────────────────────
//
// Each entry describes ONE entity type for the Admin backstage CRUD lists.
// The non-deployable types stay here because they're still the underlying
// shape of the data; the redesign just demotes them from primary navigation.

const ENTITIES = {
  deployables: {
    api: '/deployable/api',
    graph: {
      path: '/deployable/graph',
      list: '{ getAll { id name description repo_url team_id deployment_status team { id name } } }',
    },
    label: 'deployable',
    newFields: [
      { name: 'name',        label: 'Name',        type: 'text', required: true },
      { name: 'description', label: 'Description', type: 'textarea' },
      { name: 'repo_url',    label: 'Repo URL',    type: 'text' },
      { name: 'team_id',     label: 'Team id (Union)', type: 'text' },
      { name: 'deployment_status', label: 'Status', type: 'select',
        options: ['', 'operational', 'degraded', 'down', 'unknown'] },
    ],
    detailFields: [
      { name: 'description', label: 'description', type: 'textarea' },
      { name: 'repo_url',    label: 'repo_url',    type: 'text' },
      { name: 'team_id',     label: 'team_id',     type: 'text' },
      { name: 'deployment_status', label: 'deployment_status', type: 'select',
        options: ['', 'operational', 'degraded', 'down', 'unknown'] },
    ],
    primaryField: 'name',
    getRowLabel: (p) => p.name || 'unnamed',
  },

  services: {
    api: '/service/api',
    graph: {
      path: '/service/graph',
      list: '{ getAll { id name type description endpoint } }',
    },
    label: 'service',
    newFields: [
      { name: 'name', label: 'Name', type: 'text', required: true },
      { name: 'type', label: 'Type', type: 'select',
        options: ['', 'database', 'api', 'queue', 'cache', 'message-broker', 'storage', 'auth', 'other'] },
      { name: 'description', label: 'Description', type: 'textarea' },
      { name: 'endpoint', label: 'Endpoint', type: 'text' },
    ],
    detailFields: [
      { name: 'type', label: 'type', type: 'select',
        options: ['', 'database', 'api', 'queue', 'cache', 'message-broker', 'storage', 'auth', 'other'] },
      { name: 'description', label: 'description', type: 'textarea' },
      { name: 'endpoint', label: 'endpoint', type: 'text' },
    ],
    primaryField: 'name',
    getRowLabel: (p) => p.name || 'unnamed',
  },

  exposes: {
    api: '/exposes/api',
    graph: {
      path: '/exposes/graph',
      list: '{ getAll { id deployable_id service_id port protocol } }',
    },
    label: 'exposes',
    newFields: [
      { name: 'deployable_id', label: 'Deployable', type: 'ref', refKey: 'deployables', required: true },
      { name: 'service_id',    label: 'Service',    type: 'ref', refKey: 'services',    required: true },
      { name: 'port',          label: 'Port',       type: 'text' },
      { name: 'protocol',      label: 'Protocol',   type: 'select',
        options: ['', 'http', 'https', 'grpc', 'tcp', 'udp', 'other'] },
    ],
    detailFields: [
      { name: 'port',     label: 'port',     type: 'text' },
      { name: 'protocol', label: 'protocol', type: 'select',
        options: ['', 'http', 'https', 'grpc', 'tcp', 'udp', 'other'] },
    ],
    primaryField: 'deployable_id',
    getRowLabel: (p, data) => {
      const dep = data.deployables.find(d => d.id === p.deployable_id)?.name || p.deployable_id || '?';
      const svc = data.services.find(s => s.id === p.service_id)?.name || p.service_id || '?';
      return `${dep} ⇒ ${svc}`;
    },
    readonlyInDetail: ['deployable_id', 'service_id'],
  },

  dependencies: {
    api: '/dependency/api',
    graph: {
      path: '/dependency/graph',
      list: '{ getAll { id deployable_id service_id protocol auth_method criticality } }',
    },
    label: 'dependency',
    newFields: [
      { name: 'deployable_id', label: 'Deployable', type: 'ref', refKey: 'deployables', required: true },
      { name: 'service_id',    label: 'Service',    type: 'ref', refKey: 'services',    required: true },
      { name: 'criticality',   label: 'Criticality', type: 'select',
        options: ['', 'high', 'medium', 'low'], default: 'medium' },
      { name: 'protocol',    label: 'Protocol',    type: 'text' },
      { name: 'auth_method', label: 'Auth method', type: 'text' },
    ],
    detailFields: [
      { name: 'criticality', label: 'criticality', type: 'select',
        options: ['', 'high', 'medium', 'low'] },
      { name: 'protocol',    label: 'protocol',    type: 'text' },
      { name: 'auth_method', label: 'auth_method', type: 'text' },
    ],
    primaryField: 'deployable_id',
    getRowLabel: (p, data) => {
      const dep = data.deployables.find(d => d.id === p.deployable_id)?.name || p.deployable_id || '?';
      const svc = data.services.find(s => s.id === p.service_id)?.name || p.service_id || '?';
      return `${dep} → ${svc}`;
    },
    readonlyInDetail: ['deployable_id', 'service_id'],
  },

  contracts: {
    api: '/contract/api',
    graph: {
      path: '/contract/graph',
      list: '{ getAll { id service_id spec_url version format } }',
    },
    label: 'contract',
    newFields: [
      { name: 'service_id', label: 'Service',  type: 'ref', refKey: 'services', required: true },
      { name: 'spec_url',   label: 'Spec URL', type: 'text' },
      { name: 'version',    label: 'Version',  type: 'text' },
      { name: 'format',     label: 'Format',   type: 'select',
        options: ['', 'openapi', 'grpc', 'graphql', 'asyncapi', 'other'] },
    ],
    detailFields: [
      { name: 'spec_url', label: 'spec_url', type: 'text' },
      { name: 'version',  label: 'version',  type: 'text' },
      { name: 'format',   label: 'format',   type: 'select',
        options: ['', 'openapi', 'grpc', 'graphql', 'asyncapi', 'other'] },
    ],
    primaryField: 'service_id',
    getRowLabel: (p, data) => {
      const svc = data.services.find(s => s.id === p.service_id)?.name || p.service_id || '?';
      const ver = p.version ? `v${p.version}` : '';
      const fmt = p.format || '';
      return [svc, ver, fmt].filter(Boolean).join(' · ');
    },
    readonlyInDetail: ['service_id'],
  },

  slas: {
    api: '/sla/api',
    graph: {
      path: '/sla/graph',
      list: '{ getAll { id contract_id metric target window } }',
    },
    label: 'SLA',
    newFields: [
      { name: 'contract_id', label: 'Contract', type: 'ref', refKey: 'contracts', required: true },
      { name: 'metric', label: 'Metric', type: 'text' },
      { name: 'target', label: 'Target', type: 'text' },
      { name: 'window', label: 'Window', type: 'text' },
    ],
    detailFields: [
      { name: 'metric', label: 'metric', type: 'text' },
      { name: 'target', label: 'target', type: 'text' },
      { name: 'window', label: 'window', type: 'text' },
    ],
    primaryField: 'contract_id',
    getRowLabel: (p, data) => {
      const c = data.contracts.find(x => x.id === p.contract_id);
      const svcName = c
        ? (data.services.find(s => s.id === c.service_id)?.name || '?')
        : '?';
      return `${p.metric || '?'}: ${p.target || '?'} [${svcName}]`;
    },
    readonlyInDetail: ['contract_id'],
  },
};

// Contracts lookups want labels like "v1.2 (Customer API)" rather than the
// raw id — built once when needed by the ref-select renderer.
function contractsForLookup() {
  return state.data.contracts.map(c => {
    const svcName = state.data.services.find(s => s.id === c.service_id)?.name || '?';
    const label = `${c.version || c.id.slice(0, 8)} (${svcName})`;
    return { id: c.id, name: label };
  });
}

// ── State ─────────────────────────────────────────────────────────────────────

const state = {
  // 'catalog' | 'graph' | 'governance' | `deployable/${id}` | `admin/${entityKey}`
  screen: 'catalog',
  data: { deployables: [], exposes: [], services: [], dependencies: [], contracts: [], slas: [] },
  // Catalog filters
  search: '',
  statusFilter: '',
  // Admin screen: which row is expanded for inline edit
  expandedId: null,
  // Federated yard test envs, keyed by deployable_id (best-effort cache).
  testEnvsByDeployable: new Map(),
  // Graph instances — tracked so re-entering tabs doesn't leak listeners.
  graph: { cy: null, tableMode: false },
  detailGraph: { cy: null, deployableId: null },
};

// ── Data load ─────────────────────────────────────────────────────────────────

async function loadEntity(entityKey) {
  const cfg = ENTITIES[entityKey];
  const data = await gqlQuery(cfg.graph.path, cfg.graph.list);
  state.data[entityKey] = Array.isArray(data.getAll) ? data.getAll : [];
}

async function loadAll() {
  await Promise.all(Object.keys(ENTITIES).map(loadEntity));
}

async function createRecord(entityKey, fields) {
  const cfg = ENTITIES[entityKey];
  return apiFetch(cfg.api, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(fields),
  });
}

async function updateRecord(entityKey, id, fields) {
  const cfg = ENTITIES[entityKey];
  return apiFetch(`${cfg.api}/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(fields),
  });
}

async function deleteRecord(entityKey, id) {
  const cfg = ENTITIES[entityKey];
  return apiFetch(`${cfg.api}/${id}`, { method: 'DELETE' });
}

// Cross-app: ask yard for the test environments tied to this deployable.
// Best-effort — auth may not pass cross-origin to yard.tildarc.com, in
// which case the section just renders empty. CORS is open on meshql-server.
async function fetchTestEnvsForDeployable(deployableId) {
  if (state.testEnvsByDeployable.has(deployableId)) {
    return state.testEnvsByDeployable.get(deployableId);
  }
  const yardBase = getManifoldConfig()?.yard_public_url;
  if (!yardBase) return [];
  const url = `${yardBase.replace(/\/$/, '')}/test_environment/graph`;
  try {
    const data = await gqlQuery(url,
      `{ getByDeployableId(deployable_id: "${deployableId}") { id name kind cost_per_hour spinup_minutes teardown_policy max_duration_minutes } }`);
    const envs = data?.getByDeployableId || [];
    state.testEnvsByDeployable.set(deployableId, envs);
    return envs;
  } catch {
    state.testEnvsByDeployable.set(deployableId, []);
    return [];
  }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/** @param {string} sel @param {ParentNode} [root] */
const $  = (sel, root = document) => root.querySelector(sel);
/** @param {string} sel @param {ParentNode} [root] */
const $$ = (sel, root = document) => Array.from(root.querySelectorAll(sel));

// Build an intra-app anchor.
function intraLink(screen, label) {
  return `<a href="#${esc(screen)}">${esc(label)}</a>`;
}

function deployableById(id) {
  return state.data.deployables.find(d => d.id === id);
}
function serviceById(id) {
  return state.data.services.find(s => s.id === id);
}

// Services this deployable exposes (resolved through the `exposes` table).
function servicesExposedBy(deployableId) {
  const ids = state.data.exposes
    .filter(e => e.deployable_id === deployableId)
    .map(e => ({ exposesId: e.id, service_id: e.service_id, port: e.port, protocol: e.protocol }));
  return ids.map(row => ({ ...row, service: serviceById(row.service_id) }));
}

// Dependencies declared by this deployable (rows in the `dependencies` table).
function dependenciesOf(deployableId) {
  return state.data.dependencies.filter(d => d.deployable_id === deployableId);
}

// Reverse-direction: every (deployable, service) pair where the service is
// exposed by this deployable AND some other deployable has a dependency on
// that service. Returns array of { dependency, depender }.
function dependentsOf(deployableId) {
  const exposedServiceIds = new Set(
    state.data.exposes
      .filter(e => e.deployable_id === deployableId)
      .map(e => e.service_id)
  );
  if (!exposedServiceIds.size) return [];
  const out = [];
  for (const dep of state.data.dependencies) {
    if (!exposedServiceIds.has(dep.service_id)) continue;
    if (dep.deployable_id === deployableId) continue; // skip self-loops
    out.push({
      dependency: dep,
      depender: deployableById(dep.deployable_id),
      service: serviceById(dep.service_id),
    });
  }
  return out;
}

// ── Screen routing ────────────────────────────────────────────────────────────

function setScreen(screen) {
  state.screen = screen;
  state.search = '';
  state.expandedId = null;
  const searchEl = $('#search');
  if (searchEl) searchEl.value = '';
  closeMenu();

  // Tab highlight: only catalog/graph get the underline; detail and admin
  // screens leave the tabs unhighlighted (you're "inside" something).
  $$('#primary-nav .tab').forEach(t => {
    t.classList.toggle('active', t.dataset.screen === screen);
  });

  // Status filter is only meaningful on Catalog.
  const filterWrap = $('#status-filter-wrap');
  if (filterWrap) filterWrap.classList.toggle('visible', screen === 'catalog');

  // Title hint
  if (screen === 'catalog')              document.title = 'Groundwork — Catalog';
  else if (screen === 'graph')           document.title = 'Groundwork — Graph';
  else if (screen === 'governance')      document.title = 'Groundwork — Governance';
  else if (screen.startsWith('deployable/')) {
    const d = deployableById(screen.slice('deployable/'.length));
    document.title = `Groundwork — ${d?.name || 'deployable'}`;
  } else if (screen.startsWith('admin/')) {
    document.title = `Groundwork — Admin · ${screen.slice('admin/'.length)}`;
  }

  if (location.hash.slice(1) !== screen) location.hash = screen;
  render();
}

function render() {
  const root = $('#screen-root');
  root.innerHTML = '';
  // Tear down any prior detail-graph cytoscape so listeners don't pile up.
  if (state.detailGraph.cy) {
    try { state.detailGraph.cy.destroy(); } catch { /* no-op */ }
    state.detailGraph.cy = null;
    state.detailGraph.deployableId = null;
  }

  if (state.screen === 'catalog')                        renderCatalog(root);
  else if (state.screen === 'graph')                     renderGraphScreen(root);
  else if (state.screen === 'governance')                renderGovernance(root);
  else if (state.screen.startsWith('deployable/'))       renderDeployableDetail(root, state.screen.slice('deployable/'.length));
  else if (state.screen.startsWith('admin/'))            renderAdmin(root, state.screen.slice('admin/'.length));
  else                                                   renderCatalog(root);
}

// ── Catalog screen ────────────────────────────────────────────────────────────

function renderCatalog(root) {
  const all = state.data.deployables;
  const needle = state.search.trim().toLowerCase();
  let visible = all;
  if (state.statusFilter) {
    visible = visible.filter(d => (d.deployment_status || 'unknown') === state.statusFilter);
  }
  if (needle) {
    visible = visible.filter(d =>
      (d.name || '').toLowerCase().includes(needle) ||
      (d.description || '').toLowerCase().includes(needle) ||
      (d.team?.name || '').toLowerCase().includes(needle)
    );
  }

  updateFooterMeta(
    `${all.length} ${all.length === 1 ? 'deployable' : 'deployables'} · ${state.data.services.length} services`
  );

  const head = document.createElement('div');
  head.className = 'section-head';
  head.innerHTML =
    `<div>
       <h1>Deployables</h1>
       <div class="meta">${visible.length} of ${all.length}</div>
     </div>`;
  root.appendChild(head);

  if (visible.length === 0) {
    if (all.length === 0) {
      root.appendChild(emptyState({
        title: 'No deployables yet',
        lede: 'A deployable is a thing you ship and operate. Register one to start mapping what depends on what.',
        hint: 'Press <kbd>n</kbd> to add the first one.',
      }));
    } else {
      const div = document.createElement('div');
      div.className = 'empty';
      div.textContent = 'No deployables match your filters.';
      root.appendChild(div);
    }
    return;
  }

  // Pre-compute per-deployable rollup counts (services exposed + dependencies).
  // Faster than calling the helpers per-card while rendering.
  const exposesCount = new Map();
  for (const e of state.data.exposes) {
    exposesCount.set(e.deployable_id, (exposesCount.get(e.deployable_id) || 0) + 1);
  }
  const depsCount = new Map();
  for (const d of state.data.dependencies) {
    depsCount.set(d.deployable_id, (depsCount.get(d.deployable_id) || 0) + 1);
  }

  // Sort: by name, but unstaffed (no team) sink to the bottom so the visual
  // weight up top is the things that actually have owners.
  const sorted = [...visible].sort((a, b) => {
    const aTeam = !!a.team?.name;
    const bTeam = !!b.team?.name;
    if (aTeam !== bTeam) return aTeam ? -1 : 1;
    return (a.name || '').localeCompare(b.name || '');
  });

  const grid = document.createElement('div');
  grid.className = 'card-grid';
  for (const d of sorted) {
    grid.appendChild(buildDeployableCard(d, exposesCount.get(d.id) || 0, depsCount.get(d.id) || 0));
  }
  root.appendChild(grid);
}

function buildDeployableCard(d, nExposes, nDeps) {
  const card = document.createElement('div');
  card.className = 'card deployable-card';
  card.dataset.id = d.id;

  const teamHtml = d.team?.name
    ? `<span class="team">${crossLink('union', 'teams', d.team_id, d.team.name)}</span>`
    : `<span class="team" style="font-style:italic">unstaffed</span>`;

  const status = d.deployment_status || 'unknown';
  card.innerHTML = `
    <div class="head">
      <span class="name">${esc(d.name || 'unnamed')}</span>
      <span class="status-label">
        <span class="status-dot ${esc(status)}" title="deployment status: ${esc(status)}"></span>
        ${esc(status)}
      </span>
    </div>
    ${d.description ? `<div class="desc">${esc(d.description)}</div>` : ''}
    <div class="stats">
      <span class="stat"><strong>${nExposes}</strong> exposes</span>
      <span class="stat"><strong>${nDeps}</strong> depends on</span>
    </div>
    <div class="foot">
      ${teamHtml}
      <span class="id">${d.id ? esc(d.id.slice(0, 8)) : ''}</span>
    </div>
  `;
  // The cross-app team link is inside the card; clicks on it shouldn't
  // also navigate to the deployable detail.
  card.querySelectorAll('a').forEach(a => a.addEventListener('click', e => e.stopPropagation()));
  card.addEventListener('click', () => setScreen(`deployable/${d.id}`));
  return card;
}

function emptyState({ title, lede, hint }) {
  const div = document.createElement('div');
  div.className = 'empty-card';
  div.innerHTML =
    `<span class="empty-mark">§</span>` +
    `<h3>${esc(title)}</h3>` +
    `<p class="lede">${esc(lede)}</p>` +
    `<p class="hint">${hint}</p>`;
  return div;
}

// ── Governance screen ────────────────────────────────────────────────────────
//
// The compliance lens: are the things that SHOULD be governed actually
// governed? Three axes:
//   - Used services SHOULD have a contract (a service that has dependents
//     but no contract is a real risk — nothing pinned about its shape).
//   - Contracts SHOULD have at least one SLA (otherwise no operational
//     accountability behind the API shape).
//   - Deployables SHOULD have a team owner (unstaffed = nobody on the hook).
//
// Stat tiles up top give the rolled-up percentages; the sections below
// list the specific gaps so they can be acted on, then a "by team" view
// for the ownership cut.

function renderGovernance(root) {
  const deployables  = state.data.deployables;
  const services     = state.data.services;
  const contracts    = state.data.contracts;
  const slas         = state.data.slas;
  const exposes      = state.data.exposes;
  const dependencies = state.data.dependencies;

  const usedServiceIds       = new Set(dependencies.map(d => d.service_id));
  const contractedServiceIds = new Set(contracts.map(c => c.service_id));
  const slaContractIds       = new Set(slas.map(s => s.contract_id));

  const usedServices            = services.filter(s => usedServiceIds.has(s.id));
  const servicesUsedNoContract  = usedServices.filter(s => !contractedServiceIds.has(s.id));
  const contractsNoSla          = contracts.filter(c => !slaContractIds.has(c.id));
  const unstaffedDeployables    = deployables.filter(d => !d.team_id);

  updateFooterMeta(
    `${servicesUsedNoContract.length} contract gap${servicesUsedNoContract.length === 1 ? '' : 's'} · ` +
    `${contractsNoSla.length} SLA gap${contractsNoSla.length === 1 ? '' : 's'} · ` +
    `${unstaffedDeployables.length} unstaffed`
  );

  const head = document.createElement('div');
  head.className = 'section-head';
  head.innerHTML = `
    <div>
      <h1>Governance</h1>
      <div class="meta">Coverage of contracts, SLAs, and ownership across the catalog.</div>
    </div>`;
  root.appendChild(head);

  // ── Stat tiles ──
  const tiles = document.createElement('div');
  tiles.className = 'stat-tiles';
  tiles.appendChild(buildStatTile({
    label: 'Deployables',
    figure: deployables.length,
  }));
  tiles.appendChild(buildStatTile({
    label: 'With team',
    figure: deployables.length - unstaffedDeployables.length,
    denom: deployables.length,
    gap: unstaffedDeployables.length > 0,
    sub: unstaffedDeployables.length > 0
      ? `${unstaffedDeployables.length} unstaffed`
      : 'fully staffed',
  }));
  tiles.appendChild(buildStatTile({
    label: 'Used services with contract',
    figure: usedServices.length - servicesUsedNoContract.length,
    denom: usedServices.length,
    gap: servicesUsedNoContract.length > 0,
    sub: usedServices.length === 0
      ? 'no services depended on'
      : (servicesUsedNoContract.length > 0
          ? `${servicesUsedNoContract.length} without contract`
          : 'fully covered'),
  }));
  tiles.appendChild(buildStatTile({
    label: 'Contracts with SLA',
    figure: contracts.length - contractsNoSla.length,
    denom: contracts.length,
    gap: contractsNoSla.length > 0,
    sub: contracts.length === 0
      ? 'no contracts yet'
      : (contractsNoSla.length > 0
          ? `${contractsNoSla.length} missing SLA`
          : 'fully covered'),
  }));
  root.appendChild(tiles);

  // ── Gaps section: used services without a contract ──
  root.appendChild(buildContractGapsSection(servicesUsedNoContract, exposes, deployables, dependencies));

  // ── Gaps section: contracts without an SLA ──
  root.appendChild(buildSlaGapsSection(contractsNoSla, services));

  // ── Gaps section: unstaffed deployables ──
  if (unstaffedDeployables.length > 0) {
    root.appendChild(buildUnstaffedSection(unstaffedDeployables));
  }

  // ── By team: rollup ──
  root.appendChild(buildByTeamSection(deployables, exposes, contracts, slas));
}

/**
 * @param {object} spec
 * @param {string} spec.label
 * @param {number | string} spec.figure
 * @param {number} [spec.denom]
 * @param {boolean} [spec.gap]
 * @param {string} [spec.sub]
 */
function buildStatTile({ label, figure, denom, gap, sub }) {
  const tile = document.createElement('div');
  tile.className = 'stat-tile' + (gap ? ' gap' : '');
  const pct = (denom != null && denom > 0)
    ? ` <span class="denom">/ ${denom}</span>`
    : '';
  tile.innerHTML = `
    <div class="label">${esc(label)}</div>
    <div class="figure">${figure}${pct}</div>
    ${sub ? `<div class="sub">${esc(sub)}</div>` : ''}
  `;
  return tile;
}

function buildContractGapsSection(servicesNoContract, exposes, deployables, dependencies) {
  const section = document.createElement('section');
  section.className = 'detail-section';
  section.innerHTML = `<h2>Services in use without a contract</h2>`;
  if (servicesNoContract.length === 0) {
    const lede = document.createElement('div');
    lede.className = 'lede';
    lede.textContent = 'Every used service has at least one contract registered.';
    section.appendChild(lede);
    return section;
  }
  // Pre-compute, per service: which deployable exposes it, and how many
  // dependents (rows in `dependencies`) point at it. Helps the operator
  // prioritise — a service used by 6 deployables is a bigger gap than one
  // used by 1.
  const exposesByService = new Map();
  for (const e of exposes) {
    if (!exposesByService.has(e.service_id)) exposesByService.set(e.service_id, []);
    exposesByService.get(e.service_id).push(e.deployable_id);
  }
  const dependentsByService = new Map();
  for (const d of dependencies) {
    dependentsByService.set(d.service_id, (dependentsByService.get(d.service_id) || 0) + 1);
  }

  const list = document.createElement('div');
  list.className = 'relationship-list';
  // Sort by dependent count desc — most-depended-on gaps first.
  const sorted = [...servicesNoContract].sort((a, b) =>
    (dependentsByService.get(b.id) || 0) - (dependentsByService.get(a.id) || 0)
  );
  for (const svc of sorted) {
    const providerIds = exposesByService.get(svc.id) || [];
    const providers = providerIds
      .map(id => deployables.find(d => d.id === id))
      .filter(Boolean);
    const providerHtml = providers.length
      ? providers.map(p => `<a href="#deployable/${esc(p.id)}">${esc(p.name || p.id)}</a>`).join(', ')
      : '<span style="font-style:italic">no provider registered</span>';
    const nDeps = dependentsByService.get(svc.id) || 0;
    const r = document.createElement('div');
    r.className = 'row';
    r.innerHTML = `
      <span class="target">
        ${esc(svc.name || svc.id)}
        <span class="target-meta">exposed by ${providerHtml}</span>
      </span>
      <span class="badge medium">${nDeps} dep${nDeps === 1 ? '' : 's'}</span>
      <span></span>
    `;
    list.appendChild(r);
  }
  section.appendChild(list);
  return section;
}

function buildSlaGapsSection(contractsNoSla, services) {
  const section = document.createElement('section');
  section.className = 'detail-section';
  section.innerHTML = `<h2>Contracts without an SLA</h2>`;
  if (contractsNoSla.length === 0) {
    const lede = document.createElement('div');
    lede.className = 'lede';
    lede.textContent = 'Every contract has at least one SLA attached.';
    section.appendChild(lede);
    return section;
  }
  const list = document.createElement('div');
  list.className = 'relationship-list';
  for (const c of contractsNoSla) {
    const svc = services.find(s => s.id === c.service_id);
    const svcName = svc?.name || c.service_id;
    const ver = c.version ? `v${esc(c.version)}` : '';
    const fmt = c.format || '';
    const r = document.createElement('div');
    r.className = 'row';
    r.innerHTML = `
      <span class="target">
        ${esc(svcName)}
        <span class="target-meta">${[ver, fmt].filter(Boolean).join(' · ') || c.id.slice(0, 8)}</span>
      </span>
      ${fmt ? `<span class="pill">${esc(fmt)}</span>` : '<span></span>'}
      <span></span>
    `;
    list.appendChild(r);
  }
  section.appendChild(list);
  return section;
}

function buildUnstaffedSection(unstaffed) {
  const section = document.createElement('section');
  section.className = 'detail-section';
  section.innerHTML = `<h2>Deployables without a team</h2>`;
  const list = document.createElement('div');
  list.className = 'relationship-list';
  for (const d of unstaffed) {
    const status = d.deployment_status || 'unknown';
    const r = document.createElement('div');
    r.className = 'row';
    r.innerHTML = `
      <span class="target">
        <a href="#deployable/${esc(d.id)}">${esc(d.name || d.id)}</a>
        ${d.description ? `<span class="target-meta">${esc(d.description)}</span>` : ''}
      </span>
      <span class="status-label"><span class="status-dot ${esc(status)}"></span>${esc(status)}</span>
      <span></span>
    `;
    list.appendChild(r);
  }
  section.appendChild(list);
  return section;
}

function buildByTeamSection(deployables, exposes, contracts, slas) {
  const section = document.createElement('section');
  section.className = 'detail-section';
  section.innerHTML = `<h2>By team</h2>`;

  // Group deployables by team name (or 'Unstaffed').
  const groups = new Map();
  const UNSTAFFED = '— Unstaffed';
  for (const d of deployables) {
    const key = d.team?.name || UNSTAFFED;
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(d);
  }
  // Sort: named teams alphabetically, Unstaffed last.
  const sortedKeys = [...groups.keys()].sort((a, b) => {
    if (a === UNSTAFFED) return 1;
    if (b === UNSTAFFED) return -1;
    return a.localeCompare(b);
  });

  // Per-team aggregates: contract count = sum over each deployable's
  // exposed services that have at least one contract; SLA count = sum
  // over contracts that have at least one SLA. Approximation — services
  // shared across teams will get counted once per team that exposes them.
  const contractsByService = new Map();
  for (const c of contracts) {
    if (!contractsByService.has(c.service_id)) contractsByService.set(c.service_id, []);
    contractsByService.get(c.service_id).push(c);
  }
  const slasByContract = new Map();
  for (const s of slas) {
    if (!slasByContract.has(s.contract_id)) slasByContract.set(s.contract_id, 0);
    slasByContract.set(s.contract_id, slasByContract.get(s.contract_id) + 1);
  }
  const teamContractAggregate = (deps) => {
    const svcIds = new Set();
    for (const d of deps) {
      for (const e of exposes) {
        if (e.deployable_id === d.id) svcIds.add(e.service_id);
      }
    }
    let cContracts = 0;
    let cSlas = 0;
    for (const sid of svcIds) {
      const cs = contractsByService.get(sid) || [];
      cContracts += cs.length;
      for (const c of cs) cSlas += (slasByContract.get(c.id) || 0);
    }
    return { svcCount: svcIds.size, cContracts, cSlas };
  };

  for (const key of sortedKeys) {
    const deps = groups.get(key);
    const isUnstaffed = key === UNSTAFFED;
    const agg = teamContractAggregate(deps);
    const wrap = document.createElement('div');
    wrap.className = 'team-group' + (isUnstaffed ? ' unstaffed' : '');
    const meta = isUnstaffed
      ? `${deps.length} deployable${deps.length === 1 ? '' : 's'}`
      : `${deps.length} deployable${deps.length === 1 ? '' : 's'} · ${agg.svcCount} service${agg.svcCount === 1 ? '' : 's'} exposed · ${agg.cContracts} contract${agg.cContracts === 1 ? '' : 's'} · ${agg.cSlas} SLA${agg.cSlas === 1 ? '' : 's'}`;
    const headHtml = `
      <div class="team-head">
        <span class="team-name">${esc(key)}</span>
        <span class="team-meta">${meta}</span>
      </div>`;
    const sortedDeps = [...deps].sort((a, b) => (a.name || '').localeCompare(b.name || ''));
    const depHtml = sortedDeps.map(d => {
      const status = d.deployment_status || 'unknown';
      return `<div>
        <a href="#deployable/${esc(d.id)}">${esc(d.name || d.id)}</a>
        <span class="dep-status">${esc(status)}</span>
      </div>`;
    }).join('');
    wrap.innerHTML = headHtml + `<div class="team-deployables">${depHtml}</div>`;
    section.appendChild(wrap);
  }
  return section;
}

// ── Deployable detail screen ─────────────────────────────────────────────────

function renderDeployableDetail(root, id) {
  const d = deployableById(id);
  if (!d) {
    root.innerHTML = `<a href="#catalog" class="back-link">Catalog</a>
      <div class="empty">Deployable not found.</div>`;
    return;
  }

  updateFooterMeta(`${d.name || id}`);

  // ── Back link ──
  const back = document.createElement('a');
  back.href = '#catalog';
  back.className = 'back-link';
  back.textContent = 'Catalog';
  root.appendChild(back);

  // ── Header ──
  const status = d.deployment_status || 'unknown';
  const teamHtml = d.team?.name
    ? crossLink('union', 'teams', d.team_id, d.team.name)
    : '<span style="font-style:italic">unstaffed</span>';
  const repoHtml = d.repo_url
    ? `<a href="${esc(d.repo_url)}" target="_blank" rel="noopener">${esc(stripScheme(d.repo_url))}</a>`
    : '<span style="font-style:italic">no repo</span>';

  const header = document.createElement('div');
  header.className = 'detail-header';
  header.innerHTML = `
    <div class="title-block">
      <h1>${esc(d.name || 'unnamed')}</h1>
      <div class="meta">
        <span class="status-label">
          <span class="status-dot ${esc(status)}"></span>${esc(status)}
        </span>
        <span>·</span>
        <span>${teamHtml}</span>
        <span>·</span>
        <span>${repoHtml}</span>
      </div>
      ${d.description ? `<p class="description">${esc(d.description)}</p>` : ''}
    </div>
    <div class="actions">
      <button class="primary" id="btn-edit-deployable">Edit</button>
      <button class="danger" id="btn-delete-deployable">Delete</button>
    </div>
  `;
  root.appendChild(header);
  $('#btn-edit-deployable', header).addEventListener('click', () => editDeployable(d));
  $('#btn-delete-deployable', header).addEventListener('click', () => deleteDeployable(d));

  // ── Services exposed ──
  const exposed = servicesExposedBy(id);
  root.appendChild(renderServicesSection(exposed, d.id));

  // ── Dependencies ──
  const deps = dependenciesOf(id);
  root.appendChild(renderDependenciesSection(deps, d.id));

  // ── Dependents ──
  const dependents = dependentsOf(id);
  root.appendChild(renderDependentsSection(dependents));

  // ── Test environments (federated from yard) ──
  const envsSection = renderEnvsSectionShell(d.id);
  root.appendChild(envsSection);
  // Populate async after initial render
  fetchTestEnvsForDeployable(d.id).then(envs => {
    const slot = $('.envs-slot', envsSection);
    if (slot) populateEnvsList(slot, envs, d.id);
  });

  // ── Focused subgraph (1-hop neighborhood) ──
  const graphSection = renderSubgraphSection();
  root.appendChild(graphSection);
  // Cytoscape needs the container in the DOM before instantiating; do it
  // on the next frame so layout is settled.
  requestAnimationFrame(() => renderFocusedGraph(d.id));
}

function stripScheme(url) {
  return url.replace(/^https?:\/\//, '');
}

function renderServicesSection(exposed, deployableId) {
  const section = document.createElement('section');
  section.className = 'detail-section';
  const addId = `add-exposes-${deployableId}`;
  section.innerHTML = `
    <h2>Services exposed
      <button class="ghost section-add" id="${addId}">+ Add</button>
    </h2>`;

  if (exposed.length === 0) {
    const lede = document.createElement('div');
    lede.className = 'lede';
    lede.textContent = "This deployable doesn't expose any services yet.";
    section.appendChild(lede);
  } else {
    const list = document.createElement('div');
    list.className = 'relationship-list';
    for (const row of exposed) {
      const svcName = row.service?.name || row.service_id;
      const port = row.port ? ` :${esc(row.port)}` : '';
      const protocol = row.protocol || '';
      const r = document.createElement('div');
      r.className = 'row';
      r.innerHTML = `
        <span class="target"><a href="#admin/services">${esc(svcName)}</a>${port}</span>
        ${protocol ? `<span class="pill">${esc(protocol)}</span>` : '<span></span>'}
        <button class="row-del" title="Remove" aria-label="Remove">×</button>
      `;
      $('.row-del', r).addEventListener('click', async () => {
        if (!confirm(`Stop exposing ${svcName}?`)) return;
        try {
          await deleteRecord('exposes', row.exposesId);
          await loadEntity('exposes');
          setStatus('Removed');
          render();
        } catch (e) { setError(e); }
      });
      list.appendChild(r);
    }
    section.appendChild(list);
  }

  setTimeout(() => {
    $(`#${addId}`)?.addEventListener('click', () => addExposesForDeployable(deployableId));
  }, 0);
  return section;
}

function renderDependenciesSection(deps, deployableId) {
  const section = document.createElement('section');
  section.className = 'detail-section';
  const addId = `add-dep-${deployableId}`;
  section.innerHTML = `
    <h2>Depends on
      <button class="ghost section-add" id="${addId}">+ Add</button>
    </h2>`;

  if (deps.length === 0) {
    const lede = document.createElement('div');
    lede.className = 'lede';
    lede.textContent = 'No declared dependencies yet.';
    section.appendChild(lede);
  } else {
    const list = document.createElement('div');
    list.className = 'relationship-list';
    for (const dep of deps) {
      const svcName = serviceById(dep.service_id)?.name || dep.service_id;
      // Which deployable provides this service?
      const provider = state.data.exposes.find(e => e.service_id === dep.service_id);
      const providerDep = provider ? deployableById(provider.deployable_id) : null;
      const providerHtml = providerDep
        ? `<span class="target-meta">from ${esc(providerDep.name || providerDep.id)}</span>`
        : `<span class="target-meta" style="font-style:italic">no provider registered</span>`;
      const crit = dep.criticality || '';
      const r = document.createElement('div');
      r.className = 'row';
      r.innerHTML = `
        <span class="target">
          ${providerDep ? `<a href="#deployable/${esc(providerDep.id)}">${esc(svcName)}</a>` : esc(svcName)}
          ${providerHtml}
        </span>
        ${crit ? `<span class="badge ${esc(crit)}">${esc(crit)}</span>` : '<span></span>'}
        <button class="row-del" title="Remove" aria-label="Remove">×</button>
      `;
      $('.row-del', r).addEventListener('click', async () => {
        if (!confirm(`Remove dependency on ${svcName}?`)) return;
        try {
          await deleteRecord('dependencies', dep.id);
          await loadEntity('dependencies');
          setStatus('Removed');
          render();
        } catch (e) { setError(e); }
      });
      list.appendChild(r);
    }
    section.appendChild(list);
  }

  setTimeout(() => {
    $(`#${addId}`)?.addEventListener('click', () => addDependencyForDeployable(deployableId));
  }, 0);
  return section;
}

function renderDependentsSection(dependents) {
  const section = document.createElement('section');
  section.className = 'detail-section';
  section.innerHTML = `<h2>Depended on by</h2>`;
  if (dependents.length === 0) {
    const lede = document.createElement('div');
    lede.className = 'lede';
    lede.textContent = 'Nothing currently registered as depending on this deployable.';
    section.appendChild(lede);
  } else {
    const list = document.createElement('div');
    list.className = 'relationship-list';
    for (const { dependency, depender, service } of dependents) {
      const depName = depender?.name || dependency.deployable_id;
      const svcName = service?.name || dependency.service_id;
      const crit = dependency.criticality || '';
      const r = document.createElement('div');
      r.className = 'row';
      r.innerHTML = `
        <span class="target">
          ${depender ? `<a href="#deployable/${esc(depender.id)}">${esc(depName)}</a>` : esc(depName)}
          <span class="target-meta">via ${esc(svcName)}</span>
        </span>
        ${crit ? `<span class="badge ${esc(crit)}">${esc(crit)}</span>` : '<span></span>'}
        <span></span>
      `;
      list.appendChild(r);
    }
    section.appendChild(list);
  }
  return section;
}

function renderEnvsSectionShell(deployableId) {
  const section = document.createElement('section');
  section.className = 'detail-section';
  const yardBase = getManifoldConfig()?.yard_public_url;
  const yardLink = yardBase
    ? `<a href="${esc(yardBase.replace(/\/$/, ''))}#environments" target="_blank" rel="noopener">Open in Yard ↗</a>`
    : '';
  section.innerHTML = `
    <h2>Test environments
      <span class="section-add">${yardLink}</span>
    </h2>
    <div class="envs-slot lede">Loading…</div>
  `;
  return section;
}

function populateEnvsList(slot, envs, deployableId) {
  slot.classList.remove('lede');
  slot.innerHTML = '';
  if (envs.length === 0) {
    slot.className = 'lede';
    slot.textContent = 'No test environments registered for this deployable in Yard.';
    return;
  }
  const list = document.createElement('div');
  list.className = 'relationship-list';
  for (const env of envs) {
    const cost = env.cost_per_hour ? `$${parseFloat(env.cost_per_hour).toFixed(2)}/h` : '—';
    const spinup = env.spinup_minutes ? `${env.spinup_minutes}m spinup` : '';
    const r = document.createElement('div');
    r.className = 'row';
    r.innerHTML = `
      <span class="target">
        ${esc(env.name || env.id)}
        <span class="target-meta">${[env.teardown_policy, spinup, cost].filter(Boolean).join(' · ')}</span>
      </span>
      <span class="pill">${esc(env.kind || 'unknown')}</span>
      <span></span>
    `;
    list.appendChild(r);
  }
  slot.appendChild(list);
}

function renderSubgraphSection() {
  const section = document.createElement('section');
  section.className = 'detail-section';
  section.innerHTML = `
    <h2>Neighborhood</h2>
    <div class="dep-mini-graph" id="dep-mini-graph"></div>
  `;
  return section;
}

// ── Deployable mutations ──────────────────────────────────────────────────────

async function createNewDeployable() {
  const payload = await openModal({
    title: 'New deployable',
    fields: ENTITIES.deployables.newFields,
    submit: 'Create',
  });
  if (!payload) return;
  try {
    const created = await createRecord('deployables', payload);
    await loadEntity('deployables');
    setStatus(`Created ${created.name || 'deployable'}`);
    if (created?.id) setScreen(`deployable/${created.id}`);
    else render();
  } catch (e) { setError(e); }
}

async function editDeployable(d) {
  const fields = ENTITIES.deployables.newFields.map(f => ({ ...f, default: d[f.name] ?? '' }));
  const payload = await openModal({
    title: `Edit ${d.name || 'deployable'}`,
    fields,
    submit: 'Save',
  });
  if (!payload) return;
  try {
    await updateRecord('deployables', d.id, payload);
    await loadEntity('deployables');
    setStatus('Saved');
    render();
  } catch (e) { setError(e); }
}

async function deleteDeployable(d) {
  if (!confirm(`Delete deployable "${d.name || d.id}"? This does not remove its exposes/dependencies.`)) return;
  try {
    await deleteRecord('deployables', d.id);
    await loadEntity('deployables');
    setStatus('Deleted');
    setScreen('catalog');
  } catch (e) { setError(e); }
}

async function addDependencyForDeployable(deployableId) {
  // Caller is already on the deployable's detail page — fix the deployable_id
  // so the modal only asks for what's new (the service and metadata).
  const fields = ENTITIES.dependencies.newFields.filter(f => f.name !== 'deployable_id');
  const payload = await openModal({
    title: 'Add dependency',
    fields,
    lookupRef: (refKey) => state.data[refKey] || [],
    submit: 'Add',
  });
  if (!payload) return;
  payload.deployable_id = deployableId;
  try {
    await createRecord('dependencies', payload);
    await loadEntity('dependencies');
    setStatus('Added');
    render();
  } catch (e) { setError(e); }
}

async function addExposesForDeployable(deployableId) {
  const fields = ENTITIES.exposes.newFields.filter(f => f.name !== 'deployable_id');
  const payload = await openModal({
    title: 'Expose service',
    fields,
    lookupRef: (refKey) => state.data[refKey] || [],
    submit: 'Add',
  });
  if (!payload) return;
  payload.deployable_id = deployableId;
  try {
    await createRecord('exposes', payload);
    await loadEntity('exposes');
    setStatus('Added');
    render();
  } catch (e) { setError(e); }
}

// ── Focused subgraph (1-hop neighborhood around a deployable) ────────────────

function renderFocusedGraph(deployableId) {
  const cyEl = document.getElementById('dep-mini-graph');
  if (!cyEl) return;
  if (typeof cytoscape !== 'function') {
    cyEl.textContent = 'cytoscape failed to load';
    return;
  }

  const allNodes = composeGraphNodes(state.data.deployables);
  const allEdges = composeGraphEdges(state.data.dependencies, state.data.exposes);
  const nodeIds = new Set(allNodes.map(n => n.data.id));

  // Keep only edges touching our deployable, then keep only nodes that
  // participate in those edges + the focal deployable itself.
  const edges = allEdges.filter(e =>
    (e.data.source === deployableId || e.data.target === deployableId) &&
    nodeIds.has(e.data.source) && nodeIds.has(e.data.target)
  );
  const keepIds = new Set([deployableId]);
  for (const e of edges) {
    keepIds.add(e.data.source);
    keepIds.add(e.data.target);
  }
  const nodes = allNodes.filter(n => keepIds.has(n.data.id));

  if (state.detailGraph.cy) {
    try { state.detailGraph.cy.destroy(); } catch { /* no-op */ }
  }

  const cy = cytoscape({
    container: cyEl,
    elements: [...nodes, ...edges],
    style: GRAPH_STYLE_LIGHT,
    layout: {
      name: 'cose',
      animate: false,
      idealEdgeLength: 90,
      fit: true,
      padding: 30,
      randomize: false,
      nodeRepulsion: 200000,
      numIter: 700,
    },
    wheelSensitivity: 0.2,
    minZoom: 0.3,
    maxZoom: 3,
  });
  state.detailGraph.cy = cy;
  state.detailGraph.deployableId = deployableId;

  // Highlight the focal deployable.
  cy.$(`node[id = "${deployableId}"]`).addClass('focal');

  cy.on('tap', 'node', evt => {
    const otherId = evt.target.id();
    if (otherId !== deployableId) setScreen(`deployable/${otherId}`);
  });
}

// ── Admin (backstage CRUD) ────────────────────────────────────────────────────

function renderAdmin(root, entityKey) {
  const cfg = ENTITIES[entityKey];
  if (!cfg) {
    root.innerHTML = `<a href="#catalog" class="back-link">Catalog</a>
      <div class="empty">Unknown entity type "${esc(entityKey)}"</div>`;
    return;
  }

  const items = state.data[entityKey] || [];
  const needle = state.search.trim().toLowerCase();
  const visible = needle
    ? items.filter(item => cfg.getRowLabel(item, state.data).toLowerCase().includes(needle))
    : items;

  updateFooterMeta(`${items.length} ${cfg.label}${items.length === 1 ? '' : 's'}`);

  const back = document.createElement('a');
  back.href = '#catalog';
  back.className = 'back-link';
  back.textContent = 'Catalog';
  root.appendChild(back);

  const head = document.createElement('div');
  head.className = 'section-head';
  const labelPlural = `${cfg.label[0].toUpperCase()}${cfg.label.slice(1)}s`;
  head.innerHTML = `
    <div>
      <h1>${esc(labelPlural)}</h1>
      <div class="meta">${visible.length} of ${items.length}</div>
    </div>
  `;
  root.appendChild(head);

  if (visible.length === 0) {
    root.appendChild(emptyState({
      title: `No ${cfg.label}s yet`,
      lede: needle
        ? 'No matches for that search.'
        : `Press n to add the first ${cfg.label}.`,
      hint: needle ? '' : 'Press <kbd>n</kbd> to add the first one.',
    }));
    return;
  }

  const list = document.createElement('ul');
  list.className = 'admin-list';
  for (const item of visible) list.appendChild(buildAdminRow(entityKey, item));
  root.appendChild(list);
}

function buildAdminRow(entityKey, item) {
  const cfg = ENTITIES[entityKey];
  const id = item.id;
  const label = cfg.getRowLabel(item, state.data);

  const li = document.createElement('li');
  li.className = 'entity-row' + (state.expandedId === id ? ' expanded' : '');
  li.dataset.id = id;

  const header = document.createElement('div');
  header.className = 'entity-row-header';
  header.setAttribute('role', 'button');
  header.setAttribute('tabindex', '0');
  header.setAttribute('aria-expanded', String(state.expandedId === id));
  header.innerHTML = `
    <span class="expand-icon"></span>
    <span class="entity-label">${esc(label)}</span>
    <span class="entity-id">${id ? esc(id.slice(0, 8)) : ''}</span>
  `;

  const detail = document.createElement('div');
  detail.className = 'entity-detail';
  detail.innerHTML = buildAdminDetailHTML(entityKey, id, item);

  const toggle = () => {
    state.expandedId = (id === state.expandedId) ? null : id;
    render();
  };
  header.addEventListener('click', toggle);
  header.addEventListener('keydown', e => {
    if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); toggle(); }
  });

  li.append(header, detail);

  if (state.expandedId === id) {
    setTimeout(() => {
      $('.btn-save', li)?.addEventListener('click', () => saveAdminRow(entityKey, id, li));
      $('.btn-delete', li)?.addEventListener('click', () => confirmDeleteAdmin(entityKey, id));
    }, 0);
  }
  return li;
}

function buildAdminDetailHTML(entityKey, id, payload) {
  const cfg = ENTITIES[entityKey];
  let html = '';

  // Readonly FK fields rendered first so the row context is obvious.
  if (cfg.readonlyInDetail) {
    for (const fkName of cfg.readonlyInDetail) {
      const val = payload[fkName] ?? '';
      const resolved = resolveReadonlyFK(fkName, val);
      html += `
        <div class="field-row">
          <label>${esc(fkName)}</label>
          <span class="readonly-val">${resolved}</span>
        </div>`;
    }
  }

  for (const f of cfg.detailFields) {
    const val = payload[f.name] ?? '';
    if (f.type === 'textarea') {
      html += `
        <div class="field-row">
          <label for="d-${esc(id)}-${esc(f.name)}">${esc(f.label)}</label>
          <textarea id="d-${esc(id)}-${esc(f.name)}" name="${esc(f.name)}" rows="2">${esc(val)}</textarea>
        </div>`;
    } else if (f.type === 'select') {
      const opts = (f.options || []).map(o =>
        `<option value="${esc(o)}"${o === val ? ' selected' : ''}>${esc(o) || '—'}</option>`
      ).join('');
      html += `
        <div class="field-row">
          <label for="d-${esc(id)}-${esc(f.name)}">${esc(f.label)}</label>
          <select id="d-${esc(id)}-${esc(f.name)}" name="${esc(f.name)}">${opts}</select>
        </div>`;
    } else {
      html += `
        <div class="field-row">
          <label for="d-${esc(id)}-${esc(f.name)}">${esc(f.label)}</label>
          <input id="d-${esc(id)}-${esc(f.name)}" name="${esc(f.name)}" type="text" value="${esc(val)}" />
        </div>`;
    }
  }

  html += `
    <div class="detail-actions">
      <button class="btn-save primary">Save</button>
      <button class="btn-delete danger">Delete</button>
    </div>`;
  return html;
}

function resolveReadonlyFK(fkName, val) {
  if (!val) return '—';
  if (fkName === 'deployable_id') {
    const dep = deployableById(val);
    return dep ? `<a href="#deployable/${esc(dep.id)}">${esc(dep.name || dep.id)}</a>` : esc(val);
  }
  if (fkName === 'service_id') {
    const svc = serviceById(val);
    return svc ? esc(svc.name || svc.id) : esc(val);
  }
  if (fkName === 'contract_id') {
    const c = state.data.contracts.find(x => x.id === val);
    if (!c) return esc(val);
    const svcName = serviceById(c.service_id)?.name || '?';
    return esc(`${c.version || c.id.slice(0, 8)} (${svcName})`);
  }
  return esc(val);
}

async function saveAdminRow(entityKey, id, li) {
  const cfg = ENTITIES[entityKey];
  const fields = {};
  $$('[name]', li).forEach(/** @param {Element} e */ e => {
    const n = /** @type {HTMLInputElement} */ (e);
    fields[n.name] = (n.value || '').trim();
  });

  // Preserve FKs and primary fields that aren't in the editable form.
  const original = state.data[entityKey].find(x => x.id === id);
  if (original) {
    if (original[cfg.primaryField] !== undefined) fields[cfg.primaryField] = original[cfg.primaryField];
    for (const fkName of (cfg.readonlyInDetail || [])) {
      if (original[fkName] !== undefined) fields[fkName] = original[fkName];
    }
  }
  try {
    await updateRecord(entityKey, id, fields);
    await loadEntity(entityKey);
    setStatus('Saved');
    render();
  } catch (e) { setError(e); }
}

async function confirmDeleteAdmin(entityKey, id) {
  const cfg = ENTITIES[entityKey];
  if (!confirm(`Delete this ${cfg.label}?`)) return;
  try {
    await deleteRecord(entityKey, id);
    if (state.expandedId === id) state.expandedId = null;
    await loadEntity(entityKey);
    setStatus('Deleted');
    render();
  } catch (e) { setError(e); }
}

async function createNewForAdmin(entityKey) {
  const cfg = ENTITIES[entityKey];
  // For SLAs, the contracts ref needs richer labels; substitute the lookup.
  const refLookup = (refKey) => refKey === 'contracts'
    ? contractsForLookup()
    : (state.data[refKey] || []);
  const payload = await openModal({
    title: `New ${cfg.label}`,
    fields: cfg.newFields,
    lookupRef: refLookup,
    submit: 'Create',
  });
  if (!payload) return;
  try {
    const created = await createRecord(entityKey, payload);
    await loadEntity(entityKey);
    setStatus(`Created ${cfg.label}`);
    if (created?.id) state.expandedId = created.id;
    render();
  } catch (e) { setError(e); }
}

// ── Full graph screen (preserved from prior implementation) ──────────────────

function renderGraphScreen(root) {
  updateFooterMeta(
    `${state.data.deployables.length} deployables · ${state.data.dependencies.length} dependencies`
  );

  const shell = document.createElement('div');
  shell.className = 'graph-shell';
  shell.innerHTML = `
    <div class="graph-toolbar" role="toolbar" aria-label="graph filters">
      <span class="filter-group" role="group" aria-label="filter by criticality">
        <span>criticality:</span>
        <label><input type="checkbox" data-filter-crit="high" checked> high</label>
        <label><input type="checkbox" data-filter-crit="medium" checked> medium</label>
        <label><input type="checkbox" data-filter-crit="low" checked> low</label>
      </span>
      <span class="filter-group">
        <label for="filter-team">team:</label>
        <select id="filter-team" aria-label="filter by team">
          <option value="">all teams</option>
        </select>
      </span>
      <button id="reset-graph" type="button">reset view</button>
      <button id="toggle-graph-view" type="button" aria-pressed="false">view as table</button>
    </div>
    <div class="graph-canvas-wrap">
      <div id="cy" role="img" aria-label="dependency graph canvas"></div>
      <div id="graph-table-wrap" hidden>
        <table id="graph-table" aria-label="deployables with dependency counts">
          <thead>
            <tr>
              <th data-sort="name" scope="col">deployable</th>
              <th data-sort="status" scope="col">status</th>
              <th data-sort="team" scope="col">team</th>
              <th data-sort="outgoing" scope="col" class="numeric">depends on</th>
              <th data-sort="incoming" scope="col" class="numeric">depended on by</th>
            </tr>
          </thead>
          <tbody></tbody>
        </table>
      </div>
      <aside id="graph-detail" class="graph-detail" hidden aria-live="polite"></aside>
    </div>
  `;
  root.appendChild(shell);
  // The cytoscape container only exists after we've appended the shell.
  requestAnimationFrame(() => renderFullGraph());
}

function composeGraphEdges(dependencies, exposes) {
  const exposesByService = new Map();
  for (const ex of exposes) {
    if (!ex || !ex.service_id) continue;
    if (!exposesByService.has(ex.service_id)) exposesByService.set(ex.service_id, []);
    exposesByService.get(ex.service_id).push(ex.deployable_id);
  }
  const edges = [];
  for (const dep of dependencies) {
    if (!dep?.service_id || !dep?.deployable_id) continue;
    const producers = exposesByService.get(dep.service_id) || [];
    for (const producerId of producers) {
      if (producerId === dep.deployable_id) continue;
      edges.push({
        data: {
          id: `${dep.id}-${producerId}`,
          source: dep.deployable_id,
          target: producerId,
          criticality: dep.criticality || 'medium',
        },
      });
    }
  }
  return edges;
}

function composeGraphNodes(deployables) {
  const known = new Set(['operational', 'degraded', 'down', 'unknown']);
  return deployables.map(d => {
    const raw = d.deployment_status || 'unknown';
    return {
      data: {
        id: d.id,
        label: d.name || d.id,
        status: known.has(raw) ? raw : 'unknown',
        team: d.team?.name || '',
        team_id: d.team_id || '',
      },
    };
  });
}

// Light-theme version of the graph style (the editorial-paper aesthetic).
const GRAPH_STYLE_LIGHT = [
  {
    selector: 'node',
    style: {
      'label': 'data(label)',
      'background-color': '#9ca3af',
      'text-valign': 'center',
      'text-halign': 'right',
      'text-margin-x': 6,
      'font-size': 11,
      'font-family': 'ui-sans-serif, system-ui, "SF Pro Text", Inter, sans-serif',
      'width': 18,
      'height': 18,
      'color': '#111827',
      'border-width': 1,
      'border-color': '#ffffff',
    },
  },
  { selector: 'node[status = "operational"]', style: { 'background-color': '#16a34a' } },
  { selector: 'node[status = "degraded"]',    style: { 'background-color': '#f59e0b' } },
  { selector: 'node[status = "down"]',        style: { 'background-color': '#dc2626' } },
  { selector: 'node[status = "unknown"]',     style: { 'background-color': '#9ca3af' } },
  { selector: 'node.focal', style: { 'border-width': 3, 'border-color': '#111827', 'width': 22, 'height': 22 } },
  {
    selector: 'edge',
    style: {
      'width': 1.5,
      'line-color': '#9ca3af',
      'curve-style': 'bezier',
      'target-arrow-shape': 'triangle',
      'target-arrow-color': '#9ca3af',
      'opacity': 0.85,
    },
  },
  { selector: 'edge[criticality = "low"]',    style: { 'width': 1,   'line-color': '#cbd5e1', 'target-arrow-color': '#cbd5e1' } },
  { selector: 'edge[criticality = "medium"]', style: { 'width': 2,   'line-color': '#9ca3af', 'target-arrow-color': '#9ca3af' } },
  { selector: 'edge[criticality = "high"]',   style: { 'width': 3,   'line-color': '#dc2626', 'target-arrow-color': '#dc2626' } },
  { selector: '.faded', style: { 'opacity': 0.12, 'text-opacity': 0.12 } },
  { selector: ':selected', style: { 'border-width': 3, 'border-color': '#111827' } },
];

function renderFullGraph() {
  const cyEl = document.getElementById('cy');
  if (!cyEl) return;
  if (typeof cytoscape !== 'function') {
    cyEl.textContent = 'cytoscape failed to load';
    return;
  }
  if (state.graph.cy) {
    try { state.graph.cy.destroy(); } catch { /* no-op */ }
    state.graph.cy = null;
  }
  cyEl.textContent = '';

  const deployables = state.data.deployables || [];
  const dependencies = state.data.dependencies || [];
  const exposes = state.data.exposes || [];

  const nodes = composeGraphNodes(deployables);
  const edges = composeGraphEdges(dependencies, exposes);
  const nodeIds = new Set(nodes.map(n => n.data.id));
  const safeEdges = edges.filter(e => nodeIds.has(e.data.source) && nodeIds.has(e.data.target));

  const cy = cytoscape({
    container: cyEl,
    elements: [...nodes, ...safeEdges],
    style: GRAPH_STYLE_LIGHT,
    layout: {
      name: 'cose', animate: false, idealEdgeLength: 100, nodeOverlap: 20,
      refresh: 20, fit: true, padding: 40, randomize: false, componentSpacing: 80,
      nodeRepulsion: 400000, edgeElasticity: 100, nestingFactor: 5, gravity: 80, numIter: 1000,
    },
    wheelSensitivity: 0.2, minZoom: 0.2, maxZoom: 3,
  });
  state.graph.cy = cy;

  cy.on('tap', 'node', evt => {
    const node = evt.target;
    cy.elements().addClass('faded');
    node.removeClass('faded');
    node.neighborhood().removeClass('faded');
    showGraphDetail(node.data());
  });
  cy.on('dbltap', 'node', evt => {
    setScreen(`deployable/${evt.target.id()}`);
  });
  cy.on('tap', evt => {
    if (evt.target === cy) {
      cy.elements().removeClass('faded');
      const d = document.getElementById('graph-detail');
      if (d) d.hidden = true;
    }
  });

  populateTeamFilter(deployables);
  wireGraphToolbar();
  applyGraphViewMode(state.graph.tableMode);
  renderGraphTable(deployables, safeEdges);
}

function showGraphDetail(nodeData) {
  const panel = document.getElementById('graph-detail');
  if (!panel) return;
  const deployableId = nodeData.id;
  const exposed = servicesExposedBy(deployableId);
  const deps = dependenciesOf(deployableId);
  const dependents = dependentsOf(deployableId);

  const exposedHtml = exposed.length
    ? `<ul>${exposed.map(s => `<li>${esc(s.service?.name || s.service_id)}</li>`).join('')}</ul>`
    : '<p class="empty-list">no exposed services</p>';
  const depsHtml = deps.length
    ? `<ul>${deps.map(d => `<li>${esc(serviceById(d.service_id)?.name || d.service_id)}</li>`).join('')}</ul>`
    : '<p class="empty-list">no dependencies</p>';
  const dependentsHtml = dependents.length
    ? `<ul>${dependents.map(({depender, service}) => `<li>${esc(depender?.name || '?')} via ${esc(service?.name || '?')}</li>`).join('')}</ul>`
    : '<p class="empty-list">no dependents</p>';

  panel.innerHTML = `
    <h2>${esc(nodeData.label || deployableId)}</h2>
    <dl>
      <dt>status</dt><dd><span class="status-dot ${esc(nodeData.status || 'unknown')}"></span> ${esc(nodeData.status || 'unknown')}</dd>
      <dt>team</dt><dd>${esc(nodeData.team || '—')}</dd>
      <dt>id</dt><dd>${esc(deployableId.slice(0, 8))}</dd>
    </dl>
    <h3>exposes</h3>${exposedHtml}
    <h3>depends on</h3>${depsHtml}
    <h3>depended on by</h3>${dependentsHtml}
    <p style="margin-top:14px"><a href="#deployable/${esc(deployableId)}">Open detail →</a></p>
  `;
  panel.hidden = false;
}

function populateTeamFilter(deployables) {
  const sel = document.getElementById('filter-team');
  if (!sel) return;
  const teams = new Set();
  for (const d of deployables) {
    if (d.team?.name) teams.add(d.team.name);
  }
  const sorted = [...teams].sort();
  const prev = sel.value;
  sel.innerHTML = '<option value="">all teams</option>' +
    sorted.map(t => `<option value="${esc(t)}">${esc(t)}</option>`).join('');
  if (prev && sorted.includes(prev)) sel.value = prev;
}

function wireGraphToolbar() {
  const replaceWithClone = (id) => {
    const el = document.getElementById(id);
    if (!el) return null;
    const fresh = el.cloneNode(true);
    el.parentNode.replaceChild(fresh, el);
    return fresh;
  };
  document.querySelectorAll('[data-filter-crit]').forEach(cb => {
    const fresh = cb.cloneNode(true);
    cb.parentNode.replaceChild(fresh, cb);
  });
  document.querySelectorAll('[data-filter-crit]').forEach(cb => {
    cb.addEventListener('change', applyGraphFilters);
  });
  const teamSel = replaceWithClone('filter-team');
  if (teamSel) teamSel.addEventListener('change', applyGraphFilters);
  const reset = replaceWithClone('reset-graph');
  if (reset) reset.addEventListener('click', () => {
    document.querySelectorAll('[data-filter-crit]').forEach(c => { c.checked = true; });
    const ts = document.getElementById('filter-team');
    if (ts) ts.value = '';
    if (state.graph.cy) {
      state.graph.cy.elements().removeClass('faded');
      state.graph.cy.elements().style('display', 'element');
      state.graph.cy.fit(undefined, 40);
    }
    const detail = document.getElementById('graph-detail');
    if (detail) detail.hidden = true;
  });
  const toggle = replaceWithClone('toggle-graph-view');
  if (toggle) toggle.addEventListener('click', () => {
    state.graph.tableMode = !state.graph.tableMode;
    applyGraphViewMode(state.graph.tableMode);
  });
}

function applyGraphFilters() {
  const cy = state.graph.cy;
  if (!cy) return;
  const enabledCrit = new Set();
  document.querySelectorAll('[data-filter-crit]').forEach(c => {
    if (c.checked) enabledCrit.add(c.dataset.filterCrit);
  });
  const teamSel = document.getElementById('filter-team');
  const team = teamSel ? teamSel.value : '';
  cy.nodes().forEach(n => {
    n.style('display', !team || n.data('team') === team ? 'element' : 'none');
  });
  cy.edges().forEach(e => {
    const critOk = enabledCrit.has(e.data('criticality') || 'medium');
    const endpointsVisible = e.source().style('display') !== 'none'
                          && e.target().style('display') !== 'none';
    e.style('display', critOk && endpointsVisible ? 'element' : 'none');
  });
}

function applyGraphViewMode(tableMode) {
  const cyEl = document.getElementById('cy');
  const tableWrap = document.getElementById('graph-table-wrap');
  const toggle = document.getElementById('toggle-graph-view');
  if (cyEl) cyEl.hidden = tableMode;
  if (tableWrap) tableWrap.hidden = !tableMode;
  if (toggle) {
    toggle.textContent = tableMode ? 'view as graph' : 'view as table';
    toggle.setAttribute('aria-pressed', String(tableMode));
  }
  if (tableMode) {
    const detail = document.getElementById('graph-detail');
    if (detail) detail.hidden = true;
  }
  if (!tableMode && state.graph.cy) {
    requestAnimationFrame(() => {
      state.graph.cy.resize();
      state.graph.cy.fit(undefined, 40);
    });
  }
}

function renderGraphTable(deployables, edges) {
  const tbody = document.querySelector('#graph-table tbody');
  if (!tbody) return;
  const outgoing = new Map();
  const incoming = new Map();
  for (const e of edges) {
    outgoing.set(e.data.source, (outgoing.get(e.data.source) || 0) + 1);
    incoming.set(e.data.target, (incoming.get(e.data.target) || 0) + 1);
  }
  const rows = deployables.map(d => ({
    id: d.id,
    name: d.name || d.id,
    status: d.deployment_status || 'unknown',
    team: d.team?.name || '',
    outgoing: outgoing.get(d.id) || 0,
    incoming: incoming.get(d.id) || 0,
  }));
  rows.sort((a, b) => a.name.localeCompare(b.name));
  const draw = (data) => {
    tbody.innerHTML = data.map(r => `
      <tr data-id="${esc(r.id)}">
        <td><a href="#deployable/${esc(r.id)}">${esc(r.name)}</a></td>
        <td><span class="status-dot ${esc(r.status)}"></span> ${esc(r.status)}</td>
        <td>${r.team ? esc(r.team) : '<span style="color: var(--text-soft)">—</span>'}</td>
        <td class="numeric">${r.outgoing}</td>
        <td class="numeric">${r.incoming}</td>
      </tr>`).join('');
  };
  let currentSort = { key: 'name', asc: true };
  document.querySelectorAll('#graph-table th[data-sort]').forEach(th => {
    const fresh = th.cloneNode(true);
    th.parentNode.replaceChild(fresh, th);
  });
  document.querySelectorAll('#graph-table th[data-sort]').forEach(th => {
    th.addEventListener('click', () => {
      const key = th.dataset.sort;
      const asc = currentSort.key === key ? !currentSort.asc : true;
      currentSort = { key, asc };
      const sorted = [...rows].sort((a, b) => {
        const av = a[key], bv = b[key];
        if (typeof av === 'number' && typeof bv === 'number') return asc ? av - bv : bv - av;
        return asc ? String(av).localeCompare(String(bv)) : String(bv).localeCompare(String(av));
      });
      draw(sorted);
    });
  });
  draw(rows);
}

// ── Hash routing ──────────────────────────────────────────────────────────────

const KNOWN_SCREEN = (key) =>
  key === 'catalog' || key === 'graph' || key === 'governance' ||
  key.startsWith('deployable/') ||
  (key.startsWith('admin/') && ENTITIES[key.slice('admin/'.length)]);

function initHashRouting() {
  window.addEventListener('hashchange', () => {
    const key = location.hash.slice(1);
    if (key && KNOWN_SCREEN(key) && key !== state.screen) setScreen(key);
  });
  const initial = location.hash.slice(1);
  if (initial && KNOWN_SCREEN(initial)) state.screen = initial;
  else location.replace('#' + state.screen);
}

// ── Wire-up ──────────────────────────────────────────────────────────────────

function openMenu()  { $('#settings-menu').classList.add('open'); }
function closeMenu() { $('#settings-menu').classList.remove('open'); }
function toggleMenu(){ $('#settings-menu').classList.toggle('open'); }

function newActionForCurrentScreen() {
  if (state.screen.startsWith('admin/')) {
    const key = state.screen.slice('admin/'.length);
    return () => createNewForAdmin(key);
  }
  // catalog, graph, governance, deployable/* — primary creation flow is
  // always a new deployable.
  return createNewDeployable;
}

function bindUI() {
  $$('#primary-nav .tab').forEach(t => {
    t.addEventListener('click', () => setScreen(t.dataset.screen));
  });

  $('#btn-new').addEventListener('click', () => newActionForCurrentScreen()());

  $('#btn-settings').addEventListener('click', (e) => {
    e.stopPropagation();
    toggleMenu();
  });
  $$('#settings-menu .menu-item').forEach(item => {
    item.addEventListener('click', () => setScreen(item.dataset.screen));
  });
  document.addEventListener('click', (e) => {
    if (!e.target.closest('.menu-host')) closeMenu();
  });

  $('#search').addEventListener('input', (e) => {
    state.search = e.target.value;
    render();
  });

  $('#status-filter').addEventListener('change', (e) => {
    state.statusFilter = e.target.value;
    if (state.screen === 'catalog') render();
  });

  document.addEventListener('keydown', (e) => {
    const tag = document.activeElement?.tagName?.toLowerCase();
    const inField = ['input', 'textarea', 'select'].includes(tag);
    const modalOpen = !!document.querySelector('.modal-backdrop');

    if (e.key === 'Escape') {
      if (modalOpen) return; // modal handles its own Esc
      if (state.expandedId) { state.expandedId = null; render(); return; }
      if (state.screen.startsWith('deployable/') || state.screen.startsWith('admin/')) {
        setScreen('catalog');
        return;
      }
      const search = $('#search');
      if (document.activeElement === search) {
        search.value = '';
        state.search = '';
        search.blur();
        render();
      }
      closeMenu();
      return;
    }
    if (e.key === '/' && !inField) {
      e.preventDefault();
      $('#search').focus();
      $('#search').select();
      return;
    }
    if (e.key === 'n' && !inField && !modalOpen) {
      e.preventDefault();
      newActionForCurrentScreen()();
      return;
    }
  });
}

async function init() {
  bindUI();
  initHashRouting();
  setStatus('Loading…', 'info', { sticky: true });
  await loadManifoldConfig();
  try {
    await loadAll();
    setStatus('');
  } catch (err) {
    setError(err);
  }
  setScreen(state.screen);
  $('#search')?.focus();
}

init();
