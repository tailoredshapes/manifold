// yard — test coordinator (environments, runs, sync, settings)
// Vanilla JS ES module · no build step · no framework · no charting library.

// ── Enum vocab ───────────────────────────────────────────────────────────────

const ENV_KINDS         = ['mock', 'stub', 'sandbox', 'isolated', 'multi-tenant', 'external'];
const TEARDOWN_POLICIES = ['on_finish', 'on_idle', 'manual', 'never'];
const PROVIDERS         = ['docker', 'aws_ec2', 'aws_ecs', 'azure_container', 'gcp_gke', 'kubernetes', 'external_saas', 'local'];
const DATA_KINDS        = ['prod_snapshot', 'synthetic', 'fixtures', 'external_mock'];
const SYNC_KINDS        = ['push', 'pull', 'shared'];
const REFRESH_POLICIES  = ['on_demand', 'periodic', 'per_test_run', 'versioned'];
const RUN_STATUSES      = ['pending', 'running', 'passed', 'failed', 'cancelled', 'errored'];

// ── Entity definitions (used by Settings + new-record modal) ─────────────────

const ENTITIES = {
  testEnvironments: {
    api: '/test_environment/api',
    label: 'Test environment',
    primaryField: 'name',
    fields: [
      { name: 'name',                 label: 'Name',                  type: 'text',    required: true },
      { name: 'kind',                 label: 'Kind',                  type: 'select',  required: true, options: ENV_KINDS },
      { name: 'deployable_id',        label: 'Deployable ID',         type: 'text' },
      { name: 'service_id',           label: 'Service ID',            type: 'text' },
      { name: 'infrastructure_id',    label: 'Infrastructure',        type: 'ref',     refKey: 'testInfrastructures' },
      { name: 'mock_source_id',       label: 'Mock source',           type: 'ref',     refKey: 'mockSources' },
      { name: 'cost_per_hour',        label: 'Cost / hour ($)',       type: 'text' },
      { name: 'spinup_minutes',       label: 'Spinup minutes',        type: 'text' },
      { name: 'teardown_policy',      label: 'Teardown',              type: 'select',  options: TEARDOWN_POLICIES },
      { name: 'max_duration_minutes', label: 'Max duration (min)',    type: 'text' },
      { name: 'concurrency_limit',    label: 'Concurrency limit',     type: 'text' },
      { name: 'rate_limit',           label: 'Rate limit',            type: 'text' },
      { name: 'contractual_limit',    label: 'Contractual limit',     type: 'text' },
      { name: 'notes',                label: 'Notes',                 type: 'textarea', full: true },
    ],
    rowLabel: p => p.name || '(unnamed)',
    rowMeta:  p => p.kind || '',
  },
  testInfrastructures: {
    api: '/test_infrastructure/api',
    label: 'Infrastructure',
    primaryField: 'name',
    fields: [
      { name: 'name',          label: 'Name',          type: 'text',   required: true },
      { name: 'provider',      label: 'Provider',      type: 'select', required: true, options: PROVIDERS },
      { name: 'region',        label: 'Region',        type: 'text' },
      { name: 'instance_type', label: 'Instance type', type: 'text' },
      { name: 'cost_per_hour', label: 'Cost / hour',   type: 'text' },
      { name: 'notes',         label: 'Notes',         type: 'textarea', full: true },
    ],
    rowLabel: p => p.name || '(unnamed)',
    rowMeta:  p => [p.provider, p.region].filter(Boolean).join(' · '),
  },
  mockSources: {
    api: '/mock_source/api',
    label: 'Mock source',
    primaryField: 'name',
    fields: [
      { name: 'name',     label: 'Name',     type: 'text', required: true },
      { name: 'repo_url', label: 'Repo URL', type: 'text' },
      { name: 'path',     label: 'Path',     type: 'text' },
      { name: 'language', label: 'Language', type: 'text' },
      { name: 'notes',    label: 'Notes',    type: 'textarea', full: true },
    ],
    rowLabel: p => p.name || '(unnamed)',
    rowMeta:  p => [p.language, p.repo_url].filter(Boolean).join(' · '),
  },
  dataSources: {
    api: '/data_source/api',
    label: 'Data source',
    primaryField: 'name',
    fields: [
      { name: 'name',           label: 'Name',           type: 'text',   required: true },
      { name: 'kind',           label: 'Kind',           type: 'select', required: true, options: DATA_KINDS },
      { name: 'location',       label: 'Location',       type: 'text' },
      { name: 'refresh_policy', label: 'Refresh policy', type: 'select', options: REFRESH_POLICIES },
      { name: 'notes',          label: 'Notes',          type: 'textarea', full: true },
    ],
    rowLabel: p => p.name || '(unnamed)',
    rowMeta:  p => [p.kind, p.refresh_policy].filter(Boolean).join(' · '),
  },
  dataSyncs: {
    api: '/data_sync/api',
    label: 'Data sync',
    primaryField: 'target_env_id',
    fields: [
      { name: 'kind',              label: 'Kind',              type: 'select', required: true, options: SYNC_KINDS },
      { name: 'target_env_id',     label: 'Target env',        type: 'ref',    required: true, refKey: 'testEnvironments' },
      { name: 'source_env_id',     label: 'Source env',        type: 'ref',    refKey: 'testEnvironments' },
      { name: 'source_data_id',    label: 'Source data',       type: 'ref',    refKey: 'dataSources' },
      { name: 'refresh_policy',    label: 'Refresh policy',    type: 'select', options: REFRESH_POLICIES },
      { name: 'estimated_minutes', label: 'Estimated minutes', type: 'text' },
      { name: 'notes',             label: 'Notes',             type: 'textarea', full: true },
    ],
    rowLabel: (p, data) => {
      const t = data.testEnvironments.find(e => e.id === p.target_env_id)?.name || p.target_env_id || '?';
      const s = p.source_env_id
        ? data.testEnvironments.find(e => e.id === p.source_env_id)?.name
        : data.dataSources.find(d => d.id === p.source_data_id)?.name;
      return `${s || '?'} → ${t}`;
    },
    rowMeta: p => p.kind || '',
  },
  testRuns: {
    api: '/test_run/api',
    label: 'Test run',
    primaryField: 'test_environment_id',
    fields: [
      { name: 'test_environment_id', label: 'Environment',     type: 'ref',    required: true, refKey: 'testEnvironments' },
      { name: 'change_request_id',   label: 'Change request',  type: 'text' },
      { name: 'test_suite_id',       label: 'Test suite',      type: 'ref',    refKey: 'testSuites' },
      { name: 'team_id',             label: 'Team',            type: 'text' },
      { name: 'started_at',          label: 'Started at',      type: 'text' },
      { name: 'finished_at',         label: 'Finished at',     type: 'text' },
      { name: 'status',              label: 'Status',          type: 'select', options: RUN_STATUSES },
      { name: 'duration_minutes',    label: 'Duration (min)',  type: 'text' },
      { name: 'cost_actual',         label: 'Cost actual ($)', type: 'text' },
    ],
    rowLabel: p => p.test_environment_id || '(unset)',
    rowMeta:  p => p.status || 'pending',
  },
  testSuites: {
    api: '/test_suite/api',
    label: 'Test suite',
    primaryField: 'name',
    fields: [
      { name: 'name',          label: 'Name',          type: 'text', required: true },
      { name: 'deployable_id', label: 'Deployable ID', type: 'text' },
      { name: 'runner',        label: 'Runner',        type: 'text' },
      { name: 'command',       label: 'Command',       type: 'text' },
      { name: 'description',   label: 'Description',   type: 'textarea', full: true },
    ],
    rowLabel: p => p.name || '(unnamed)',
    rowMeta:  p => p.runner || '',
  },
};

