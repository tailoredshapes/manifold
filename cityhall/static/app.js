// cityhall — org hierarchy, bylaws, plans.
// Vanilla JS ES module, no build step, no framework.

// ── Entity config ─────────────────────────────────────────────────────────────

const ORG_KINDS = ['', 'enterprise', 'division', 'domain', 'product', 'team'];
const GATE_TYPES = ['', 'AutoGate', 'ApprovalGate', 'WindowGate', 'QuiesceGate', 'FreezePeriod'];
const TIERS = ['', 'dev', 'integration', 'uat', 'prod'];
const CR_STATUSES = ['', 'draft', 'submitted', 'approved', 'rejected', 'deployed', 'rolled_back'];

const ENTITIES = {
  orgNodes: {
    api: '/org_node/api',
    label: 'org node',
    newFields: [
      { name: 'name', label: 'name', type: 'text', required: true },
      { name: 'kind', label: 'kind', type: 'select', required: true, options: ORG_KINDS },
      { name: 'parent_id', label: 'parent', type: 'dynamic-select', required: false,
        optionsFrom: (data) => [{value: '', label: '— none —'}].concat(data.orgNodes.map(n => ({ value: n.id, label: `${n.payload?.name || n.id} (${n.payload?.kind || ''})` }))) },
      { name: 'team_id', label: 'team_id', type: 'text', required: false },
    ],
    detailFields: [
      { name: 'kind', label: 'kind', type: 'select', options: ORG_KINDS },
      { name: 'parent_id', label: 'parent_id', type: 'text' },
      { name: 'team_id', label: 'team_id', type: 'text' },
    ],
    primaryField: 'name',
    getRowLabel: (payload) => `${payload.name || '?'} [${payload.kind || '?'}]`,
    getRowBadge: (payload) => payload.kind || null,
  },

  bylaws: {
    api: '/bylaw/api',
    label: 'bylaw',
    newFields: [
      { name: 'org_node_id', label: 'org node', type: 'dynamic-select', required: true,
        optionsFrom: (data) => data.orgNodes.map(n => ({ value: n.id, label: `${n.payload?.name || n.id} (${n.payload?.kind || ''})` })) },
      { name: 'gate_type', label: 'gate', type: 'select', required: true, options: GATE_TYPES },
      { name: 'priority', label: 'priority', type: 'text', required: false },
      { name: 'description', label: 'description', type: 'text', required: false },
      { name: 'window', label: 'window', type: 'text', required: false },
      { name: 'quiesce_for', label: 'quiesce_for', type: 'text', required: false },
      { name: 'approvers', label: 'approvers', type: 'text', required: false },
      { name: 'conditions', label: 'conditions (JSON)', type: 'text', required: false },
    ],
    detailFields: [
      { name: 'gate_type', label: 'gate_type', type: 'select', options: GATE_TYPES },
      { name: 'priority', label: 'priority', type: 'text' },
      { name: 'description', label: 'description', type: 'textarea' },
      { name: 'window', label: 'window', type: 'text' },
      { name: 'quiesce_for', label: 'quiesce_for', type: 'text' },
      { name: 'approvers', label: 'approvers', type: 'text' },
      { name: 'conditions', label: 'conditions', type: 'textarea' },
    ],
    primaryField: 'org_node_id',
    getRowLabel: (payload, data) => {
      const node = data.orgNodes.find(n => n.id === payload.org_node_id)?.payload?.name
        || payload.org_node_id || '?';
      return `${payload.gate_type || '?'} @ ${node}`;
    },
    getRowBadge: (payload) => payload.gate_type || null,
    readonlyInDetail: [
      { name: 'org_node_id', label: 'org node',
        resolve: (payload, data) => data.orgNodes.find(n => n.id === payload.org_node_id)?.payload?.name || payload.org_node_id || '—' },
    ],
  },

  changeRequests: {
    api: '/change_request/api',
    label: 'change request',
    newFields: [
      { name: 'summary', label: 'summary', type: 'text', required: true },
      { name: 'description', label: 'description', type: 'text', required: false },
      { name: 'tier', label: 'tier', type: 'select', required: false, options: TIERS },
      { name: 'status', label: 'status', type: 'select', required: false, options: CR_STATUSES },
      { name: 'target_deployables', label: 'target_deployables (JSON array)', type: 'text', required: false },
      { name: 'target_versions', label: 'target_versions (JSON object)', type: 'text', required: false },
      { name: 'requested_by', label: 'requested_by (Person id)', type: 'text', required: false },
    ],
    detailFields: [
      { name: 'summary', label: 'summary', type: 'textarea' },
      { name: 'description', label: 'description', type: 'textarea' },
      { name: 'tier', label: 'tier', type: 'select', options: TIERS },
      { name: 'status', label: 'status', type: 'select', options: CR_STATUSES },
      { name: 'target_deployables', label: 'target_deployables', type: 'textarea' },
      { name: 'target_versions', label: 'target_versions', type: 'textarea' },
      { name: 'requested_by', label: 'requested_by', type: 'text' },
    ],
    primaryField: 'summary',
    getRowLabel: (payload) => `${payload.summary || '?'} [${payload.tier || '?'}]`,
    getRowBadge: (payload) => payload.status || null,
  },

  plans: {
    api: '/deployment_plan/api',
    label: 'plan',
    newFields: [
      { name: 'change_request_id', label: 'change_request_id', type: 'text', required: true },
      { name: 'tier', label: 'tier', type: 'select', required: false, options: TIERS },
    ],
    detailFields: [
      { name: 'tier', label: 'tier', type: 'select', options: TIERS },
      { name: 'steps', label: 'steps (JSON)', type: 'textarea' },
      { name: 'blockers', label: 'blockers (JSON)', type: 'textarea' },
      { name: 'computed_at', label: 'computed_at', type: 'text' },
    ],
    primaryField: 'change_request_id',
    getRowLabel: (payload) => `${payload.summary || payload.change_request_id || 'plan'} [${payload.tier || '?'}]`,
    getRowBadge: (payload) => {
      const blockers = (() => { try { return JSON.parse(payload.blockers || '[]'); } catch { return []; } })();
      return blockers.length ? 'blocked' : (payload.tier || null);
    },
  },

  gantts: {
    api: '/gantt_output/api',
    label: 'gantt',
    newFields: [
      { name: 'deployment_plan_id', label: 'deployment_plan_id', type: 'text', required: true },
      { name: 'tier', label: 'tier', type: 'text', required: false },
    ],
    detailFields: [
      { name: 'tier', label: 'tier', type: 'text' },
      { name: 'mermaid', label: 'mermaid', type: 'textarea' },
    ],
    primaryField: 'deployment_plan_id',
    getRowLabel: (payload) => `gantt [${payload.tier || '?'}]`,
    getRowBadge: null,
  },
};

