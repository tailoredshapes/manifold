// groundwork — multi-entity service catalog UI
// Vanilla JS ES module, no build step, no framework.

// ── Entity config ─────────────────────────────────────────────────────────────

const ENTITIES = {
  deployables: {
    api: '/deployable/api',
    graph: {
      path: '/deployable/graph',
      list: '{ getAll { id name description repo_url team_id deployment_status team { id name } } }',
    },
    label: 'deployable',
    newFields: [
      { name: 'name', label: 'name', type: 'text', required: true },
      { name: 'description', label: 'description', type: 'text', required: false },
      { name: 'repo_url', label: 'repo_url', type: 'text', required: false },
      { name: 'team_id', label: 'team_id (Union)', type: 'text', required: false },
    ],
    detailFields: [
      { name: 'description', label: 'description', type: 'textarea' },
      { name: 'repo_url', label: 'repo_url', type: 'text' },
      { name: 'team_id', label: 'team_id', type: 'text' },
    ],
    primaryField: 'name',
    getRowLabel: (payload) => payload.name || 'unnamed',
    getRowBadge: (payload) => payload.team?.name || null,
    getRowBadgeHtml: (payload) => {
      const label = payload.team?.name;
      if (!label) return null;
      // Cross-app deeplink: team detail lives in Union under the Teams screen.
      return { html: crossLink('union', 'teams', payload.team_id, label) };
    },
  },

  services: {
    api: '/service/api',
    graph: {
      path: '/service/graph',
      list: '{ getAll { id name type description endpoint } }',
    },
    label: 'service',
    newFields: [
      { name: 'name', label: 'name', type: 'text', required: true },
      { name: 'type', label: 'type', type: 'select', required: false,
        options: ['', 'database', 'api', 'queue', 'cache', 'message-broker', 'storage', 'auth', 'other'] },
      { name: 'description', label: 'description', type: 'text', required: false },
      { name: 'endpoint', label: 'endpoint', type: 'text', required: false },
    ],
    detailFields: [
      { name: 'type', label: 'type', type: 'select',
        options: ['', 'database', 'api', 'queue', 'cache', 'message-broker', 'storage', 'auth', 'other'] },
      { name: 'description', label: 'description', type: 'textarea' },
      { name: 'endpoint', label: 'endpoint', type: 'text' },
    ],
    primaryField: 'name',
    getRowLabel: (payload) => payload.name || 'unnamed',
    getRowBadge: null,
  },

  exposes: {
    api: '/exposes/api',
    graph: {
      path: '/exposes/graph',
      list: '{ getAll { id deployable_id service_id port protocol } }',
    },
    label: 'exposes',
    newFields: [
      { name: 'deployable_id', label: 'deployable', type: 'dynamic-select', required: true,
        optionsFrom: (data) => data.deployables.map(d => ({ value: d.id, label: d.name || d.id })) },
      { name: 'service_id', label: 'service', type: 'dynamic-select', required: true,
        optionsFrom: (data) => data.services.map(s => ({ value: s.id, label: s.name || s.id })) },
      { name: 'port', label: 'port', type: 'text', required: false },
      { name: 'protocol', label: 'protocol', type: 'select', required: false,
        options: ['', 'http', 'https', 'grpc', 'tcp', 'udp', 'other'] },
    ],
    detailFields: [
      { name: 'port', label: 'port', type: 'text' },
      { name: 'protocol', label: 'protocol', type: 'select',
        options: ['', 'http', 'https', 'grpc', 'tcp', 'udp', 'other'] },
    ],
    primaryField: 'deployable_id',
    getRowLabel: (payload, data) => {
      const dep = data.deployables.find(d => d.id === payload.deployable_id)?.name
        || payload.deployable_id || '?';
      const svc = data.services.find(s => s.id === payload.service_id)?.name
        || payload.service_id || '?';
      return `${dep} ⇒ ${svc}`;
    },
    getRowBadge: (payload) => payload.protocol || null,
    readonlyInDetail: [
      { name: 'deployable_id', label: 'deployable',
        resolve: (payload, data) => data.deployables.find(d => d.id === payload.deployable_id)?.name || payload.deployable_id || '—',
        htmlResolve: (payload, data) => {
          const name = data.deployables.find(d => d.id === payload.deployable_id)?.name || payload.deployable_id;
          if (!name) return esc('—');
          return intraLink('deployables', payload.deployable_id, name);
        } },
      { name: 'service_id', label: 'service',
        resolve: (payload, data) => data.services.find(s => s.id === payload.service_id)?.name || payload.service_id || '—',
        htmlResolve: (payload, data) => {
          const name = data.services.find(s => s.id === payload.service_id)?.name || payload.service_id;
          if (!name) return esc('—');
          return intraLink('services', payload.service_id, name);
        } },
    ],
  },

  dependencies: {
    api: '/dependency/api',
    graph: {
      path: '/dependency/graph',
      list: '{ getAll { id deployable_id service_id protocol auth_method criticality } }',
    },
    label: 'dependency',
    newFields: [
      { name: 'deployable_id', label: 'deployable', type: 'dynamic-select', required: true,
        optionsFrom: (data) => data.deployables.map(a => ({ value: a.id, label: a.name || a.id })) },
      { name: 'service_id', label: 'service', type: 'dynamic-select', required: true,
        optionsFrom: (data) => data.services.map(s => ({ value: s.id, label: s.name || s.id })) },
      { name: 'criticality', label: 'criticality', type: 'select', required: false,
        options: ['', 'high', 'medium', 'low'] },
      { name: 'protocol', label: 'protocol', type: 'text', required: false },
      { name: 'auth_method', label: 'auth_method', type: 'text', required: false },
    ],
    detailFields: [
      { name: 'criticality', label: 'criticality', type: 'select',
        options: ['', 'high', 'medium', 'low'] },
      { name: 'protocol', label: 'protocol', type: 'text' },
      { name: 'auth_method', label: 'auth_method', type: 'text' },
    ],
    primaryField: 'deployable_id',
    getRowLabel: (payload, data) => {
      const depName = data.deployables.find(a => a.id === payload.deployable_id)?.name
        || payload.deployable_id || '?';
      const svcName = data.services.find(s => s.id === payload.service_id)?.name
        || payload.service_id || '?';
      return `${depName} → ${svcName}`;
    },
    getRowBadge: (payload) => payload.criticality || null,
    readonlyInDetail: [
      { name: 'deployable_id', label: 'deployable',
        resolve: (payload, data) => data.deployables.find(a => a.id === payload.deployable_id)?.name || payload.deployable_id || '—',
        htmlResolve: (payload, data) => {
          const name = data.deployables.find(a => a.id === payload.deployable_id)?.name || payload.deployable_id;
          if (!name) return esc('—');
          return intraLink('deployables', payload.deployable_id, name);
        } },
      { name: 'service_id', label: 'service',
        resolve: (payload, data) => data.services.find(s => s.id === payload.service_id)?.name || payload.service_id || '—',
        htmlResolve: (payload, data) => {
          const name = data.services.find(s => s.id === payload.service_id)?.name || payload.service_id;
          if (!name) return esc('—');
          return intraLink('services', payload.service_id, name);
        } },
    ],
  },

  contracts: {
    api: '/contract/api',
    graph: {
      path: '/contract/graph',
      list: '{ getAll { id service_id spec_url version format } }',
    },
    label: 'contract',
    newFields: [
      { name: 'service_id', label: 'service', type: 'dynamic-select', required: true,
        optionsFrom: (data) => data.services.map(s => ({ value: s.id, label: s.name || s.id })) },
      { name: 'spec_url', label: 'spec_url', type: 'text', required: false },
      { name: 'version', label: 'version', type: 'text', required: false },
      { name: 'format', label: 'format', type: 'select', required: false,
        options: ['', 'openapi', 'grpc', 'graphql', 'asyncapi', 'other'] },
    ],
    detailFields: [
      { name: 'spec_url', label: 'spec_url', type: 'text' },
      { name: 'version', label: 'version', type: 'text' },
      { name: 'format', label: 'format', type: 'select',
        options: ['', 'openapi', 'grpc', 'graphql', 'asyncapi', 'other'] },
    ],
    primaryField: 'service_id',
    getRowLabel: (payload, data) => {
      const svcName = data.services.find(s => s.id === payload.service_id)?.name
        || payload.service_id || '?';
      const ver = payload.version ? `v${payload.version}` : '';
      const fmt = payload.format || '';
      return [svcName, ver, fmt].filter(Boolean).join(' · ');
    },
    getRowBadge: (payload) => payload.format || null,
    readonlyInDetail: [
      { name: 'service_id', label: 'service',
        resolve: (payload, data) => data.services.find(s => s.id === payload.service_id)?.name || payload.service_id || '—',
        htmlResolve: (payload, data) => {
          const name = data.services.find(s => s.id === payload.service_id)?.name || payload.service_id;
          if (!name) return esc('—');
          return intraLink('services', payload.service_id, name);
        } },
    ],
  },

  slas: {
    api: '/sla/api',
    graph: {
      path: '/sla/graph',
      list: '{ getAll { id contract_id metric target window } }',
    },
    label: 'sla',
    newFields: [
      { name: 'contract_id', label: 'contract', type: 'dynamic-select', required: true,
        optionsFrom: (data) => data.contracts.map(c => {
          const svcName = data.services.find(s => s.id === c.service_id)?.name || '?';
          const ver = c.version ? `v${c.version}` : c.id.slice(0, 8);
          return { value: c.id, label: `${ver} (${svcName})` };
        }) },
      { name: 'metric', label: 'metric', type: 'text', required: false },
      { name: 'target', label: 'target', type: 'text', required: false },
      { name: 'window', label: 'window', type: 'text', required: false },
    ],
    detailFields: [
      { name: 'metric', label: 'metric', type: 'text' },
      { name: 'target', label: 'target', type: 'text' },
      { name: 'window', label: 'window', type: 'text' },
    ],
    primaryField: 'contract_id',
    getRowLabel: (payload, data) => {
      const contract = data.contracts.find(c => c.id === payload.contract_id);
      const svcName = contract
        ? (data.services.find(s => s.id === contract.service_id)?.name || '?')
        : '?';
      return `${payload.metric || '?'}: ${payload.target || '?'} [${svcName}]`;
    },
    getRowBadge: null,
    readonlyInDetail: [
      { name: 'contract_id', label: 'contract',
        resolve: (payload, data) => {
          const c = data.contracts.find(x => x.id === payload.contract_id);
          if (!c) return payload.contract_id || '—';
          const svcName = data.services.find(s => s.id === c.service_id)?.name || '?';
          return `${c.version || c.id.slice(0, 8)} (${svcName})`;
        },
        htmlResolve: (payload, data) => {
          const c = data.contracts.find(x => x.id === payload.contract_id);
          if (!c) return esc(payload.contract_id || '—');
          const svcName = data.services.find(s => s.id === c.service_id)?.name || '?';
          const label = `${c.version || c.id.slice(0, 8)} (${svcName})`;
          return intraLink('contracts', payload.contract_id, label);
        } },
    ],
  },
};