// ── State ────────────────────────────────────────────────────────────────────

const state = {
  screen: 'environments',                  // environments | runs | sync | settings:<entity>
  data: {
    testEnvironments: [],
    testInfrastructures: [],
    mockSources: [],
    dataSources: [],
    dataSyncs: [],
    testRuns: [],
    testSuites: [],
  },
  availability: new Map(),                 // env id → { status: 'available'|'cap'|'blocked'|'unknown', raw }
  history:      new Map(),                 // env id → history payload
  expandedEnvId: null,
  expandedRunId: null,
  expandedSettingId: null,
  runFilter: 'all',                        // all | <RUN_STATUS>
  syncEnvId: null,
  search: '',
  loading: false,
  modal: { open: false, entityKey: null },
};

// ── DOM helpers ──────────────────────────────────────────────────────────────

const $  = (sel, root = document) => root.querySelector(sel);
const $$ = (sel, root = document) => Array.from(root.querySelectorAll(sel));

function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v == null || v === false) continue;
    if (k === 'class')        node.className = v;
    else if (k === 'dataset') Object.assign(node.dataset, v);
    else if (k === 'html')    node.innerHTML = v;
    else if (k.startsWith('on') && typeof v === 'function')
                              node.addEventListener(k.slice(2).toLowerCase(), v);
    else if (k in node)       try { node[k] = v; } catch { node.setAttribute(k, v); }
    else                      node.setAttribute(k, v);
  }
  for (const c of children.flat()) {
    if (c == null || c === false) continue;
    node.appendChild(c.nodeType ? c : document.createTextNode(String(c)));
  }
  return node;
}