// ── State ─────────────────────────────────────────────────────────────────────

const state = {
  activeEntity: 'orgNodes',
  data: { orgNodes: [], bylaws: [], changeRequests: [], plans: [], gantts: [] },
  expandedId: null,
  filter: '',
  newFormOpen: false,
};

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

async function loadEntity(entityKey) {
  const cfg = ENTITIES[entityKey];
  const items = await apiFetch(cfg.api);
  state.data[entityKey] = Array.isArray(items) ? items : [];
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

  const visible = needle
    ? items.filter(item => {
        const label = cfg.getRowLabel(item.payload ?? {}, state.data).toLowerCase();
        return label.includes(needle) || (item.id || '').toLowerCase().includes(needle);
      })
    : items;

  const list = document.getElementById('entity-list');
  list.innerHTML = '';

  if (visible.length === 0) {
    list.innerHTML = `<li class="empty-state">${needle ? 'no matches' : `no ${entityKey} yet — press n to add one`}</li>`;
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
  const payload = item.payload ?? {};
  const label = cfg.getRowLabel(payload, state.data);
  const badge = cfg.getRowBadge ? cfg.getRowBadge(payload) : null;

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

  if (badge) {
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
      const val = rf.resolve(payload, state.data);
      html += `
        <div class="field-row">
          <label>${esc(rf.label)}</label>
          <span class="readonly-val">${esc(val)}</span>
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
    if (original.payload?.[pf] !== undefined) {
      fields[pf] = original.payload[pf];
    }
    // Preserve all readonly FK fields too
    if (cfg.readonlyInDetail) {
      for (const rf of cfg.readonlyInDetail) {
        if (original.payload?.[rf.name] !== undefined) {
          fields[rf.name] = original.payload[rf.name];
        }
      }
    }
  }

  try {
    const updated = await updateRecord(entityKey, id, fields);
    const idx = state.data[entityKey].findIndex(a => a.id === id);
    if (idx !== -1) state.data[entityKey][idx] = updated;
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
    state.data[entityKey] = state.data[entityKey].filter(a => a.id !== id);
    if (state.expandedId === id) state.expandedId = null;
    updateBadge(entityKey);
    setError('');
    renderList();
  } catch (err) {
    setError(err.message);
  }
}

// ── Sidebar navigation ────────────────────────────────────────────────────────

function setActiveEntity(entityKey) {
  state.activeEntity = entityKey;
  state.expandedId = null;
  state.filter = '';
  document.getElementById('search').value = '';

  document.querySelectorAll('.nav-item').forEach(el => {
    el.classList.toggle('active', el.dataset.entity === entityKey);
  });

  hideNewForm();
  renderList();
}

// ── Status bar ────────────────────────────────────────────────────────────────

function updateStatusCount(total, shown) {
  const el = document.getElementById('status-count');
  if (!el) return;
  const entityKey = state.activeEntity;
  if (state.filter && shown !== total) {
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
    state.data[entityKey].unshift(created);
    updateBadge(entityKey);
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

// ── Bootstrap ─────────────────────────────────────────────────────────────────

async function init() {
  initKeyboard();
  initSidebar();

  try {
    await loadAll();
    setError('');
  } catch (err) {
    setError(err.message);
  }

  renderList();
  document.getElementById('search').focus();
}

init();