// ── State ─────────────────────────────────────────────────────────────────────

const state = {
  activeEntity: 'deployables',
  data: { deployables: [], exposes: [], services: [], dependencies: [], contracts: [], slas: [] },
  expandedId: null,
  filter: '',
  // Per-entity column filters keyed by entity → field → selected value.
  // Currently only deployables.deployment_status is wired up; the shape
  // generalises if more column filters are added later.
  columnFilter: { deployables: { deployment_status: '' } },
  newFormOpen: false,
  config: {},  // populated from /config.json: cross-app public URLs
  graph: { cy: null, tableMode: false },  // cytoscape instance + table-view toggle
};

// ── Cross-app linking ─────────────────────────────────────────────────────────

async function loadConfig() {
  try {
    const res = await fetch('/config.json');
    if (res.ok) state.config = await res.json();
  } catch {
    state.config = {};
  }
}

// Build a cross-app anchor pointing at <base>#<screen>[/<id>], or fall back to
// plain escaped text when the target app's public URL is unknown. Receiving
// end may not yet honour the id segment — that's deferred.
function crossLink(appKey, screen, id, label) {
  const base = state.config?.[`${appKey}_public_url`];
  if (!base) return esc(label);
  const hash = id ? `#${screen}/${encodeURIComponent(id)}` : `#${screen}`;
  return `<a href="${esc(base.replace(/\/$/, ''))}${hash}">${esc(label)}</a>`;
}