function esc(s) {
  return String(s ?? '')
    .replace(/&/g, '&amp;').replace(/"/g, '&quot;')
    .replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

// ── API ──────────────────────────────────────────────────────────────────────

async function apiFetch(url, opts) {
  const res = await fetch(url, opts);
  if (!res.ok) {
    const body = await res.text().catch(() => '');
    throw new Error(`${opts?.method || 'GET'} ${url} → ${res.status}${body ? ': ' + body : ''}`);
  }
  if (res.status === 204) return null;
  const ctype = res.headers.get('content-type') || '';
  return ctype.includes('application/json') ? res.json() : res.text();
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

async function loadEntity(key) {
  const items = await apiFetch(ENTITIES[key].api);
  state.data[key] = Array.isArray(items) ? items : [];
}

async function loadAll() {
  const [
    testEnvironments,
    testInfrastructures,
    mockSources,
    dataSources,
    dataSyncs,
    testRuns,
    testSuites,
  ] = await Promise.all([
    gqlQuery(
      '/test_environment/graph',
      '{ getAll { id name kind deployable_id service_id infrastructure_id mock_source_id cost_per_hour spinup_minutes teardown_policy max_duration_minutes concurrency_limit rate_limit contractual_limit notes } }'
    ).then(d => d.getAll).catch(() => []),
    gqlQuery(
      '/test_infrastructure/graph',
      '{ getAll { id name provider region instance_type cost_per_hour notes } }'
    ).then(d => d.getAll).catch(() => []),
    apiFetch(ENTITIES.mockSources.api).catch(() => []),
    apiFetch(ENTITIES.dataSources.api).catch(() => []),
    apiFetch(ENTITIES.dataSyncs.api).catch(() => []),
    apiFetch(ENTITIES.testRuns.api).catch(() => []),
    apiFetch(ENTITIES.testSuites.api).catch(() => []),
  ]);
  state.data.testEnvironments    = Array.isArray(testEnvironments) ? testEnvironments : [];
  state.data.testInfrastructures = Array.isArray(testInfrastructures) ? testInfrastructures : [];
  state.data.mockSources         = Array.isArray(mockSources) ? mockSources : [];
  state.data.dataSources         = Array.isArray(dataSources) ? dataSources : [];
  state.data.dataSyncs           = Array.isArray(dataSyncs) ? dataSyncs : [];
  state.data.testRuns            = Array.isArray(testRuns) ? testRuns : [];
  state.data.testSuites          = Array.isArray(testSuites) ? testSuites : [];
}

async function createRecord(key, payload) {
  return apiFetch(ENTITIES[key].api, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
}

async function updateRecord(key, id, payload) {
  return apiFetch(`${ENTITIES[key].api}/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
}

async function deleteRecord(key, id) {
  return apiFetch(`${ENTITIES[key].api}/${id}`, { method: 'DELETE' });
}

async function fetchAvailability(envId) {
  return apiFetch(`/test_environment/${envId}/availability`);
}

async function fetchHistory(envId) {
  return apiFetch(`/test_environment/${envId}/history`);
}

// ── Footer meta / empty-card helpers ─────────────────────────────────────────

function updateFooterMeta(text) {
  const node = document.getElementById('footer-meta');
  if (node) node.textContent = text || '';
}

function emptyCard({ title, lede, hintHtml }) {
  return el('div', { class: 'empty-card' },
    el('span', { class: 'empty-mark' }, '§'),
    el('h3', {}, title),
    el('p', { class: 'lede' }, lede),
    el('p', { class: 'hint', html: hintHtml || 'Press <kbd>n</kbd> to register the first one.' }),
  );
}

// ── Status strip ─────────────────────────────────────────────────────────────

let statusTimer = null;

function setStatus(message, kind = 'info', { sticky = false } = {}) {
  const strip = $('#status-strip');
  strip.classList.remove('error', 'info');
  if (!message) {
    strip.style.display = 'none';
    strip.textContent = '';
    return;
  }
  strip.classList.add(kind === 'error' ? 'error' : 'info');
  strip.textContent = message;
  strip.style.display = 'block';
  if (statusTimer) clearTimeout(statusTimer);
  if (!sticky) {
    statusTimer = setTimeout(() => {
      strip.style.display = 'none';
      strip.textContent = '';
    }, kind === 'error' ? 6000 : 3000);
  }
}

function setError(err) {
  if (!err) return setStatus('');
  setStatus(err.message || String(err), 'error');
}

// ── Availability classification ──────────────────────────────────────────────

function classifyAvailability(raw) {
  if (!raw || typeof raw !== 'object') return 'unknown';
  const explicit = (raw.status || raw.state || '').toLowerCase();
  if (explicit) {
    if (['available', 'ok', 'idle', 'free'].includes(explicit))           return 'available';
    if (['at_concurrency', 'concurrency_cap', 'cap', 'busy'].includes(explicit))
                                                                          return 'cap';
    if (['contractual_cap', 'blocked', 'over_quota', 'rate_limited'].includes(explicit))
                                                                          return 'blocked';
  }
  if (raw.contractual_blocked === true) return 'blocked';
  if (raw.at_contractual_cap === true)  return 'blocked';
  if (raw.at_concurrency_cap === true)  return 'cap';
  if (typeof raw.available === 'boolean') return raw.available ? 'available' : 'cap';
  return 'unknown';
}

function statusLabel(s) {
  switch (s) {
    case 'available': return 'available';
    case 'cap':       return 'at concurrency cap';
    case 'blocked':   return 'at contractual cap';
    default:          return 'unknown';
  }
}

// ── Formatting ───────────────────────────────────────────────────────────────

function fmtCost(v) {
  if (v == null || v === '') return '—';
  const n = parseFloat(v);
  if (Number.isFinite(n)) return `$${n.toFixed(2)}/h`;
  return String(v);
}

function fmtMinutes(v) {
  if (v == null || v === '') return '—';
  return `${v} min`;
}

function fmtNum(v, digits = 2) {
  if (v == null || v === '') return '—';
  const n = parseFloat(v);
  if (Number.isFinite(n)) return n.toFixed(digits);
  return String(v);
}

// ── Field renderer (forms) ───────────────────────────────────────────────────

function fieldInput(field, value) {
  const id = `f-${field.name}-${Math.random().toString(36).slice(2, 8)}`;
  let input;
  if (field.type === 'textarea') {
    input = el('textarea', { id, name: field.name, rows: 2 }, value ?? '');
  } else if (field.type === 'select') {
    input = el('select', { id, name: field.name });
    input.appendChild(el('option', { value: '' }, '—'));
    for (const opt of field.options || []) {
      const o = el('option', { value: opt }, opt);
      if (opt === value) o.selected = true;
      input.appendChild(o);
    }
  } else if (field.type === 'ref') {
    input = el('select', { id, name: field.name });
    input.appendChild(el('option', { value: '' }, '— none —'));
    for (const item of state.data[field.refKey] || []) {
      const lbl = item.name || item.target_env_id || item.id;
      const o = el('option', { value: item.id }, lbl);
      if (item.id === value) o.selected = true;
      input.appendChild(o);
    }
  } else {
    input = el('input', {
      id, name: field.name, type: 'text',
      value: value ?? '',
      autocomplete: 'off', spellcheck: false,
    });
  }
  return el('div', { class: 'field' + (field.full ? ' full' : '') },
    el('label', { for: id }, field.label + (field.required ? ' *' : '')),
    input,
  );
}

function readForm(container) {
  const out = {};
  $$('[name]', container).forEach(node => {
    const v = node.value.trim();
    if (v !== '') out[node.name] = v;
  });
  return out;
}

// ── Modal (new record) ───────────────────────────────────────────────────────

function openNewModal(entityKey) {
  const cfg = ENTITIES[entityKey];
  if (!cfg) return;
  state.modal.open = true;
  state.modal.entityKey = entityKey;

  $('#modal-title').textContent = `New ${cfg.label.toLowerCase()}`;
  const fieldsEl = $('#modal-fields');
  fieldsEl.innerHTML = '';
  for (const f of cfg.fields) fieldsEl.appendChild(fieldInput(f, ''));

  $('#modal-root').classList.add('open');
  const first = $('input, select, textarea', fieldsEl);
  if (first) first.focus();
}

function closeModal() {
  state.modal.open = false;
  state.modal.entityKey = null;
  $('#modal-root').classList.remove('open');
}

async function saveNewModal() {
  const key = state.modal.entityKey;
  if (!key) return;
  const cfg = ENTITIES[key];
  const fieldsEl = $('#modal-fields');
  const payload = readForm(fieldsEl);
  for (const f of cfg.fields) {
    if (f.required && !payload[f.name]) {
      setError(new Error(`${f.label} is required`));
      const node = $(`[name="${f.name}"]`, fieldsEl);
      if (node) node.focus();
      return;
    }
  }
  try {
    const created = await createRecord(key, payload);
    state.data[key].unshift(created);
    closeModal();
    setStatus(`${cfg.label} created`);
    render();
  } catch (e) { setError(e); }
}

// ── Screens: dispatcher ──────────────────────────────────────────────────────

function render() {
  const root = $('#screen-root');
  root.innerHTML = '';
  $$('#primary-nav .tab').forEach(t => {
    // Tabs become "active" for both top-nav screens and any settings:* sub-screen.
    // For settings:* sub-screens, none of the top tabs should be highlighted.
    t.classList.toggle('active', t.dataset.screen === state.screen);
  });
  // Default footer meta — individual screens overwrite below.
  updateFooterMeta('');

  if (state.screen === 'environments')           renderEnvironments(root);
  else if (state.screen === 'runs')              renderRuns(root);
  else if (state.screen === 'sync')              renderSync(root);
  else if (state.screen.startsWith('settings:')) renderSettings(root, state.screen.slice('settings:'.length));
  else                                           renderEnvironments(root);
}

// ── Screen: Environments ─────────────────────────────────────────────────────

function renderEnvironments(root) {
  const envs = state.data.testEnvironments;
  const needle = state.search.trim().toLowerCase();
  const visible = needle
    ? envs.filter(e => (e.name || '').toLowerCase().includes(needle)
                    || (e.kind || '').toLowerCase().includes(needle))
    : envs;

  updateFooterMeta(
    `${envs.length} ${envs.length === 1 ? 'environment' : 'environments'} · ${state.data.testRuns.length} runs`
  );

  root.appendChild(
    el('div', { class: 'section-head' },
      el('div', {},
        el('h1', {}, 'Test environments'),
        el('div', { class: 'meta' }, `${visible.length} of ${envs.length}`),
      ),
    ),
  );

  if (visible.length === 0) {
    if (envs.length === 0) {
      root.appendChild(emptyCard({
        title: 'No environments yet',
        lede:  'An environment is a promise about cost, time, and constraint.',
      }));
    } else {
      root.appendChild(el('div', { class: 'empty' }, 'No environments match your search.'));
    }
    return;
  }

  const grid = el('div', { class: 'card-grid' });
  for (const item of visible) grid.appendChild(buildEnvCard(item));
  root.appendChild(grid);

  // lazily fetch availability for each card after render
  queueMicrotask(() => loadAvailabilityForVisible(visible));
}

function pillKindClass(kind) {
  const k = (kind || '').replace(/[^a-z-]/gi, '').toLowerCase();
  return 'pill k-' + (k || 'unknown');
}

function buildEnvCard(item) {
  const id = item.id;
  const expanded = state.expandedEnvId === id;

  const card = el('div', {
    class: 'card env-card' + (expanded ? ' expanded' : ''),
    dataset: { id },
  });

  // Head
  card.appendChild(el('div', { class: 'head' },
    el('div', { class: 'name' }, item.name || '(unnamed)'),
    el('span', { class: pillKindClass(item.kind) }, item.kind || 'unknown'),
  ));

  // Stats
  card.appendChild(el('div', { class: 'stat-grid' },
    statBlock('Cost / hour',  fmtCost(item.cost_per_hour)),
    statBlock('Spinup',       fmtMinutes(item.spinup_minutes)),
    statBlock('Max duration', fmtMinutes(item.max_duration_minutes)),
    statBlock('Teardown',     item.teardown_policy || '—'),
  ));

  // Constraints summary
  const cParts = [];
  if (item.rate_limit)        cParts.push(`rate ≤ ${item.rate_limit}`);
  if (item.concurrency_limit) cParts.push(`concurrency ≤ ${item.concurrency_limit}`);
  if (item.contractual_limit) cParts.push(`contractual ≤ ${item.contractual_limit}`);
  if (cParts.length) {
    card.appendChild(el('div', { class: 'constraints' }, cParts.join(' · ')));
  }

  // Footer (status + id)
  const av  = state.availability.get(id) || 'unknown';
  card.appendChild(el('div', { class: 'status-foot' },
    el('span', { class: 'status-label' },
      el('span', { class: 'dot s-' + av }),
      statusLabel(av),
    ),
    el('span', { class: 'id' }, id ? id.slice(0, 8) : ''),
  ));

  // Detail
  if (expanded) card.appendChild(buildEnvDetail(item));

  // Click to expand (ignore clicks inside the detail panel)
  card.addEventListener('click', (e) => {
    if (e.target.closest('.env-detail')) return;
    state.expandedEnvId = expanded ? null : id;
    render();
  });

  return card;
}

function statBlock(label, value) {
  return el('div', { class: 'stat' },
    el('div', { class: 'lbl' }, label),
    el('div', { class: 'val' }, value),
  );
}

function buildEnvDetail(item) {
  const id = item.id;
  const cfg = ENTITIES.testEnvironments;
  const detail = el('div', { class: 'env-detail' });

  // Editable form (subset)
  const form = el('div', { class: 'form-grid' });
  const editable = ['kind', 'cost_per_hour', 'spinup_minutes', 'teardown_policy',
                    'max_duration_minutes', 'concurrency_limit', 'rate_limit',
                    'contractual_limit', 'notes'];
  for (const fname of editable) {
    const f = cfg.fields.find(x => x.name === fname);
    if (!f) continue;
    form.appendChild(fieldInput(f, item[fname]));
  }
  detail.appendChild(form);

  // History block (filled when "Run history" pressed)
  const histBlock = el('div', { class: 'history-block', style: 'display:none' });
  detail.appendChild(histBlock);

  const cached = state.history.get(id);
  if (cached) {
    histBlock.style.display = 'block';
    histBlock.appendChild(renderHistoryStats(cached));
  }

  // Actions
  const actions = el('div', { class: 'row-actions' },
    el('button', { class: 'primary',
      onClick: async () => {
        const payload = readForm(form);
        if (item.name) payload.name = item.name;
        try {
          const updated = await updateRecord('testEnvironments', id, payload);
          const idx = state.data.testEnvironments.findIndex(x => x.id === id);
          if (idx !== -1) state.data.testEnvironments[idx] = updated;
          setStatus('Saved');
          render();
        } catch (err) { setError(err); }
      },
    }, 'Save'),
    el('button', {
      onClick: async () => {
        try {
          setStatus('Loading run history…');
          const h = await fetchHistory(id);
          state.history.set(id, h);
          histBlock.innerHTML = '';
          histBlock.style.display = 'block';
          histBlock.appendChild(renderHistoryStats(h));
          setStatus('');
        } catch (err) { setError(err); }
      },
    }, 'Run history'),
    el('button', { class: 'danger',
      onClick: async () => {
        if (!confirm(`Delete environment "${item.name || id}"?`)) return;
        try {
          await deleteRecord('testEnvironments', id);
          state.data.testEnvironments = state.data.testEnvironments.filter(x => x.id !== id);
          state.expandedEnvId = null;
          setStatus('Deleted');
          render();
        } catch (err) { setError(err); }
      },
    }, 'Delete'),
  );
  detail.appendChild(actions);

  return detail;
}

function renderHistoryStats(h) {
  const grid = el('div', { class: 'stat-grid' });
  const runCount = h.run_count ?? h.runs ?? h.total ?? 0;
  const passRate = h.pass_rate != null ? `${Math.round(parseFloat(h.pass_rate) * 100)}%` : '—';
  const avgDur   = h.average_duration_minutes ?? h.avg_duration_minutes ?? h.average_duration ?? null;
  grid.appendChild(statBlock('Runs',         String(runCount)));
  grid.appendChild(statBlock('Pass rate',    passRate));
  grid.appendChild(statBlock('Avg duration', avgDur != null ? fmtMinutes(avgDur) : '—'));
  return grid;
}

async function loadAvailabilityForVisible(envs) {
  const work = envs.map(async (e) => {
    if (state.availability.has(e.id)) return;
    try {
      const raw = await fetchAvailability(e.id);
      state.availability.set(e.id, classifyAvailability(raw));
    } catch {
      state.availability.set(e.id, 'unknown');
    }
    // patch dot in place — avoid full re-render
    const card = document.querySelector(`.env-card[data-id="${e.id}"]`);
    if (!card) return;
    const dot = card.querySelector('.status-foot .dot');
    const lbl = card.querySelector('.status-foot .status-label');
    const av  = state.availability.get(e.id);
    if (dot && lbl) {
      dot.className = 'dot s-' + av;
      lbl.innerHTML = '';
      lbl.appendChild(el('span', { class: 'dot s-' + av }));
      lbl.appendChild(document.createTextNode(' ' + statusLabel(av)));
    }
  });
  await Promise.all(work);
}

// ── Screen: Runs ─────────────────────────────────────────────────────────────

function renderRuns(root) {
  const runs = state.data.testRuns.slice().sort((a, b) => {
    const ax = a.started_at || '';
    const bx = b.started_at || '';
    return bx.localeCompare(ax);
  });

  const filterKey = state.runFilter;
  const needle = state.search.trim().toLowerCase();
  const visible = runs.filter(r => {
    if (filterKey !== 'all' && (r.status || 'pending') !== filterKey) return false;
    if (!needle) return true;
    const env = state.data.testEnvironments.find(e => e.id === r.test_environment_id);
    const envName = env?.name || '';
    return envName.toLowerCase().includes(needle)
        || (r.change_request_id || '').toLowerCase().includes(needle)
        || (r.id || '').toLowerCase().includes(needle);
  });

  updateFooterMeta(
    `${runs.length} ${runs.length === 1 ? 'run' : 'runs'} · ${state.data.testEnvironments.length} environments`
  );

  root.appendChild(el('div', { class: 'section-head' },
    el('div', {},
      el('h1', {}, 'Test run history'),
      el('div', { class: 'meta' }, `${visible.length} of ${runs.length}`),
    ),
  ));

  // Filter pills (rendered as underline tabs — see CSS)
  const filterRow = el('div', { class: 'filter-row' });
  const filters = [['all', 'All'], ...RUN_STATUSES.map(s => [s, s])];
  for (const [key, label] of filters) {
    filterRow.appendChild(el('button', {
      class: 'filter-pill' + (filterKey === key ? ' active' : ''),
      onClick: () => { state.runFilter = key; render(); },
    }, label));
  }
  root.appendChild(filterRow);

  if (visible.length === 0) {
    if (runs.length === 0) {
      root.appendChild(emptyCard({
        title: 'No runs yet',
        lede:  'Every run leaves a trace. None yet.',
      }));
    } else {
      root.appendChild(el('div', { class: 'empty' }, 'No runs match.'));
    }
    return;
  }

  // Pre-compute syncs by target env, by source env
  const syncByTargetEnv = new Map();
  for (const s of state.data.dataSyncs) {
    const t = s.target_env_id;
    if (!t) continue;
    if (!syncByTargetEnv.has(t)) syncByTargetEnv.set(t, []);
    syncByTargetEnv.get(t).push(s);
  }

  const card = el('div', { class: 'card' });
  const table = el('table', { class: 'table' });
  table.appendChild(el('thead', {}, el('tr', {},
    el('th', {}, 'Environment'),
    el('th', {}, 'Status'),
    el('th', {}, 'Started'),
    el('th', {}, 'Duration'),
    el('th', {}, 'Cost'),
    el('th', {}, 'Change request'),
  )));
  const tbody = el('tbody', {});
  for (const r of visible) {
    const env = state.data.testEnvironments.find(e => e.id === r.test_environment_id);
    const envName = env?.name || '(unset)';
    const status = r.status || 'pending';
    const expanded = state.expandedRunId === r.id;

    const row = el('tr', {
      class: expanded ? 'expanded' : '',
      onClick: () => { state.expandedRunId = expanded ? null : r.id; render(); },
    },
      el('td', {}, envName),
      el('td', {}, el('span', { class: 'badge s-' + status }, status)),
      el('td', {}, r.started_at || '—'),
      el('td', {}, fmtMinutes(r.duration_minutes)),
      el('td', {}, fmtCost(r.cost_actual).replace('/h','')),
      el('td', { class: 'muted' }, r.change_request_id || '—'),
    );
    tbody.appendChild(row);

    if (expanded) {
      const syncs = syncByTargetEnv.get(p.test_environment_id) || [];
      const detailCell = el('td', { colSpan: 6 });

      // Editable run form
      const form = el('div', { class: 'form-grid' });
      const cfg = ENTITIES.testRuns;
      for (const fname of ['status', 'started_at', 'finished_at', 'duration_minutes', 'cost_actual']) {
        const f = cfg.fields.find(x => x.name === fname);
        if (f) form.appendChild(fieldInput(f, r[fname]));
      }
      detailCell.appendChild(form);

      // Sync chips callout
      if (syncs.length) {
        const callout = el('div', { class: 'callout' });
        callout.appendChild(el('span', {}, 'Data sync events: '));
        for (const s of syncs) {
          const srcEnv = s.source_env_id
            ? state.data.testEnvironments.find(e => e.id === s.source_env_id)?.name
            : state.data.dataSources.find(d => d.id === s.source_data_id)?.name;
          const verb = s.kind === 'pull' ? 'pull from' : s.kind === 'push' ? 'push to' : 'shared with';
          callout.appendChild(el('span', { class: 'chip k-' + (s.kind || '') },
            `${verb} ${srcEnv || '?'}`,
          ));
        }
        detailCell.appendChild(callout);
      }

      const actions = el('div', { class: 'row-actions', style: 'margin-top: 10px;' },
        el('button', { class: 'primary',
          onClick: async () => {
            const payload = readForm(form);
            if (r.test_environment_id) payload.test_environment_id = r.test_environment_id;
            try {
              const updated = await updateRecord('testRuns', r.id, payload);
              const idx = state.data.testRuns.findIndex(x => x.id === r.id);
              if (idx !== -1) state.data.testRuns[idx] = updated;
              setStatus('Saved');
              render();
            } catch (err) { setError(err); }
          },
        }, 'Save'),
        el('button', { class: 'danger',
          onClick: async () => {
            if (!confirm('Delete this run?')) return;
            try {
              await deleteRecord('testRuns', r.id);
              state.data.testRuns = state.data.testRuns.filter(x => x.id !== r.id);
              state.expandedRunId = null;
              render();
            } catch (err) { setError(err); }
          },
        }, 'Delete'),
      );
      detailCell.appendChild(actions);

      tbody.appendChild(el('tr', { class: 'detail-row' }, detailCell));
    }
  }
  table.appendChild(tbody);
  card.appendChild(table);
  root.appendChild(card);
}

// ── Screen: Sync dashboard ───────────────────────────────────────────────────

function renderSync(root) {
  const syncCount = state.data.dataSyncs.length;
  updateFooterMeta(
    `${syncCount} ${syncCount === 1 ? 'sync' : 'syncs'} · ${state.data.testEnvironments.length} environments`
  );

  root.appendChild(el('div', { class: 'section-head' },
    el('h1', {}, 'Data sync dashboard'),
  ));

  if (state.data.testEnvironments.length === 0 && syncCount === 0) {
    root.appendChild(emptyCard({
      title: 'No syncs yet',
      lede:  'Data flows down the same edges your code does.',
    }));
    return;
  }

  // Default env
  if (!state.syncEnvId && state.data.testEnvironments.length) {
    state.syncEnvId = state.data.testEnvironments[0].id;
  }

  const grid = el('div', { class: 'sync-grid' });

  // ─── Left panel: chart ──────────────────────────────────────────────
  const leftCard  = el('div', { class: 'card sync-panel' });
  const envSelect = el('select', { onChange: e => { state.syncEnvId = e.target.value; render(); } });
  for (const e of state.data.testEnvironments) {
    const o = el('option', { value: e.id }, e.name || e.id);
    if (e.id === state.syncEnvId) o.selected = true;
    envSelect.appendChild(o);
  }
  leftCard.appendChild(el('div', { class: 'panel-head' },
    el('h2', {}, 'Sync time per run'),
    envSelect,
  ));

  const envId = state.syncEnvId;
  const runs = state.data.testRuns
    .filter(r => r.test_environment_id === envId)
    .slice()
    .sort((a, b) => (a.started_at || '').localeCompare(b.started_at || ''));

  const estTotalForEnv = state.data.dataSyncs
    .filter(s => s.target_env_id === envId)
    .reduce((acc, s) => acc + (parseFloat(s.estimated_minutes) || 0), 0);

  if (!envId || runs.length < 2) {
    leftCard.appendChild(el('div', { class: 'chart-empty' },
      'Need at least 2 runs to chart this environment.'));
  } else {
    leftCard.appendChild(el('div', { class: 'chart-frame' }, buildSvgChart(runs, estTotalForEnv)));
    leftCard.appendChild(el('div', { class: 'chart-legend' },
      el('span', {}, el('span', { class: 'swatch', style: 'background:#111827;' }), 'Run duration (min)'),
      el('span', {}, el('span', { class: 'swatch', style: 'background:#f59e0b; border-top: 1px dashed #f59e0b;' }), 'Est. sync (min, summed)'),
    ));
  }

  // ─── Right panel: slow syncs ────────────────────────────────────────
  const rightCard = el('div', { class: 'card sync-panel' });
  rightCard.appendChild(el('div', { class: 'panel-head' }, el('h2', {}, 'Slow syncs')));

  // Compute averages per env
  const avgDurByEnv = new Map();
  for (const env of state.data.testEnvironments) {
    const envRuns = state.data.testRuns.filter(r =>
      r.test_environment_id === env.id && r.duration_minutes);
    if (!envRuns.length) continue;
    const sum = envRuns.reduce((a, r) => a + (parseFloat(r.duration_minutes) || 0), 0);
    avgDurByEnv.set(env.id, sum / envRuns.length);
  }

  const slow = state.data.dataSyncs.filter(s => {
    const est = parseFloat(s.estimated_minutes) || 0;
    if (est > 60) return true;
    const target = state.data.testEnvironments.find(e => e.id === s.target_env_id);
    if (!target) return false;
    const spin = parseFloat(target.spinup_minutes) || 0;
    const avg  = avgDurByEnv.get(target.id) || 0;
    if (spin > 0 && avg > spin * 2) return true;
    return false;
  });

  if (slow.length === 0) {
    rightCard.appendChild(el('div', { class: 'empty' }, 'No slow syncs.'));
  } else {
    const table = el('table', { class: 'table' });
    table.appendChild(el('thead', {}, el('tr', {},
      el('th', {}, 'Sync'),
      el('th', {}, 'Kind'),
      el('th', {}, 'Est min'),
      el('th', {}, 'Reason'),
    )));
    const tbody = el('tbody', {});
    for (const s of slow) {
      const target = state.data.testEnvironments.find(e => e.id === s.target_env_id);
      const targetName = target?.name || s.target_env_id || '?';
      const srcName = s.source_env_id
        ? state.data.testEnvironments.find(e => e.id === s.source_env_id)?.name
        : state.data.dataSources.find(d => d.id === s.source_data_id)?.name;

      const est = parseFloat(s.estimated_minutes) || 0;
      const spin = parseFloat(target?.spinup_minutes) || 0;
      const avg = avgDurByEnv.get(target?.id) || 0;
      const reasons = [];
      if (est > 60)                             reasons.push(`est ${est}m > 60m`);
      if (spin > 0 && avg > spin * 2)           reasons.push(`avg ${avg.toFixed(1)}m > 2× spinup ${spin}m`);

      tbody.appendChild(el('tr', {
        style: 'background: var(--warn-soft);',
      },
        el('td', {}, `${srcName || '?'} → ${targetName}`),
        el('td', {}, el('span', { class: 'chip k-' + (s.kind || '') }, s.kind || '—')),
        el('td', {}, est ? est.toString() : '—'),
        el('td', { class: 'muted' }, reasons.join(' · ')),
      ));
    }
    table.appendChild(tbody);
    rightCard.appendChild(table);
  }

  grid.appendChild(leftCard);
  grid.appendChild(rightCard);
  root.appendChild(grid);
}

// ── SVG line chart (vanilla, no library) ─────────────────────────────────────

function buildSvgChart(runs, estimatedSum) {
  const W = 640, H = 280;
  const M = { top: 20, right: 16, bottom: 36, left: 44 };
  const innerW = W - M.left - M.right;
  const innerH = H - M.top  - M.bottom;

  const durations = runs.map(r => parseFloat(r.duration_minutes) || 0);
  const xs = runs.map((_, i) => i);
  const yMaxRaw = Math.max(...durations, estimatedSum, 1);
  const yMax = niceCeil(yMaxRaw);

  const xScale = i => M.left + (xs.length === 1 ? innerW / 2 : (i / (xs.length - 1)) * innerW);
  const yScale = v => M.top + innerH - (v / yMax) * innerH;

  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('viewBox', `0 0 ${W} ${H}`);
  svg.setAttribute('role', 'img');
  svg.setAttribute('aria-label', 'Sync time per run');

  const ns = 'http://www.w3.org/2000/svg';
  const make = (t, attrs = {}, txt) => {
    const n = document.createElementNS(ns, t);
    for (const [k, v] of Object.entries(attrs)) n.setAttribute(k, v);
    if (txt != null) n.textContent = txt;
    return n;
  };

  // Plot frame (axes)
  svg.appendChild(make('line', {
    x1: M.left, y1: M.top + innerH,
    x2: M.left + innerW, y2: M.top + innerH,
    stroke: '#9ca3af', 'stroke-width': 1,
  }));
  svg.appendChild(make('line', {
    x1: M.left, y1: M.top,
    x2: M.left, y2: M.top + innerH,
    stroke: '#9ca3af', 'stroke-width': 1,
  }));

  // Y ticks (5 steps)
  const ticks = 5;
  for (let i = 0; i <= ticks; i++) {
    const v = (yMax * i) / ticks;
    const y = yScale(v);
    svg.appendChild(make('line', {
      x1: M.left - 4, y1: y, x2: M.left + innerW, y2: y,
      stroke: '#e5e7eb', 'stroke-width': 1,
    }));
    svg.appendChild(make('text', {
      x: M.left - 8, y: y + 4,
      'text-anchor': 'end',
      'font-size': 10, fill: '#6b7280',
      'font-family': 'ui-sans-serif,system-ui,sans-serif',
    }, fmtNum(v, 0)));
  }

  // X ticks (every run, capped at 8 labels)
  const stride = Math.max(1, Math.ceil(runs.length / 8));
  for (let i = 0; i < runs.length; i++) {
    const x = xScale(i);
    svg.appendChild(make('line', {
      x1: x, y1: M.top + innerH, x2: x, y2: M.top + innerH + 4,
      stroke: '#9ca3af', 'stroke-width': 1,
    }));
    if (i % stride === 0 || i === runs.length - 1) {
      const lbl = (runs[i].started_at || '').slice(0, 10) || `#${i + 1}`;
      svg.appendChild(make('text', {
        x: x, y: M.top + innerH + 16,
        'text-anchor': 'middle',
        'font-size': 10, fill: '#6b7280',
        'font-family': 'ui-sans-serif,system-ui,sans-serif',
      }, lbl));
    }
  }

  // Axis labels
  svg.appendChild(make('text', {
    x: M.left, y: M.top - 6,
    'font-size': 10, fill: '#6b7280',
    'font-family': 'ui-sans-serif,system-ui,sans-serif',
  }, 'minutes'));
  svg.appendChild(make('text', {
    x: M.left + innerW, y: H - 6,
    'text-anchor': 'end',
    'font-size': 10, fill: '#6b7280',
    'font-family': 'ui-sans-serif,system-ui,sans-serif',
  }, 'run'));

  // Estimated baseline (dashed)
  if (estimatedSum > 0) {
    const yEst = yScale(estimatedSum);
    svg.appendChild(make('line', {
      x1: M.left, y1: yEst, x2: M.left + innerW, y2: yEst,
      stroke: '#f59e0b', 'stroke-width': 1.5, 'stroke-dasharray': '4 3',
    }));
  }

  // Run duration polyline
  const pts = runs.map((_, i) => `${xScale(i)},${yScale(durations[i])}`).join(' ');
  svg.appendChild(make('polyline', {
    points: pts,
    fill: 'none', stroke: '#111827', 'stroke-width': 2,
    'stroke-linejoin': 'round', 'stroke-linecap': 'round',
  }));

  // Run dots
  for (let i = 0; i < runs.length; i++) {
    svg.appendChild(make('circle', {
      cx: xScale(i), cy: yScale(durations[i]),
      r: 3, fill: '#111827',
    }));
  }

  return svg;
}

function niceCeil(v) {
  if (v <= 0) return 1;
  const exp  = Math.pow(10, Math.floor(Math.log10(v)));
  const frac = v / exp;
  let nice;
  if      (frac <= 1)  nice = 1;
  else if (frac <= 2)  nice = 2;
  else if (frac <= 5)  nice = 5;
  else                 nice = 10;
  return nice * exp;
}

// ── Screen: Settings (simple CRUD) ───────────────────────────────────────────

const SETTINGS_EMPTY = {
  testInfrastructures: { title: 'No infrastructure yet',  lede: 'The substrate beneath every environment.' },
  mockSources:         { title: 'No mock sources yet',    lede: 'Pretend, until production catches up.' },
  dataSources:         { title: 'No data sources yet',    lede: 'Where test data comes from — and where it goes stale.' },
  testSuites:          { title: 'No test suites yet',     lede: 'Tests are stories about deployables. Catalogue them.' },
};

function renderSettings(root, entityKey) {
  const cfg = ENTITIES[entityKey];
  if (!cfg) {
    root.appendChild(el('div', { class: 'empty' }, 'Unknown settings page.'));
    return;
  }
  const items = state.data[entityKey] || [];
  const needle = state.search.trim().toLowerCase();
  const visible = needle
    ? items.filter(i => JSON.stringify(i).toLowerCase().includes(needle))
    : items;

  const labelPlural = cfg.label.toLowerCase() + 's';
  updateFooterMeta(`${items.length} ${items.length === 1 ? cfg.label.toLowerCase() : labelPlural}`);

  root.appendChild(el('div', { class: 'section-head' },
    el('div', {},
      el('h1', {}, cfg.label + 's'),
      el('div', { class: 'meta' }, `${visible.length} of ${items.length}`),
    ),
    el('button', { class: 'primary', onClick: () => openNewModal(entityKey) }, `New ${cfg.label.toLowerCase()}`),
  ));

  if (visible.length === 0) {
    if (items.length === 0) {
      const tmpl = SETTINGS_EMPTY[entityKey] || {
        title: `No ${labelPlural} yet`,
        lede:  'Nothing here yet.',
      };
      root.appendChild(emptyCard(tmpl));
    } else {
      root.appendChild(el('div', { class: 'empty' }, 'No matches.'));
    }
    return;
  }

  const card = el('div', { class: 'card' });
  const list = el('div', { class: 'settings-list' });
  for (const item of visible) {
    const id = item.id;
    const expanded = state.expandedSettingId === id;

    if (!expanded) {
      list.appendChild(el('div', {
        class: 'row',
        onClick: () => { state.expandedSettingId = id; render(); },
      },
        el('div', {},
          el('div', {}, cfg.rowLabel(item, state.data)),
          el('div', { class: 'meta' }, cfg.rowMeta(item, state.data)),
        ),
        el('button', { class: 'ghost' }, 'Edit'),
      ));
    } else {
      const form = el('div', { class: 'form-grid' });
      for (const f of cfg.fields) form.appendChild(fieldInput(f, item[f.name]));

      const wrap = el('div', { class: 'row expanded' },
        el('div', {},
          el('div', { style: 'font-weight:600; margin-bottom: 8px;' }, cfg.rowLabel(item, state.data)),
          form,
          el('div', { class: 'row-actions', style: 'margin-top:12px;' },
            el('button', { class: 'primary',
              onClick: async () => {
                const payload = readForm(form);
                try {
                  const updated = await updateRecord(entityKey, id, payload);
                  const idx = state.data[entityKey].findIndex(x => x.id === id);
                  if (idx !== -1) state.data[entityKey][idx] = updated;
                  state.expandedSettingId = null;
                  setStatus('Saved');
                  render();
                } catch (err) { setError(err); }
              },
            }, 'Save'),
            el('button', {
              onClick: () => { state.expandedSettingId = null; render(); },
            }, 'Cancel'),
            el('button', { class: 'danger',
              onClick: async () => {
                if (!confirm(`Delete this ${cfg.label.toLowerCase()}?`)) return;
                try {
                  await deleteRecord(entityKey, id);
                  state.data[entityKey] = state.data[entityKey].filter(x => x.id !== id);
                  state.expandedSettingId = null;
                  render();
                } catch (err) { setError(err); }
              },
            }, 'Delete'),
          ),
        ),
      );
      list.appendChild(wrap);
    }
  }
  card.appendChild(list);
  root.appendChild(card);
}

// ── New-button helper (entity per screen) ────────────────────────────────────

function newEntityKeyForCurrentScreen() {
  if (state.screen === 'environments') return 'testEnvironments';
  if (state.screen === 'runs')         return 'testRuns';
  if (state.screen === 'sync')         return 'dataSyncs';
  if (state.screen.startsWith('settings:')) return state.screen.slice('settings:'.length);
  return 'testEnvironments';
}

// ── Wire-up ──────────────────────────────────────────────────────────────────

function isValidScreen(name) {
  if (!name) return false;
  return Array.from(document.querySelectorAll('[data-screen]'))
    .some(el => el.dataset.screen === name);
}

function setScreen(screen) {
  state.screen = screen;
  state.search = '';
  $('#search').value = '';
  state.expandedEnvId = null;
  state.expandedRunId = null;
  state.expandedSettingId = null;
  closeMenu();
  render();

  if (location.hash.slice(1) !== screen) {
    location.hash = screen;
  }
}

function initHashRouting() {
  window.addEventListener('hashchange', () => {
    const key = location.hash.slice(1);
    if (isValidScreen(key) && key !== state.screen) {
      setScreen(key);
    }
  });
  const initial = location.hash.slice(1);
  if (isValidScreen(initial)) {
    state.screen = initial;
  } else {
    location.replace('#' + state.screen);
  }
}

function openMenu()  { $('#settings-menu').classList.add('open'); }
function closeMenu() { $('#settings-menu').classList.remove('open'); }
function toggleMenu(){ $('#settings-menu').classList.toggle('open'); }

function bindUI() {
  $$('#primary-nav .tab').forEach(tab => {
    tab.addEventListener('click', () => setScreen(tab.dataset.screen));
  });

  $('#btn-new').addEventListener('click', () => openNewModal(newEntityKeyForCurrentScreen()));

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

  $('#modal-cancel').addEventListener('click', closeModal);
  $('#modal-save').addEventListener('click', saveNewModal);
  $('#modal-root').addEventListener('click', (e) => {
    if (e.target.id === 'modal-root') closeModal();
  });

  document.addEventListener('keydown', (e) => {
    const tag = document.activeElement?.tagName?.toLowerCase();
    const inField = ['input', 'textarea', 'select'].includes(tag);

    if (e.key === 'Escape') {
      if (state.modal.open)              { closeModal(); return; }
      if (state.expandedEnvId)           { state.expandedEnvId = null; render(); return; }
      if (state.expandedRunId)           { state.expandedRunId = null; render(); return; }
      if (state.expandedSettingId)       { state.expandedSettingId = null; render(); return; }
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
    if (e.key === 'n' && !inField && !state.modal.open) {
      e.preventDefault();
      openNewModal(newEntityKeyForCurrentScreen());
      return;
    }
    if (e.key === 'Enter' && state.modal.open && inField && tag !== 'textarea') {
      e.preventDefault();
      saveNewModal();
      return;
    }
  });
}

async function init() {
  bindUI();
  initHashRouting();
  state.loading = true;
  setStatus('Loading…', 'info', { sticky: true });
  try {
    await loadAll();
    setStatus('');
  } catch (err) {
    setError(err);
  } finally {
    state.loading = false;
  }
  render();
}

init();