// Build an intra-app anchor pointing at #<screen>[/<id>].
function intraLink(screen, id, label) {
  const hash = id ? `#${screen}/${encodeURIComponent(id)}` : `#${screen}`;
  return `<a href="${hash}">${esc(label)}</a>`;
}

// ── API helpers ───────────────────────────────────────────────────────────────

async function apiFetch(url, opts) {
  const res = await fetch(url, opts);
  if (!res.ok) {
    const body = await res.text().catch(() => '');
    throw new Error(`${opts?.method || 'GET'} ${url} → ${res.status}${body ? ': ' + body : ''}`);
  }
  if (res.status === 204) return null;
  return res.json();
}

async function gqlQuery(path, query, variables = {}) {
  const res = await fetch(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ query, variables }),
  });
  if (!res.ok) {
    const body = await res.text().catch(() => '');
    throw new Error(`graph ${path} ${res.status}${body ? ': ' + body : ''}`);
  }
  const body = await res.json();
  if (body.errors && body.errors.length) {
    throw new Error(body.errors.map(e => e.message).join('; '));
  }
  return body.data;
}

async function loadEntity(entityKey) {
  const cfg = ENTITIES[entityKey];
  const data = await gqlQuery(cfg.graph.path, cfg.graph.list);
  state.data[entityKey] = Array.isArray(data.getAll) ? data.getAll : [];
  updateBadge(entityKey);
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

// ── Rendering ─────────────────────────────────────────────────────────────────

function esc(s) {
  return String(s ?? '').replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

function updateBadge(entityKey) {
  const el = document.getElementById(`badge-${entityKey}`);
  if (el) el.textContent = state.data[entityKey]?.length ?? 0;
}

function renderList() {
  const entityKey = state.activeEntity;
  const cfg = ENTITIES[entityKey];
  const items = state.data[entityKey] ?? [];
  const needle = state.filter.trim().toLowerCase();
  const colFilters = state.columnFilter[entityKey] || {};

  let visible = items;

  // Column filters (currently only deployment_status on deployables).
  // Each filter narrows independently; composes with search below.
  for (const [field, val] of Object.entries(colFilters)) {
    if (!val) continue;
    visible = visible.filter(item => (item[field] ?? null) === val);
  }

  if (needle) {
    visible = visible.filter(item => {
      const label = cfg.getRowLabel(item, state.data).toLowerCase();
      return label.includes(needle) || (item.id || '').toLowerCase().includes(needle);
    });
  }

  const list = document.getElementById('entity-list');
  list.innerHTML = '';

  const hasActiveColFilter = Object.values(colFilters).some(Boolean);
  if (visible.length === 0) {
    const isFiltering = needle || hasActiveColFilter;
    list.innerHTML = `<li class="empty-state">${isFiltering ? 'no matches' : `no ${entityKey} yet — press n to add one`}</li>`;
  } else {
    for (const item of visible) {
      list.appendChild(buildRow(entityKey, item));
    }
  }

  updateStatusCount(items.length, visible.length);
}

function buildRow(entityKey, item) {
  const cfg = ENTITIES[entityKey];
  const id = item.id;
  const payload = item;
  const label = cfg.getRowLabel(payload, state.data);
  const badge = cfg.getRowBadge ? cfg.getRowBadge(payload) : null;
  const badgeHtml = cfg.getRowBadgeHtml ? cfg.getRowBadgeHtml(payload, state.data) : null;

  const li = document.createElement('li');
  li.className = 'entity-row' + (state.expandedId === id ? ' expanded' : '');
  li.dataset.id = id;

  const header = document.createElement('div');
  header.className = 'entity-row-header';
  header.setAttribute('tabindex', '0');
  header.setAttribute('role', 'button');
  header.setAttribute('aria-expanded', String(state.expandedId === id));

  const icon = document.createElement('span');
  icon.className = 'expand-icon';

  const labelEl = document.createElement('span');
  labelEl.className = 'entity-label';
  labelEl.textContent = label;

  header.append(icon, labelEl);

  // Deployment-status dot — currently only deployables carry this field.
  // Semantic CSS classes drive the colour; SR text comes from .sr-only span
  // plus a title attribute for sighted hover.
  if (entityKey === 'deployables') {
    const status = payload.deployment_status || 'unknown';
    const dot = document.createElement('span');
    dot.className = `status-dot ${status}`;
    dot.setAttribute('title', `deployment status: ${status}`);
    const sr = document.createElement('span');
    sr.className = 'sr-only';
    sr.textContent = `deployment status ${status}`;
    dot.appendChild(sr);
    header.appendChild(dot);
  }

  if (badgeHtml) {
    const badgeEl = document.createElement('span');
    badgeEl.className = 'badge';
    badgeEl.innerHTML = badgeHtml.html;
    // Anchor inside badge should not toggle the row.
    badgeEl.addEventListener('click', e => e.stopPropagation());
    header.appendChild(badgeEl);
  } else if (badge) {
    const badgeEl = document.createElement('span');
    badgeEl.className = `badge ${badge}`;
    badgeEl.textContent = badge;
    header.appendChild(badgeEl);
  }

  const idEl = document.createElement('span');
  idEl.className = 'entity-id';
  idEl.textContent = id ? id.slice(0, 8) : '';
  header.appendChild(idEl);

  const detail = document.createElement('div');
  detail.className = 'entity-detail';
  detail.innerHTML = buildDetailHTML(entityKey, id, payload);

  li.append(header, detail);

  const toggle = () => expandRow(id === state.expandedId ? null : id);
  header.addEventListener('click', toggle);
  header.addEventListener('keydown', e => {
    if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); toggle(); }
  });

  detail.querySelector('.btn-save')?.addEventListener('click', () => saveRow(entityKey, id, li));
  detail.querySelector('.btn-delete')?.addEventListener('click', () => confirmDelete(entityKey, id));

  return li;
}

function buildDetailHTML(entityKey, id, payload) {
  const cfg = ENTITIES[entityKey];
  let html = '';

  // Readonly FK fields shown at top
  if (cfg.readonlyInDetail) {
    for (const rf of cfg.readonlyInDetail) {
      // htmlResolve is the new opt-in path for cross/intra-app linking.
      // It MUST return safe pre-escaped HTML (callers use esc() / intraLink()).
      const valHtml = rf.htmlResolve
        ? rf.htmlResolve(payload, state.data)
        : esc(rf.resolve(payload, state.data));
      html += `
        <div class="field-row">
          <label>${esc(rf.label)}</label>
          <span class="readonly-val">${valHtml}</span>
        </div>`;
    }
  }

  // Editable fields
  for (const f of cfg.detailFields) {
    const val = payload[f.name] ?? '';
    if (f.type === 'textarea') {
      html += `
        <div class="field-row">
          <label for="d-${esc(id)}-${f.name}">${esc(f.label)}</label>
          <textarea id="d-${esc(id)}-${f.name}" name="${f.name}" rows="2">${esc(val)}</textarea>
        </div>`;
    } else if (f.type === 'select') {
      const opts = (f.options || []).map(o =>
        `<option value="${esc(o)}"${o === val ? ' selected' : ''}>${esc(o) || '—'}</option>`
      ).join('');
      html += `
        <div class="field-row">
          <label for="d-${esc(id)}-${f.name}">${esc(f.label)}</label>
          <select id="d-${esc(id)}-${f.name}" name="${f.name}">${opts}</select>
        </div>`;
    } else {
      html += `
        <div class="field-row">
          <label for="d-${esc(id)}-${f.name}">${esc(f.label)}</label>
          <input id="d-${esc(id)}-${f.name}" name="${f.name}" type="text" value="${esc(val)}" />
        </div>`;
    }
  }

  html += `
    <div class="detail-actions">
      <button class="btn-save primary">save</button>
      <button class="btn-delete danger">delete</button>
    </div>`;

  return html;
}

function expandRow(id) {
  state.expandedId = id;
  document.querySelectorAll('.entity-row').forEach(row => {
    const isTarget = row.dataset.id === id;
    row.classList.toggle('expanded', isTarget);
    const header = row.querySelector('.entity-row-header');
    if (header) header.setAttribute('aria-expanded', String(isTarget));
  });
}

async function saveRow(entityKey, id, li) {
  const cfg = ENTITIES[entityKey];
  const fields = {};

  li.querySelectorAll('[name]').forEach(el => {
    fields[el.name] = el.value.trim();
  });

  // Preserve primary field from original record (e.g. name, deployable_id)
  const original = state.data[entityKey].find(a => a.id === id);
  if (original) {
    const pf = cfg.primaryField;
    if (original[pf] !== undefined) {
      fields[pf] = original[pf];
    }
    // Preserve all readonly FK fields too
    if (cfg.readonlyInDetail) {
      for (const rf of cfg.readonlyInDetail) {
        if (original[rf.name] !== undefined) {
          fields[rf.name] = original[rf.name];
        }
      }
    }
  }

  try {
    await updateRecord(entityKey, id, fields);
    // REST writes return only local fields; re-read via /graph so federated
    // fields (e.g. Deployable.team) stay accurate in the rendered row.
    await loadEntity(entityKey);
    setError('');
    renderList();
  } catch (err) {
    setError(err.message);
  }
}

async function confirmDelete(entityKey, id) {
  const cfg = ENTITIES[entityKey];
  if (!confirm(`Delete this ${cfg.label}?`)) return;
  try {
    await deleteRecord(entityKey, id);
    if (state.expandedId === id) state.expandedId = null;
    // Re-read via /graph for consistency with create/update paths and to pick
    // up any concurrent changes.
    await loadEntity(entityKey);
    setError('');
    renderList();
  } catch (err) {
    setError(err.message);
  }
}

// ── Graph view (Cytoscape) ────────────────────────────────────────────────────

// Compose Deployable→Deployable edges from Dependency + Exposes records.
// A Dependency tells us "consumer Deployable depends on Service"; we need
// to resolve the producer side via Exposes (which Deployables expose that
// Service). Returns an array of cytoscape edge objects keyed unique on the
// triple (dependency_id, producer_id). Self-loops are skipped.
function composeGraphEdges(dependencies, exposes) {
  const exposesByService = new Map();
  for (const ex of exposes) {
    if (!ex || !ex.service_id) continue;
    if (!exposesByService.has(ex.service_id)) {
      exposesByService.set(ex.service_id, []);
    }
    exposesByService.get(ex.service_id).push(ex.deployable_id);
  }

  const edges = [];
  for (const dep of dependencies) {
    if (!dep || !dep.service_id || !dep.deployable_id) continue;
    const producers = exposesByService.get(dep.service_id) || [];
    for (const producerId of producers) {
      if (producerId === dep.deployable_id) continue; // skip self-loops
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

// Build cytoscape node objects from deployables. `status` is bucketed to one
// of the four known values so the data-driven CSS selectors below always
// match — otherwise an unexpected status leaves nodes unstyled.
function composeGraphNodes(deployables) {
  const known = new Set(['operational', 'degraded', 'down', 'unknown']);
  return deployables.map(d => {
    const raw = d.deployment_status || 'unknown';
    const status = known.has(raw) ? raw : 'unknown';
    return {
      data: {
        id: d.id,
        label: d.name || d.id,
        status,
        team: d.team?.name || '',
        team_id: d.team_id || '',
      },
    };
  });
}

// Cytoscape style spec. Uses data-driven selectors (e.g. node[status =
// "operational"]) rather than function-mapper style — the latter exists in
// the v3 API but is more brittle across versions; selectors are guaranteed.
const GRAPH_STYLE = [
  {
    selector: 'node',
    style: {
      'label': 'data(label)',
      'background-color': '#9ca3af',
      'text-valign': 'center',
      'text-halign': 'right',
      'text-margin-x': 6,
      'font-size': 10,
      'font-family': 'Cascadia Code, Fira Code, monospace',
      'width': 18,
      'height': 18,
      'color': '#e2e8f0',
      'border-width': 1,
      'border-color': '#0d0d0d',
    },
  },
  { selector: 'node[status = "operational"]', style: { 'background-color': '#16a34a' } },
  { selector: 'node[status = "degraded"]',    style: { 'background-color': '#d97706' } },
  { selector: 'node[status = "down"]',        style: { 'background-color': '#dc2626' } },
  { selector: 'node[status = "unknown"]',     style: { 'background-color': '#9ca3af' } },
  {
    selector: 'edge',
    style: {
      'width': 1.5,
      'line-color': '#94a3b8',
      'curve-style': 'bezier',
      'target-arrow-shape': 'triangle',
      'target-arrow-color': '#94a3b8',
      'opacity': 0.75,
    },
  },
  { selector: 'edge[criticality = "low"]',    style: { 'width': 1,   'line-color': '#475569', 'target-arrow-color': '#475569' } },
  { selector: 'edge[criticality = "medium"]', style: { 'width': 2,   'line-color': '#94a3b8', 'target-arrow-color': '#94a3b8' } },
  { selector: 'edge[criticality = "high"]',   style: { 'width': 3.5, 'line-color': '#dc2626', 'target-arrow-color': '#dc2626' } },
  { selector: '.faded', style: { 'opacity': 0.12, 'text-opacity': 0.12 } },
  { selector: ':selected', style: { 'border-width': 3, 'border-color': '#4ade80' } },
];

// Render the dependency graph from current state. Idempotent — destroys any
// prior cytoscape instance before creating a new one so re-entering the tab
// doesn't leak listeners.
function renderGraph() {
  const cyEl = document.getElementById('cy');
  if (!cyEl) return;

  if (typeof cytoscape !== 'function') {
    cyEl.textContent = 'cytoscape failed to load';
    return;
  }

  // Tear down any prior instance.
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

  // Build set of node ids so we don't add edges to/from missing nodes
  // (would throw in cytoscape).
  const nodeIds = new Set(nodes.map(n => n.data.id));
  const safeEdges = edges.filter(e => nodeIds.has(e.data.source) && nodeIds.has(e.data.target));

  const cy = cytoscape({
    container: cyEl,
    elements: [...nodes, ...safeEdges],
    style: GRAPH_STYLE,
    layout: {
      name: 'cose',
      animate: false,
      idealEdgeLength: 100,
      nodeOverlap: 20,
      refresh: 20,
      fit: true,
      padding: 40,
      randomize: false,
      componentSpacing: 80,
      nodeRepulsion: 400000,
      edgeElasticity: 100,
      nestingFactor: 5,
      gravity: 80,
      numIter: 1000,
    },
    wheelSensitivity: 0.2,
    minZoom: 0.2,
    maxZoom: 3,
  });

  state.graph.cy = cy;

  // ── Interactions ──
  cy.on('tap', 'node', evt => {
    const node = evt.target;
    cy.elements().addClass('faded');
    node.removeClass('faded');
    node.neighborhood().removeClass('faded');
    showGraphDetail(node.data());
  });

  cy.on('tap', evt => {
    if (evt.target === cy) {
      cy.elements().removeClass('faded');
      const d = document.getElementById('graph-detail');
      if (d) d.hidden = true;
    }
  });

  // ── Toolbar wiring ──
  populateTeamFilter(deployables);
  wireGraphToolbar();

  // Switching back to canvas mode if previously left in table mode.
  applyGraphViewMode(state.graph.tableMode);

  // Build the table fallback up-front so toggling is instant.
  renderGraphTable(deployables, safeEdges);
}

// Side-panel renderer. Looks up the focused Deployable's contracts (via
// services it exposes) and SLAs (on those contracts) from in-memory state.
// No extra round-trip — all six entities are already loaded by loadAll().
function showGraphDetail(nodeData) {
  const panel = document.getElementById('graph-detail');
  if (!panel) return;

  const deployableId = nodeData.id;
  const services = state.data.services || [];
  const exposes = state.data.exposes || [];
  const contracts = state.data.contracts || [];
  const slas = state.data.slas || [];

  // Services this deployable exposes
  const exposedServiceIds = exposes
    .filter(e => e.deployable_id === deployableId)
    .map(e => e.service_id);
  const exposedServices = services.filter(s => exposedServiceIds.includes(s.id));

  // Contracts on those services
  const relevantContracts = contracts.filter(c => exposedServiceIds.includes(c.service_id));

  // SLAs on those contracts
  const relevantContractIds = new Set(relevantContracts.map(c => c.id));
  const relevantSlas = slas.filter(s => relevantContractIds.has(s.contract_id));

  const statusLabel = nodeData.status || 'unknown';
  const team = nodeData.team || '';

  const exposedHtml = exposedServices.length
    ? `<ul>${exposedServices.map(s => `<li>${esc(s.name || s.id)}${s.type ? ' <span style="color: var(--text-dim)">[' + esc(s.type) + ']</span>' : ''}</li>`).join('')}</ul>`
    : '<p class="empty-list">no exposed services</p>';

  const contractsHtml = relevantContracts.length
    ? `<ul>${relevantContracts.map(c => {
        const svcName = services.find(s => s.id === c.service_id)?.name || c.service_id;
        const ver = c.version ? `v${esc(c.version)}` : '';
        const fmt = c.format ? esc(c.format) : '';
        return `<li>${esc(svcName)}${ver ? ' · ' + ver : ''}${fmt ? ' · ' + fmt : ''}</li>`;
      }).join('')}</ul>`
    : '<p class="empty-list">no contracts</p>';

  const slasHtml = relevantSlas.length
    ? `<ul>${relevantSlas.map(s => `<li>${esc(s.metric || '?')}: ${esc(s.target || '?')}${s.window ? ' / ' + esc(s.window) : ''}</li>`).join('')}</ul>`
    : '<p class="empty-list">no SLAs</p>';

  panel.innerHTML = `
    <h2>${esc(nodeData.label || deployableId)}</h2>
    <dl>
      <dt>status</dt><dd><span class="status-dot ${esc(statusLabel)}" aria-hidden="true"></span> ${esc(statusLabel)}</dd>
      <dt>team</dt><dd>${team ? esc(team) : '<span style="color: var(--text-dim)">—</span>'}</dd>
      <dt>id</dt><dd style="color: var(--text-dim)">${esc(deployableId.slice(0, 8))}</dd>
    </dl>
    <h3>exposes</h3>
    ${exposedHtml}
    <h3>contracts</h3>
    ${contractsHtml}
    <h3>SLAs</h3>
    ${slasHtml}
  `;
  panel.hidden = false;
}

// Populate the team filter <select> from the unique teams on deployables.
function populateTeamFilter(deployables) {
  const sel = document.getElementById('filter-team');
  if (!sel) return;
  const teams = new Set();
  for (const d of deployables) {
    const name = d.team?.name;
    if (name) teams.add(name);
  }
  const sorted = [...teams].sort();
  // Preserve current selection across rebuilds.
  const prev = sel.value;
  sel.innerHTML = '<option value="">all teams</option>' +
    sorted.map(t => `<option value="${esc(t)}">${esc(t)}</option>`).join('');
  if (prev && sorted.includes(prev)) sel.value = prev;
}

// Wire the toolbar buttons + checkboxes. Idempotent — uses cloneNode to drop
// stale listeners before re-attaching, since renderGraph() can be called
// many times across a session.
function wireGraphToolbar() {
  const replaceWithClone = (id) => {
    const el = document.getElementById(id);
    if (!el) return null;
    const fresh = el.cloneNode(true);
    el.parentNode.replaceChild(fresh, el);
    return fresh;
  };

  // Criticality checkboxes
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

// Read the current filter UI and apply it to the cytoscape elements.
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
  // Cytoscape doesn't auto-hide edges whose endpoints are hidden; without
  // this pass, filtering by team leaves orphan arrows pointing at empty space.
  cy.edges().forEach(e => {
    const critOk = enabledCrit.has(e.data('criticality') || 'medium');
    const endpointsVisible = e.source().style('display') !== 'none'
                          && e.target().style('display') !== 'none';
    e.style('display', critOk && endpointsVisible ? 'element' : 'none');
  });
}

// Swap between canvas and table representations of the graph. The table is
// the canonical interface for keyboard users — graph nodes aren't focusable
// without bespoke aria-tree machinery we don't yet have.
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
  // Hide the side panel in table mode — it's specific to the canvas focus.
  if (tableMode) {
    const detail = document.getElementById('graph-detail');
    if (detail) detail.hidden = true;
  }
  // Cytoscape needs a resize hint after the container un-hides, or it'll
  // draw at zero-size and require a manual fit().
  if (!tableMode && state.graph.cy) {
    requestAnimationFrame(() => {
      state.graph.cy.resize();
      state.graph.cy.fit(undefined, 40);
    });
  }
}

// Build the keyboard-accessible table fallback. Columns: deployable name,
// status, team, outgoing edge count, incoming edge count. Sortable by any
// column via click on the <th>.
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

  // Default sort: by name asc.
  rows.sort((a, b) => a.name.localeCompare(b.name));

  const render = (data) => {
    tbody.innerHTML = data.map(r => `
      <tr data-id="${esc(r.id)}">
        <td>${esc(r.name)}</td>
        <td><span class="status-dot ${esc(r.status)}" aria-hidden="true"></span> ${esc(r.status)}</td>
        <td>${r.team ? esc(r.team) : '<span style="color: var(--text-dim)">—</span>'}</td>
        <td class="numeric">${r.outgoing}</td>
        <td class="numeric">${r.incoming}</td>
      </tr>
    `).join('');
  };

  // Sort wiring — toggle asc/desc on repeated click.
  const headers = document.querySelectorAll('#graph-table th[data-sort]');
  let currentSort = { key: 'name', asc: true };
  headers.forEach(th => {
    // Clone to drop any prior listener
    const fresh = th.cloneNode(true);
    th.parentNode.replaceChild(fresh, th);
  });
  document.querySelectorAll('#graph-table th[data-sort]').forEach(th => {
    th.addEventListener('click', () => {
      const key = th.dataset.sort;
      const asc = currentSort.key === key ? !currentSort.asc : true;
      currentSort = { key, asc };
      const sorted = [...rows].sort((a, b) => {
        const av = a[key];
        const bv = b[key];
        if (typeof av === 'number' && typeof bv === 'number') {
          return asc ? av - bv : bv - av;
        }
        return asc ? String(av).localeCompare(String(bv)) : String(bv).localeCompare(String(av));
      });
      render(sorted);
    });
  });

  render(rows);
}

// ── Sidebar navigation ────────────────────────────────────────────────────────

// "graph" is a special pseudo-entity — it has no /api or /graph endpoint
// and no row in ENTITIES; it's a separate visualization screen. Recognising
// it as a known key lets sidebar navigation, hash routing, and filter UI
// reset all branch on it consistently.
function isKnownEntity(key) {
  return !!ENTITIES[key] || key === 'graph';
}

function setActiveEntity(entityKey) {
  state.activeEntity = entityKey;
  state.expandedId = null;
  state.filter = '';
  document.getElementById('search').value = '';

  // Reset column filters for the entity being shown so a stale dropdown
  // value doesn't silently hide rows on entity switch.
  if (state.columnFilter[entityKey]) {
    for (const k of Object.keys(state.columnFilter[entityKey])) {
      state.columnFilter[entityKey][k] = '';
    }
  }
  updateStatusFilterUI();

  document.querySelectorAll('.nav-item').forEach(el => {
    el.classList.toggle('active', el.dataset.entity === entityKey);
  });

  hideNewForm();

  // Graph view is rendered into its own screen container; entity-list-area
  // is hidden while the graph is active.
  const listArea = document.getElementById('entity-list-area');
  const graphScreen = document.getElementById('screen-graph');
  if (entityKey === 'graph') {
    if (listArea) listArea.hidden = true;
    if (graphScreen) graphScreen.classList.add('visible');
    document.title = 'groundwork — graph';
    renderGraph();
  } else {
    if (listArea) listArea.hidden = false;
    if (graphScreen) graphScreen.classList.remove('visible');
    document.title = `groundwork — ${entityKey}`;
    renderList();
  }

  if (location.hash.slice(1) !== entityKey) {
    location.hash = entityKey;
  }
}

function initHashRouting() {
  const fromHash = () => {
    const key = location.hash.slice(1);
    if (key && isKnownEntity(key) && key !== state.activeEntity) {
      setActiveEntity(key);
    }
  };
  window.addEventListener('hashchange', fromHash);
  const initial = location.hash.slice(1);
  if (initial && isKnownEntity(initial)) {
    state.activeEntity = initial;
  } else {
    location.replace('#' + state.activeEntity);
  }
}

// ── Status bar ────────────────────────────────────────────────────────────────

function updateStatusCount(total, shown) {
  const el = document.getElementById('status-count');
  if (!el) return;
  const entityKey = state.activeEntity;
  if (shown !== total) {
    el.textContent = `${shown} of ${total} ${entityKey}`;
  } else {
    el.textContent = `${total} ${entityKey}`;
  }
}

function setError(msg) {
  const el = document.getElementById('status-error');
  if (el) el.textContent = msg || '';
}

// ── New record form ───────────────────────────────────────────────────────────

function showNewForm() {
  const entityKey = state.activeEntity;
  const cfg = ENTITIES[entityKey];

  const titleEl = document.getElementById('new-form-title');
  titleEl.textContent = `new ${cfg.label}`;

  const fieldsEl = document.getElementById('new-form-fields');
  fieldsEl.innerHTML = '';

  for (const f of cfg.newFields) {
    const row = document.createElement('div');
    row.className = 'field-row';

    const label = document.createElement('label');
    label.setAttribute('for', `new-${f.name}`);
    label.textContent = f.label;
    label.className = f.required ? 'required' : 'optional';

    let input;
    if (f.type === 'select') {
      input = document.createElement('select');
      input.id = `new-${f.name}`;
      input.name = f.name;
      for (const opt of (f.options || [])) {
        const o = document.createElement('option');
        o.value = opt;
        o.textContent = opt || '—';
        input.appendChild(o);
      }
    } else if (f.type === 'dynamic-select') {
      input = document.createElement('select');
      input.id = `new-${f.name}`;
      input.name = f.name;
      const placeholder = document.createElement('option');
      placeholder.value = '';
      placeholder.textContent = '— select —';
      input.appendChild(placeholder);
      for (const opt of (f.optionsFrom ? f.optionsFrom(state.data) : [])) {
        const o = document.createElement('option');
        o.value = opt.value;
        o.textContent = opt.label;
        input.appendChild(o);
      }
    } else {
      input = document.createElement('input');
      input.id = `new-${f.name}`;
      input.name = f.name;
      input.type = 'text';
      input.autocomplete = 'off';
      input.spellcheck = false;
    }

    if (f.required) input.classList.add('required-field');
    row.append(label, input);
    fieldsEl.appendChild(row);
  }

  const form = document.getElementById('new-form');
  form.classList.add('visible');
  state.newFormOpen = true;

  // Focus first input
  const first = fieldsEl.querySelector('input, select');
  if (first) first.focus();
}

function hideNewForm() {
  document.getElementById('new-form').classList.remove('visible');
  state.newFormOpen = false;
}

async function submitNewForm() {
  const entityKey = state.activeEntity;
  const cfg = ENTITIES[entityKey];
  const fieldsEl = document.getElementById('new-form-fields');
  const fields = {};

  fieldsEl.querySelectorAll('[name]').forEach(el => {
    const val = el.value.trim();
    if (val) fields[el.name] = val;
  });

  // Validate required fields
  for (const f of cfg.newFields) {
    if (f.required && !fields[f.name]) {
      setError(`'${f.label}' is required`);
      const input = fieldsEl.querySelector(`[name="${f.name}"]`);
      if (input) input.focus();
      return;
    }
  }

  try {
    const created = await createRecord(entityKey, fields);
    // REST writes return only local fields; re-read via /graph so federated
    // fields (e.g. Deployable.team) are present on the new row.
    await loadEntity(entityKey);
    hideNewForm();
    state.expandedId = created.id;
    setError('');
    renderList();
  } catch (err) {
    setError(err.message);
  }
}

// ── Keyboard shortcuts ────────────────────────────────────────────────────────

function initKeyboard() {
  const search = document.getElementById('search');

  document.addEventListener('keydown', e => {
    const tag = document.activeElement?.tagName?.toLowerCase();
    const inInput = tag === 'input' || tag === 'textarea' || tag === 'select';

    if (e.key === 'Escape') {
      if (state.newFormOpen) { hideNewForm(); return; }
      if (state.expandedId !== null) { expandRow(null); return; }
      if (document.activeElement === search) {
        search.value = '';
        state.filter = '';
        renderList();
      }
      return;
    }

    if (e.key === '/' && !inInput) {
      e.preventDefault();
      search.focus();
      search.select();
      return;
    }

    if (e.key === 'n' && !inInput) {
      e.preventDefault();
      showNewForm();
      return;
    }

    // Enter in new form fields: submit
    if (e.key === 'Enter' && state.newFormOpen && inInput) {
      e.preventDefault();
      submitNewForm();
      return;
    }
  });

  search.addEventListener('input', () => {
    state.filter = search.value;
    renderList();
  });
}

function initSidebar() {
  document.querySelectorAll('.nav-item').forEach(el => {
    el.addEventListener('click', () => setActiveEntity(el.dataset.entity));
  });
}

// Show/hide and reset the deployment-status filter based on active entity.
// The <select> lives in the header but is only meaningful for deployables.
function updateStatusFilterUI() {
  const wrap = document.getElementById('status-filter-wrap');
  const sel = document.getElementById('status-filter');
  if (!wrap || !sel) return;
  const isDeployables = state.activeEntity === 'deployables';
  wrap.hidden = !isDeployables;
  if (isDeployables) {
    sel.value = state.columnFilter.deployables.deployment_status || '';
  }
}

function initStatusFilter() {
  const sel = document.getElementById('status-filter');
  if (!sel) return;
  sel.addEventListener('change', () => {
    state.columnFilter.deployables.deployment_status = sel.value;
    renderList();
  });
  updateStatusFilterUI();
}

// ── Bootstrap ─────────────────────────────────────────────────────────────────

async function init() {
  initKeyboard();
  initSidebar();
  initStatusFilter();
  initHashRouting();

  document.querySelectorAll('.nav-item').forEach(el => {
    el.classList.toggle('active', el.dataset.entity === state.activeEntity);
  });

  // /config.json publishes cross-app public URLs; needed before first render
  // so that cross-app anchors land with the right base.
  await loadConfig();

  try {
    await loadAll();
    setError('');
  } catch (err) {
    setError(err.message);
  }

  // Route through setActiveEntity so a cold-load at #graph takes the graph
  // branch — renderList() would crash because ENTITIES['graph'] is undefined.
  setActiveEntity(state.activeEntity);
  document.getElementById('search').focus();
}

init();
