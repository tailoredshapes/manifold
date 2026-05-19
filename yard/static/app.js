// yard — test coordinator (environments, runs, sync, settings)
// Vanilla JS ES module · no build step · no framework · no charting library.
//
// Primitives (el, esc, apiFetch, gqlQuery, modal/status helpers, crossLink,
// fieldInput, readForm) come from the shared Manifold UI kit.

import {
  $, $$, el, esc,
  apiFetch, gqlQuery,
  loadManifoldConfig, getManifoldConfig, crossLink,
  setStatus, setError, updateFooterMeta,
  emptyCard, fieldInput, readForm,
  openModal,
} from '/static/manifold-ui.js';

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

// ── Domain types ─────────────────────────────────────────────────────────────

/**
 * @typedef {object} DeployableRef
 * @property {string} [id]
 * @property {string} [name]
 *
 * @typedef {object} TestEnvironment
 * @property {string} id
 * @property {string} [name]
 * @property {string} [kind]
 * @property {string} [deployable_id]
 * @property {string} [service_id]
 * @property {string} [infrastructure_id]
 * @property {string} [mock_source_id]
 * @property {string} [cost_per_hour]
 * @property {string} [spinup_minutes]
 * @property {string} [teardown_policy]
 * @property {string} [max_duration_minutes]
 * @property {string} [concurrency_limit]
 * @property {string} [rate_limit]
 * @property {string} [contractual_limit]
 * @property {string} [notes]
 * @property {DeployableRef} [deployable]
 *
 * @typedef {object} TestInfrastructure
 * @property {string} id
 * @property {string} [name]
 * @property {string} [provider]
 * @property {string} [region]
 * @property {string} [instance_type]
 * @property {string} [cost_per_hour]
 * @property {string} [notes]
 *
 * @typedef {object} MockSource
 * @property {string} id
 * @property {string} [name]
 * @property {string} [repo_url]
 * @property {string} [path]
 * @property {string} [language]
 * @property {string} [notes]
 *
 * @typedef {object} DataSource
 * @property {string} id
 * @property {string} [name]
 * @property {string} [kind]
 * @property {string} [location]
 * @property {string} [refresh_policy]
 * @property {string} [notes]
 *
 * @typedef {object} DataSync
 * @property {string} id
 * @property {string} [kind]
 * @property {string} target_env_id
 * @property {string} [source_env_id]
 * @property {string} [source_data_id]
 * @property {string} [refresh_policy]
 * @property {string} [estimated_minutes]
 * @property {string} [notes]
 *
 * @typedef {object} TestRun
 * @property {string} id
 * @property {string} test_environment_id
 * @property {string} [change_request_id]
 * @property {string} [test_suite_id]
 * @property {string} [team_id]
 * @property {string} [started_at]
 * @property {string} [finished_at]
 * @property {string} [status]
 * @property {string} [duration_minutes]
 * @property {string} [cost_actual]
 *
 * @typedef {object} TestSuite
 * @property {string} id
 * @property {string} [name]
 * @property {string} [deployable_id]
 * @property {string} [runner]
 * @property {string} [command]
 * @property {string} [description]
 *
 * @typedef {object} SyncRun
 * @property {string} id
 * @property {string} data_sync_id
 * @property {string} [source_env_id]
 * @property {string} target_env_id
 * @property {string} [source_data_id]
 * @property {string} [masking_summary]
 * @property {string} [source_revision]
 * @property {string} [triggered_by]
 * @property {string} [started_at]
 * @property {string} [finished_at]
 * @property {string} [status]
 * @property {string} [duration_minutes]
 * @property {string} [error_message]
 *
 * @typedef {object} ResetStep
 * @property {number} order
 * @property {string} data_sync_id
 * @property {string} source_label
 * @property {string} target_env_id
 * @property {string} target_env_name
 * @property {string} kind
 * @property {string} [refresh_policy]
 * @property {number} estimated_minutes
 * @property {number} estimated_cost
 * @property {string} [masking_summary]
 * @property {number[]} predecessor_orders
 *
 * @typedef {object} ResetBlocker
 * @property {string} kind
 * @property {string} message
 * @property {string[]} references
 *
 * @typedef {object} ResetPlan
 * @property {string} target_env_id
 * @property {string} target_env_name
 * @property {string} computed_at
 * @property {string | null} [last_sync_at]
 * @property {number} estimated_total_minutes
 * @property {number} estimated_total_cost
 * @property {ResetStep[]} steps
 * @property {ResetBlocker[]} blockers
 *
 * @typedef {{ status: 'available' | 'cap' | 'blocked' | 'unknown' | string, raw?: any }} AvailabilityClassification
 * @typedef {'available' | 'cap' | 'blocked' | 'unknown'} AvailabilityState
 */

// ── State ────────────────────────────────────────────────────────────────────

const state = {
  screen: 'environments',                  // environments | runs | sync | lifecycle | settings:<entity>
  data: {
    /** @type {TestEnvironment[]} */    testEnvironments: [],
    /** @type {TestInfrastructure[]} */ testInfrastructures: [],
    /** @type {MockSource[]} */         mockSources: [],
    /** @type {DataSource[]} */         dataSources: [],
    /** @type {DataSync[]} */           dataSyncs: [],
    /** @type {TestRun[]} */            testRuns: [],
    /** @type {TestSuite[]} */          testSuites: [],
    /** @type {SyncRun[]} */            syncRuns: [],
  },
  // Cross-app config now lives inside manifold-ui (loadManifoldConfig).
  /** @type {Map<string, AvailabilityState>} */
  availability: new Map(),                 // env id → status bucket
  /** @type {Map<string, any>} */
  history:      new Map(),                 // env id → history payload
  /** @type {string | null} */
  expandedEnvId: null,
  /** @type {string | null} */
  expandedRunId: null,
  /** @type {string | null} */
  expandedSettingId: null,
  /** @type {string | null} */
  expandedPlanEnvId: null,                 // Lifecycle: which env's reset-plan panel is open
  /** @type {Map<string, ResetPlan>} */
  resetPlans: new Map(),                   // env id → ResetPlan (fetched lazily)
  // Cross-app cache of groundwork data needed for the per-env dependency
  // graph. Fetched once on demand the first time an env-card is expanded.
  gwGraph: {
    loaded: false,
    /** @type {Array<{id: string, name?: string}>} */ deployables: [],
    /** @type {Array<{id: string, name?: string, type?: string}>} */ services: [],
    /** @type {Array<{id: string, deployable_id: string, service_id: string}>} */ exposes: [],
    /** @type {Array<{id: string, deployable_id: string, service_id: string, criticality?: string}>} */ dependencies: [],
  },
  /** @type {Map<string, any>} — env id → cytoscape instance, tracked so re-renders don't leak listeners */
  envDepCy: new Map(),
  runFilter: 'all',                        // all | <RUN_STATUS>
  /** @type {string | null} */
  syncEnvId: null,
  search: '',
  loading: false,
  // Modal is fully promise-driven via manifold-ui openModal() — no
  // open/close state to track here.
};

// ── Data load ────────────────────────────────────────────────────────────────

/** @returns {Promise<void>} */
async function loadAll() {
  const [
    testEnvironments,
    testInfrastructures,
    mockSources,
    dataSources,
    dataSyncs,
    testRuns,
    testSuites,
    syncRuns,
  ] = await Promise.all([
    gqlQuery(
      '/test_environment/graph',
      '{ getAll { id name kind deployable_id service_id infrastructure_id mock_source_id cost_per_hour spinup_minutes teardown_policy max_duration_minutes concurrency_limit rate_limit contractual_limit notes deployable { id name } } }'
    ).then(d => d.getAll).catch(() => []),
    gqlQuery(
      '/test_infrastructure/graph',
      '{ getAll { id name provider region instance_type cost_per_hour notes } }'
    ).then(d => d.getAll).catch(() => []),
    gqlQuery(
      '/mock_source/graph',
      '{ getAll { id name repo_url path language notes } }'
    ).then(d => d.getAll).catch(() => []),
    gqlQuery(
      '/data_source/graph',
      '{ getAll { id name kind location refresh_policy notes } }'
    ).then(d => d.getAll).catch(() => []),
    gqlQuery(
      '/data_sync/graph',
      '{ getAll { id kind target_env_id source_env_id source_data_id refresh_policy estimated_minutes notes } }'
    ).then(d => d.getAll).catch(() => []),
    gqlQuery(
      '/test_run/graph',
      '{ getAll { id test_environment_id change_request_id test_suite_id team_id started_at finished_at status duration_minutes cost_actual } }'
    ).then(d => d.getAll).catch(() => []),
    gqlQuery(
      '/test_suite/graph',
      '{ getAll { id name deployable_id runner command description } }'
    ).then(d => d.getAll).catch(() => []),
    gqlQuery(
      '/sync_run/graph',
      '{ getAll { id data_sync_id source_env_id target_env_id source_data_id masking_summary source_revision triggered_by started_at finished_at status duration_minutes error_message } }'
    ).then(d => d.getAll).catch(() => []),
  ]);
  state.data.testEnvironments    = Array.isArray(testEnvironments) ? testEnvironments : [];
  state.data.testInfrastructures = Array.isArray(testInfrastructures) ? testInfrastructures : [];
  state.data.mockSources         = Array.isArray(mockSources) ? mockSources : [];
  state.data.dataSources         = Array.isArray(dataSources) ? dataSources : [];
  state.data.dataSyncs           = Array.isArray(dataSyncs) ? dataSyncs : [];
  state.data.testRuns            = Array.isArray(testRuns) ? testRuns : [];
  state.data.testSuites          = Array.isArray(testSuites) ? testSuites : [];
  state.data.syncRuns            = Array.isArray(syncRuns) ? syncRuns : [];
}

/** @param {string} key @param {Record<string, any>} payload @returns {Promise<any>} */
async function createRecord(key, payload) {
  return apiFetch(ENTITIES[key].api, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
}

/** @param {string} key @param {string} id @param {Record<string, any>} payload @returns {Promise<any>} */
async function updateRecord(key, id, payload) {
  return apiFetch(`${ENTITIES[key].api}/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
}

/** @param {string} key @param {string} id @returns {Promise<any>} */
async function deleteRecord(key, id) {
  return apiFetch(`${ENTITIES[key].api}/${id}`, { method: 'DELETE' });
}

/** @param {string} envId @returns {Promise<any>} */
async function fetchAvailability(envId) {
  return apiFetch(`/test_environment/${envId}/availability`);
}

/** @param {string} envId @returns {Promise<any>} */
async function fetchHistory(envId) {
  return apiFetch(`/test_environment/${envId}/history`);
}

// ── Availability classification ──────────────────────────────────────────────

/** @param {any} raw @returns {AvailabilityState} */
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

/** @param {string} s @returns {string} */
function statusLabel(s) {
  switch (s) {
    case 'available': return 'available';
    case 'cap':       return 'at concurrency cap';
    case 'blocked':   return 'at contractual cap';
    default:          return 'unknown';
  }
}

// ── Formatting ───────────────────────────────────────────────────────────────

/** @param {string | number | null | undefined} v @returns {string} */
function fmtCost(v) {
  if (v == null || v === '') return '—';
  const n = parseFloat(String(v));
  if (Number.isFinite(n)) return `$${n.toFixed(2)}/h`;
  return String(v);
}

/** @param {string | number | null | undefined} v @returns {string} */
function fmtMinutes(v) {
  if (v == null || v === '') return '—';
  return `${v} min`;
}

/** @param {string | number | null | undefined} v @param {number} [digits] @returns {string} */
function fmtNum(v, digits = 2) {
  if (v == null || v === '') return '—';
  const n = parseFloat(String(v));
  if (Number.isFinite(n)) return n.toFixed(digits);
  return String(v);
}

// ── Field renderer (inline forms) ────────────────────────────────────────────
//
// fieldInput + readForm come from manifold-ui. We pass a lookup callback
// for `ref` fields so the shared module can render <select> options from
// yard's own state.data without needing to know about it.

const refLookup = (refKey) => state.data[refKey] || [];

const yardFieldInput = (field, value) => fieldInput(field, value, refLookup);

// ── New-record flow ──────────────────────────────────────────────────────────
//
// Single async function — openModal() resolves with the captured payload
// (or null on cancel/Esc/backdrop). No state to track, no separate save
// handler, no scaffold in markup.

/** @param {string} entityKey */
async function createNew(entityKey) {
  const cfg = ENTITIES[entityKey];
  if (!cfg) return;
  const payload = await openModal({
    title: `New ${cfg.label.toLowerCase()}`,
    fields: cfg.fields,
    lookupRef: refLookup,
    submit: 'Create',
  });
  if (!payload) return;
  try {
    await createRecord(entityKey, payload);
    // REST writes return only local fields; re-read via /graph so the new
    // row reflects the same shape the rest of the app already uses.
    await loadAll();
    setStatus(`${cfg.label} created`);
    render();
  } catch (e) {
    setError(e);
  }
}

// ── Screens: dispatcher ──────────────────────────────────────────────────────

function render() {
  const root = /** @type {HTMLElement} */ ($('#screen-root'));
  root.innerHTML = '';
  $$('#primary-nav .tab').forEach(t => {
    // Tabs become "active" for both top-nav screens and any settings:* sub-screen.
    // For settings:* sub-screens, none of the top tabs should be highlighted.
    t.classList.toggle('active', t.dataset.screen === state.screen);
  });
  // Default footer meta — individual screens overwrite below.
  updateFooterMeta('');

  if (state.screen === 'environments') {
    // When an env id is set (via URL or card click), render the dedicated
    // detail page instead of the catalog. URL stays `#environments/<envId>`
    // so shareable links keep working.
    if (state.expandedEnvId) renderEnvDetail(root, state.expandedEnvId);
    else                     renderEnvironments(root);
  }
  else if (state.screen === 'runs')              renderRuns(root);
  else if (state.screen === 'sync')              renderSync(root);
  else if (state.screen === 'lifecycle')         renderLifecycle(root);
  else if (state.screen.startsWith('settings:')) renderSettings(root, state.screen.slice('settings:'.length));
  else                                           renderEnvironments(root);

  // URL mirrors state — keeps shareable links in lockstep with what
  // the user is actually looking at.
  syncUrl();
}

// ── Screen: Environments ─────────────────────────────────────────────────────

/** @param {HTMLElement} root */
function renderEnvironments(root) {
  const envs = state.data.testEnvironments;
  const needle = state.search.trim().toLowerCase();
  const visible = needle
    ? envs.filter(e => (e.name || '').toLowerCase().includes(needle)
                    || (e.kind || '').toLowerCase().includes(needle)
                    || (e.deployable?.name || '').toLowerCase().includes(needle))
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

  // Group by deployable (the "product" each env tests). Envs whose
  // federated deployable lookup didn't resolve — either no deployable_id, or
  // Groundwork didn't return a name — bucket into "Unassigned" so the view
  // stays coherent even if /graph federation is partially down.
  const UNASSIGNED = '— Unassigned';
  const groups = new Map();
  for (const env of visible) {
    const key = env.deployable?.name || UNASSIGNED;
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(env);
  }

  // Named groups alphabetically, Unassigned last.
  const sortedKeys = [...groups.keys()].sort((a, b) => {
    if (a === UNASSIGNED) return 1;
    if (b === UNASSIGNED) return -1;
    return a.localeCompare(b);
  });

  for (const key of sortedKeys) {
    const items = groups.get(key);
    const headingId = `product-${key.replace(/[^a-z0-9]+/gi, '-').toLowerCase()}`;
    const section = el('section', {
      class: 'product-group',
      'aria-labelledby': headingId,
    });
    section.appendChild(
      el('h3', { class: 'product-head', id: headingId },
        el('span', { class: 'product-name' }, key),
        el('span', { class: 'product-count' },
          `${items.length} ${items.length === 1 ? 'environment' : 'environments'}`),
      ),
    );
    const grid = el('div', { class: 'card-grid' });
    for (const item of items) grid.appendChild(buildEnvCard(item));
    section.appendChild(grid);
    root.appendChild(section);
  }

  // lazily fetch availability for each card after render
  queueMicrotask(() => loadAvailabilityForVisible(visible));
}

/** @param {string | null | undefined} kind @returns {string} */
function pillKindClass(kind) {
  const k = (kind || '').replace(/[^a-z-]/gi, '').toLowerCase();
  return 'pill k-' + (k || 'unknown');
}

/** @param {TestEnvironment} item @returns {HTMLElement} */
function buildEnvCard(item) {
  const id = item.id;

  const card = el('div', {
    class: 'card env-card',
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

  // Cross-app deeplink: associated deployable lives in Groundwork.
  if (item.deployable_id) {
    const depRow = el('div', { class: 'constraints' });
    depRow.innerHTML = 'deployable: ' + crossLink('groundwork', 'deployables', item.deployable_id, item.deployable_id.slice(0, 8));
    // Anchor inside card shouldn't trigger detail navigation.
    depRow.addEventListener('click', e => e.stopPropagation());
    card.appendChild(depRow);
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

  // Click → navigate to the env detail page. Anchors inside the card
  // (cross-app deeplinks etc.) call e.stopPropagation() so they don't trigger.
  card.addEventListener('click', () => {
    state.expandedEnvId = id;
    render();
  });

  return card;
}

/** @param {string} label @param {string} value @returns {HTMLElement} */
function statBlock(label, value) {
  return el('div', { class: 'stat' },
    el('div', { class: 'lbl' }, label),
    el('div', { class: 'val' }, value),
  );
}

// Dedicated detail page for a single test environment. Sections, in order:
// header (name + kind pill + status), inline editable fields, dependency
// graph (the mock/test/prod colored neighborhood — full width now that the
// tile constraint is gone), run history rollup + recent runs on this env,
// reset-plan summary (last refresh + freshness chip + jump to Lifecycle).
//
// Hash format is the same `#environments/<envId>` we already encode for
// shareable URLs; the render dispatcher routes here when expandedEnvId is set.
/** @param {HTMLElement} root @param {string} envId */
function renderEnvDetail(root, envId) {
  const item = state.data.testEnvironments.find(e => e.id === envId);
  if (!item) {
    root.innerHTML = '';
    root.appendChild(el('a', { href: '#environments', class: 'back-link' }, 'Environments'));
    root.appendChild(el('div', { class: 'empty' }, 'Environment not found.'));
    return;
  }

  updateFooterMeta(`${item.name || item.id}`);

  // ── Back link ──
  root.appendChild(el('a', { href: '#environments', class: 'back-link' }, 'Environments'));

  // ── Header ──
  const av = state.availability.get(envId) || 'unknown';
  const header = el('div', { class: 'detail-header' });
  const titleBlock = el('div', { class: 'title-block' },
    el('h1', {}, item.name || '(unnamed)'),
    el('div', { class: 'meta' },
      el('span', { class: pillKindClass(item.kind) }, item.kind || 'unknown'),
      el('span', { class: 'meta-sep', 'aria-hidden': 'true' }, '·'),
      el('span', { class: 'status-label' },
        el('span', { class: 'dot s-' + av }),
        statusLabel(av),
      ),
      el('span', { class: 'meta-sep', 'aria-hidden': 'true' }, '·'),
      el('span', { class: 'id' }, item.id.slice(0, 8)),
    ),
  );
  header.appendChild(titleBlock);
  root.appendChild(header);

  // ── Editable fields section ──
  const cfg = ENTITIES.testEnvironments;
  const editable = ['kind', 'cost_per_hour', 'spinup_minutes', 'teardown_policy',
                    'max_duration_minutes', 'concurrency_limit', 'rate_limit',
                    'contractual_limit', 'notes'];
  const form = el('div', { class: 'form-grid' });
  for (const fname of editable) {
    const f = cfg.fields.find(x => x.name === fname);
    if (!f) continue;
    form.appendChild(yardFieldInput(f, item[fname]));
  }
  const fieldsSection = el('section', { class: 'detail-section' },
    el('h2', {}, 'Fields'),
    form,
    el('div', { class: 'row-actions' },
      el('button', { class: 'primary',
        onClick: async () => {
          const payload = readForm(form);
          if (item.name) payload.name = item.name;
          try {
            await updateRecord('testEnvironments', envId, payload);
            await loadAll();
            setStatus('Saved');
            render();
          } catch (err) { setError(err); }
        },
      }, 'Save'),
      el('button', { class: 'danger',
        onClick: async () => {
          if (!confirm(`Delete environment "${item.name || envId}"?`)) return;
          try {
            await deleteRecord('testEnvironments', envId);
            await loadAll();
            state.expandedEnvId = null;
            setStatus('Deleted');
            setScreen('environments');
          } catch (err) { setError(err); }
        },
      }, 'Delete'),
    ),
  );
  root.appendChild(fieldsSection);

  // ── Dependency graph (mock/test/prod colored) ──
  const depBlock = el('div', { class: 'env-dep-graph env-dep-graph--full' });
  const depCanvas = el('div', { class: 'env-dep-canvas', id: `env-dep-${envId}` });
  const depLegend = el('div', { class: 'env-dep-legend' },
    el('span', {}, el('span', { class: 'swatch focal' }), 'this env'),
    el('span', {}, el('span', { class: 'swatch real' }), 'external / real'),
    el('span', {}, el('span', { class: 'swatch test' }), 'sandbox / isolated / multi-tenant'),
    el('span', {}, el('span', { class: 'swatch fake' }), 'mock / stub / missing'),
  );
  depBlock.appendChild(depCanvas);
  depBlock.appendChild(depLegend);
  root.appendChild(el('section', { class: 'detail-section' },
    el('h2', {}, 'Dependencies'),
    depBlock,
  ));
  // Canvas needs to be in the DOM before cytoscape mounts.
  requestAnimationFrame(() => renderEnvDepGraph(item));

  // ── Run history + recent runs section ──
  const historySection = el('section', { class: 'detail-section' },
    el('h2', {}, 'Run history'),
  );
  const histSlot = el('div', { class: 'history-block' });
  const cached = state.history.get(envId);
  if (cached) {
    histSlot.appendChild(renderHistoryStats(cached));
  } else {
    histSlot.appendChild(el('div', { class: 'lede' }, 'Loading…'));
    fetchHistory(envId).then(h => {
      state.history.set(envId, h);
      if (state.expandedEnvId === envId) render();
    }).catch(() => {
      histSlot.innerHTML = '';
      histSlot.appendChild(el('div', { class: 'lede' }, 'No run history available.'));
    });
  }
  historySection.appendChild(histSlot);

  // Recent runs table — last 10 finished, newest first
  const envRuns = state.data.testRuns
    .filter(r => r.test_environment_id === envId)
    .slice()
    .sort((a, b) => (b.started_at || '').localeCompare(a.started_at || ''))
    .slice(0, 10);
  if (envRuns.length > 0) {
    historySection.appendChild(el('h3', { class: 'subsection' }, `Recent runs · ${envRuns.length}`));
    const list = document.createElement('div');
    list.className = 'relationship-list';
    for (const r of envRuns) {
      const row = el('div', { class: 'row' });
      row.innerHTML = `
        <span class="target">
          <a href="#runs/${esc(r.id)}">${esc(r.started_at || r.id.slice(0, 8))}</a>
          <span class="target-meta">${esc(r.test_suite_id || '—')} · ${fmtMinutes(r.duration_minutes)}</span>
        </span>
        <span class="badge s-${esc(r.status || 'pending')}">${esc(r.status || 'pending')}</span>
        <span></span>
      `;
      list.appendChild(row);
    }
    historySection.appendChild(list);
  }
  root.appendChild(historySection);

  // ── Reset-plan summary ──
  const lifecycleSection = el('section', { class: 'detail-section' },
    el('h2', {}, 'Data lifecycle'),
  );
  const latestSync = state.data.syncRuns
    .filter(s => s.status === 'succeeded' && s.target_env_id === envId)
    .reduce((/** @type {SyncRun | undefined} */ best, s) =>
      (!best || (s.finished_at || '') > (best.finished_at || '')) ? s : best,
      undefined);
  const feedingSyncs = state.data.dataSyncs.filter(s => s.target_env_id === envId);
  const fr = computeFreshness(latestSync, feedingSyncs);
  const lifecycleRow = el('div', { class: 'lifecycle-summary' },
    el('div', { class: 'lifecycle-summary-row' },
      el('span', { class: 'lifecycle-label' }, 'Last refresh:'),
      el('span', {}, latestSync?.finished_at ? fmtTimestamp(latestSync.finished_at) : '—'),
    ),
    el('div', { class: 'lifecycle-summary-row' },
      el('span', { class: 'lifecycle-label' }, 'Freshness:'),
      el('span', { class: 'freshness ' + fr.state }, fr.label),
    ),
    el('div', { class: 'lifecycle-summary-row' },
      el('span', { class: 'lifecycle-label' }, 'Feeding syncs:'),
      el('span', {}, `${feedingSyncs.length} ${feedingSyncs.length === 1 ? 'sync' : 'syncs'}`),
    ),
  );
  lifecycleSection.appendChild(lifecycleRow);
  lifecycleSection.appendChild(el('div', { class: 'row-actions', style: 'margin-top: 12px' },
    el('a', { href: `#lifecycle/${envId}`, class: 'primary plan-link' }, 'Plan reset →'),
  ));
  root.appendChild(lifecycleSection);
}

/** @param {TestEnvironment} item @returns {HTMLElement} */
function buildEnvDetail(item) {
  // Legacy inline-detail builder kept for the Settings:testEnvironments admin
  // view, where the in-place edit-on-expand pattern still applies. The
  // environments tab no longer uses this — clicking a card navigates to
  // renderEnvDetail() above.
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
    form.appendChild(yardFieldInput(f, item[fname]));
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

  // Dependency-graph block. The cytoscape canvas is created lazily on the
  // first render; the legend always shows what the colour-coding means so
  // it doesn't need to be inferred from node colours alone.
  const depBlock = el('div', { class: 'env-dep-graph' });
  const depCanvas = el('div', { class: 'env-dep-canvas', id: `env-dep-${id}` });
  const depLegend = el('div', { class: 'env-dep-legend' },
    el('span', {}, el('span', { class: 'swatch focal' }), 'this env'),
    el('span', {}, el('span', { class: 'swatch real' }), 'external / real'),
    el('span', {}, el('span', { class: 'swatch test' }), 'sandbox / isolated / multi-tenant'),
    el('span', {}, el('span', { class: 'swatch fake' }), 'mock / stub / missing'),
  );
  depBlock.appendChild(depCanvas);
  depBlock.appendChild(depLegend);
  detail.appendChild(depBlock);
  // Trigger the (idempotent) fetch + render on next frame so the canvas
  // is in the DOM before cytoscape mounts.
  requestAnimationFrame(() => renderEnvDepGraph(item));

  // Actions
  const actions = el('div', { class: 'row-actions' },
    el('button', { class: 'primary',
      onClick: async () => {
        const payload = readForm(form);
        if (item.name) payload.name = item.name;
        try {
          await updateRecord('testEnvironments', id, payload);
          await loadAll();
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
          await loadAll();
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

/** @param {any} h @returns {HTMLElement} */
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

// ── Per-env dependency graph ────────────────────────────────────────────────
//
// For an env A (whose code is deployable D), walk D's groundwork dependencies
// (each is a service S that some other deployable D' exposes) and find the
// yard environments serving D'. Color the resulting node by the most-real
// kind available across those envs: external (green), test envs (yellow),
// mock/stub (red). When no env serves D', the node is "missing" (also red).
//
// Cross-origin caveat: groundwork.tildarc.com requires auth; production
// browser sessions on yard.tildarc.com don't pass identity headers across
// origins. The fetch is best-effort — failure renders a friendly empty
// state instead of an error. (Dev edge at localhost:8090 injects synthetic
// headers, so the graph works end-to-end there.)

/** @returns {Promise<void>} */
async function ensureGroundworkGraphLoaded() {
  if (state.gwGraph.loaded) return;
  const gwBase = getManifoldConfig()?.groundwork_public_url;
  if (!gwBase) {
    state.gwGraph.loaded = true; // mark loaded to skip retries
    return;
  }
  const base = gwBase.replace(/\/$/, '');
  try {
    const [deps, exps, depls, svcs] = await Promise.all([
      gqlQuery(`${base}/dependency/graph`,
        '{ getAll { id deployable_id service_id criticality } }').then(d => d.getAll || []).catch(() => []),
      gqlQuery(`${base}/exposes/graph`,
        '{ getAll { id deployable_id service_id } }').then(d => d.getAll || []).catch(() => []),
      gqlQuery(`${base}/deployable/graph`,
        '{ getAll { id name } }').then(d => d.getAll || []).catch(() => []),
      gqlQuery(`${base}/service/graph`,
        '{ getAll { id name type } }').then(d => d.getAll || []).catch(() => []),
    ]);
    state.gwGraph = {
      loaded: true,
      dependencies: deps,
      exposes: exps,
      deployables: depls,
      services: svcs,
    };
  } catch {
    // Mark as loaded so we don't keep retrying — the renderer will show
    // an explanatory empty state.
    state.gwGraph.loaded = true;
  }
}

// One of: 'real' | 'test' | 'fake' | 'missing'.
// `real`    → external (the actual prod / third-party thing)
// `test`    → sandbox, isolated, multi-tenant (real code, not prod)
// `fake`    → mock, stub
// `missing` → no env serving the upstream deployable at all
/** @param {string | null | undefined} kind @returns {'real' | 'test' | 'fake'} */
function classifyEnvKind(kind) {
  const k = (kind || '').toLowerCase();
  if (k === 'external')                                       return 'real';
  if (k === 'sandbox' || k === 'isolated' || k === 'multi-tenant') return 'test';
  if (k === 'mock' || k === 'stub')                           return 'fake';
  return 'test'; // unknown kind — better than red
}

// Pick the most-real classification across a set of envs.
/** @param {TestEnvironment[]} envs @returns {'real' | 'test' | 'fake' | 'missing'} */
function bestClassification(envs) {
  if (!envs.length) return 'missing';
  /** @type {'real' | 'test' | 'fake'} */
  let best = 'fake';
  const rank = { fake: 0, test: 1, real: 2 };
  for (const e of envs) {
    const c = classifyEnvKind(e.kind);
    if (rank[c] > rank[best]) best = c;
  }
  return best;
}

const ENV_DEP_STYLE = [
  {
    selector: 'node',
    style: {
      label: 'data(label)',
      'background-color': '#9ca3af',
      'text-valign': 'bottom',
      'text-halign': 'center',
      'text-margin-y': 4,
      'font-size': 10,
      'font-family': 'ui-sans-serif, system-ui, "SF Pro Text", Inter, sans-serif',
      width: 16,
      height: 16,
      color: '#111827',
      'border-width': 1,
      'border-color': '#ffffff',
    },
  },
  { selector: 'node[cls = "focal"]',   style: { 'background-color': '#111827', width: 22, height: 22, 'font-weight': 600 } },
  { selector: 'node[cls = "real"]',    style: { 'background-color': '#16a34a' } },
  { selector: 'node[cls = "test"]',    style: { 'background-color': '#f59e0b' } },
  { selector: 'node[cls = "fake"]',    style: { 'background-color': '#dc2626' } },
  { selector: 'node[cls = "missing"]', style: { 'background-color': '#dc2626', 'border-color': '#dc2626', 'border-style': 'dotted', 'border-width': 2 } },
  {
    selector: 'edge',
    style: {
      width: 1.5,
      'line-color': '#cbd5e1',
      'curve-style': 'bezier',
      'target-arrow-shape': 'triangle',
      'target-arrow-color': '#cbd5e1',
      opacity: 0.85,
    },
  },
];

/** @param {TestEnvironment} env */
async function renderEnvDepGraph(env) {
  const container = document.getElementById(`env-dep-${env.id}`);
  if (!container) return;
  if (typeof cytoscape !== 'function') {
    container.parentElement.innerHTML =
      '<div class="env-dep-empty">cytoscape failed to load</div>';
    return;
  }

  // Tear down any prior instance for this env (re-renders can happen when
  // the user expands → collapses → re-expands the same card).
  const prev = state.envDepCy.get(env.id);
  if (prev) { try { prev.destroy(); } catch { /* noop */ } state.envDepCy.delete(env.id); }

  await ensureGroundworkGraphLoaded();
  const gw = state.gwGraph;

  if (!env.deployable_id || gw.deployables.length === 0) {
    container.parentElement.innerHTML =
      '<div class="env-dep-empty">' +
      (!env.deployable_id
        ? 'This env has no deployable_id — nothing to chart.'
        : 'Groundwork catalog unavailable — can\'t map dependencies right now.') +
      '</div>';
    return;
  }

  const focalDeployable = gw.deployables.find(d => d.id === env.deployable_id);
  const focalLabel = focalDeployable?.name || env.deployable_id.slice(0, 8);

  // For each dep of the focal deployable: find the upstream service, the
  // deployable(s) exposing it, and the yard envs serving those deployables.
  const focalDeps = gw.dependencies.filter(d => d.deployable_id === env.deployable_id);
  const envsByDeployable = new Map();
  for (const e of state.data.testEnvironments) {
    if (!e.deployable_id) continue;
    if (!envsByDeployable.has(e.deployable_id)) envsByDeployable.set(e.deployable_id, []);
    envsByDeployable.get(e.deployable_id).push(e);
  }

  const nodes = [{ data: { id: env.deployable_id, label: focalLabel, cls: 'focal' } }];
  const edges = [];
  const seenUpstream = new Set([env.deployable_id]);

  for (const dep of focalDeps) {
    const exposers = gw.exposes.filter(x => x.service_id === dep.service_id);
    const svcName = gw.services.find(s => s.id === dep.service_id)?.name || 'service';

    if (exposers.length === 0) {
      // No deployable exposes this service. Show a "missing" node anchored
      // to the service name itself.
      const missingId = `missing:${dep.service_id}`;
      if (!seenUpstream.has(missingId)) {
        nodes.push({ data: { id: missingId, label: svcName, cls: 'missing' } });
        seenUpstream.add(missingId);
      }
      edges.push({ data: { id: `e:${env.deployable_id}->${missingId}`, source: env.deployable_id, target: missingId } });
      continue;
    }
    for (const ex of exposers) {
      const upDepId = ex.deployable_id;
      if (upDepId === env.deployable_id) continue; // self-loop
      const upName = gw.deployables.find(d => d.id === upDepId)?.name || upDepId.slice(0, 8);
      const envsForUp = envsByDeployable.get(upDepId) || [];
      const cls = bestClassification(envsForUp);

      if (!seenUpstream.has(upDepId)) {
        nodes.push({ data: { id: upDepId, label: upName, cls } });
        seenUpstream.add(upDepId);
      }
      edges.push({ data: { id: `e:${env.deployable_id}->${upDepId}:${dep.service_id}`, source: env.deployable_id, target: upDepId } });
    }
  }

  if (nodes.length === 1) {
    // Only the focal node — no dependencies registered.
    container.parentElement.innerHTML =
      '<div class="env-dep-empty">' +
      `${esc(focalLabel)} has no registered dependencies in Groundwork.` +
      '</div>';
    return;
  }

  const cy = cytoscape({
    container,
    elements: [...nodes, ...edges],
    style: ENV_DEP_STYLE,
    layout: {
      name: 'cose',
      animate: false,
      fit: true,
      padding: 30,
      randomize: false,
      idealEdgeLength: 90,
      nodeRepulsion: 200000,
      numIter: 600,
    },
    wheelSensitivity: 0.2,
    minZoom: 0.3,
    maxZoom: 3,
  });
  state.envDepCy.set(env.id, cy);

  // Click an upstream node → if the public URL for groundwork is known,
  // open that deployable's detail page (great for "who owns this red dep?").
  cy.on('tap', 'node', evt => {
    const nodeId = evt.target.id();
    if (nodeId === env.deployable_id || nodeId.startsWith('missing:')) return;
    const gwBase = getManifoldConfig()?.groundwork_public_url;
    if (gwBase) {
      window.open(`${gwBase.replace(/\/$/, '')}/#deployable/${encodeURIComponent(nodeId)}`,
        '_blank', 'noopener');
    }
  });
}

/** @param {TestEnvironment[]} envs */
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

/** @param {HTMLElement} root */
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
    el('th', {}, 'Team'),
    el('th', {}, 'Change request'),
  )));
  const tbody = el('tbody', {});
  for (const r of visible) {
    const env = state.data.testEnvironments.find(e => e.id === r.test_environment_id);
    const envName = env?.name || '(unset)';
    const status = r.status || 'pending';
    const expanded = state.expandedRunId === r.id;

    // Cross-app deeplinks: change_request lives in Cityhall; team lives in Union.
    const crCell = r.change_request_id
      ? el('td', { class: 'muted mono', html: crossLink('cityhall', 'changes', r.change_request_id, r.change_request_id.slice(0, 8)) })
      : el('td', { class: 'muted' }, '—');
    const teamCell = r.team_id
      ? el('td', { class: 'muted mono', html: crossLink('union', 'teams', r.team_id, r.team_id.slice(0, 8)) })
      : el('td', { class: 'muted' }, '—');

    const row = el('tr', {
      class: expanded ? 'expanded' : '',
      onClick: (e) => {
        if (e.target.closest('a')) return;
        state.expandedRunId = expanded ? null : r.id;
        render();
      },
    },
      el('td', {}, envName),
      el('td', {}, el('span', { class: 'badge s-' + status }, status)),
      el('td', {}, r.started_at || '—'),
      el('td', {}, fmtMinutes(r.duration_minutes)),
      el('td', {}, fmtCost(r.cost_actual).replace('/h','')),
      teamCell,
      crCell,
    );
    tbody.appendChild(row);

    if (expanded) {
      const syncs = syncByTargetEnv.get(r.test_environment_id) || [];
      const detailCell = el('td', { colSpan: 7 });

      // Editable run form
      const form = el('div', { class: 'form-grid' });
      const cfg = ENTITIES.testRuns;
      for (const fname of ['status', 'started_at', 'finished_at', 'duration_minutes', 'cost_actual']) {
        const f = cfg.fields.find(x => x.name === fname);
        if (f) form.appendChild(yardFieldInput(f, r[fname]));
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
              await updateRecord('testRuns', r.id, payload);
              await loadAll();
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
              await loadAll();
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

/** @param {HTMLElement} root */
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
    const o = /** @type {HTMLOptionElement} */ (el('option', { value: e.id }, e.name || e.id));
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

/**
 * @param {Array<TestRun & { syncMinutes?: number }>} runs
 * @param {number} estimatedSum
 * @returns {SVGSVGElement}
 */
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

/** @param {number} v @returns {number} */
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

// ── Screen: Lifecycle (data governance) ──────────────────────────────────────
//
// Operator-facing dashboard for the data side of every environment. Per env:
// when was it last refreshed (latest succeeded SyncRun to that env), is the
// refresh overdue versus the declared refresh_policy on its feeding sync, and
// what would resetting it cost? The "Plan reset" panel expands inline and
// calls /test_environment/<id>/reset_plan — same shape as cityhall's
// ComputedPlan so the two surfaces feel symmetric.

// Heuristic refresh-policy → expected interval in hours. Used to compare
// against time-since-last-sync to surface staleness. Policies that don't
// map to a wall-clock window (per_test_run, versioned, on_demand) are
// treated as "not applicable" rather than overdue.
const REFRESH_INTERVAL_HOURS = {
  periodic: 24,
};

/** @param {HTMLElement} root */
function renderLifecycle(root) {
  const envs = state.data.testEnvironments;
  const syncs = state.data.dataSyncs;
  const runs = state.data.syncRuns;

  updateFooterMeta(
    `${envs.length} ${envs.length === 1 ? 'environment' : 'environments'} · ${syncs.length} syncs · ${runs.length} sync runs`
  );

  root.appendChild(
    el('div', { class: 'section-head' },
      el('div', {},
        el('h1', {}, 'Data lifecycle'),
        el('div', { class: 'meta' },
          'When was each env last refreshed, what would resetting it cost, what would it break.'),
      ),
    ),
  );

  if (envs.length === 0) {
    root.appendChild(emptyCard({
      title: 'No environments yet',
      lede: 'Lifecycle answers questions about the data inside an environment. Register one first.',
    }));
    return;
  }

  // Pre-compute: per-env latest succeeded SyncRun + its feeding syncs.
  const latestSyncByEnv = new Map();
  for (const r of runs) {
    if (r.status !== 'succeeded') continue;
    if (!r.finished_at) continue;
    const prev = latestSyncByEnv.get(r.target_env_id);
    if (!prev || (r.finished_at || '') > (prev.finished_at || '')) {
      latestSyncByEnv.set(r.target_env_id, r);
    }
  }
  const syncsByTarget = new Map();
  for (const s of syncs) {
    if (!syncsByTarget.has(s.target_env_id)) syncsByTarget.set(s.target_env_id, []);
    syncsByTarget.get(s.target_env_id).push(s);
  }

  const table = el('table', { class: 'lifecycle-table' });
  const thead = el('thead', {},
    el('tr', {},
      el('th', {}, 'Environment'),
      el('th', {}, 'Kind'),
      el('th', {}, 'Last refresh'),
      el('th', {}, 'Freshness'),
      el('th', {}, 'Reset cost'),
      el('th', {}, ''),
    ),
  );
  table.appendChild(thead);
  const tbody = el('tbody', {});
  const sorted = [...envs].sort((a, b) => (a.name || '').localeCompare(b.name || ''));
  for (const env of sorted) {
    const latest = latestSyncByEnv.get(env.id);
    const feedingSyncs = syncsByTarget.get(env.id) || [];
    const fr = computeFreshness(latest, feedingSyncs);
    tbody.appendChild(buildLifecycleRow(env, latest, fr));
    if (state.expandedPlanEnvId === env.id) {
      tbody.appendChild(buildPlanPanelRow(env));
    }
  }
  table.appendChild(tbody);
  root.appendChild(table);
}

// freshness state: one of { fresh, aging, overdue, never, unconfigured }
/**
 * @param {SyncRun | undefined} latest
 * @param {DataSync[]} feedingSyncs
 * @returns {{ state: string, label: string, age: number | null }}
 */
function computeFreshness(latest, feedingSyncs) {
  if (feedingSyncs.length === 0) {
    return { state: 'unconfigured', label: 'no sync', age: null };
  }
  if (!latest) {
    return { state: 'never', label: 'never refreshed', age: null };
  }
  const ageHours = ageHoursFromIso(latest.finished_at);
  if (ageHours == null) return { state: 'unconfigured', label: 'unknown', age: null };

  // Pick the strictest interval among the feeding syncs' policies. Policies
  // that aren't time-based (per_test_run, versioned, on_demand) don't
  // contribute — they're "fresh on demand."
  let intervalH = null;
  for (const s of feedingSyncs) {
    const h = REFRESH_INTERVAL_HOURS[s.refresh_policy];
    if (h != null && (intervalH == null || h < intervalH)) intervalH = h;
  }
  if (intervalH == null) {
    // Time-based policy not declared — show the age but don't mark stale.
    return { state: 'fresh', label: fmtAge(ageHours), age: ageHours };
  }
  if (ageHours <= intervalH * 0.75)      return { state: 'fresh',    label: fmtAge(ageHours), age: ageHours };
  if (ageHours <= intervalH)             return { state: 'aging',    label: fmtAge(ageHours), age: ageHours };
  return { state: 'overdue', label: fmtAge(ageHours), age: ageHours };
}

/** @param {string | null | undefined} iso @returns {number | null} */
function ageHoursFromIso(iso) {
  if (!iso) return null;
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return null;
  return (Date.now() - t) / (1000 * 60 * 60);
}

/** @param {number | null} hours @returns {string} */
function fmtAge(hours) {
  if (hours == null) return '—';
  if (hours < 1)    return `${Math.round(hours * 60)}m ago`;
  if (hours < 48)   return `${Math.round(hours)}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}

/**
 * @param {TestEnvironment} env
 * @param {SyncRun | undefined} latest
 * @param {ReturnType<typeof computeFreshness>} fr
 * @returns {HTMLElement}
 */
function buildLifecycleRow(env, latest, fr) {
  const tr = el('tr', { dataset: { id: env.id } });
  tr.appendChild(el('td', {},
    el('div', { class: 'name' }, env.name || '(unnamed)'),
    el('div', { class: 'id' }, env.id ? env.id.slice(0, 8) : ''),
  ));
  tr.appendChild(el('td', {}, env.kind || '—'));
  tr.appendChild(el('td', { class: 'age' }, latest?.finished_at ? fmtTimestamp(latest.finished_at) : '—'));
  tr.appendChild(el('td', {}, el('span', { class: 'freshness ' + fr.state }, fr.label)));

  const plan = state.resetPlans.get(env.id);
  const costCell = el('td', { class: 'age' });
  if (plan) {
    costCell.textContent = plan.estimated_total_cost > 0
      ? `$${plan.estimated_total_cost.toFixed(2)} · ${Math.round(plan.estimated_total_minutes)}m`
      : `${Math.round(plan.estimated_total_minutes)}m`;
  } else {
    costCell.innerHTML = '<span style="color:var(--text-soft)">—</span>';
  }
  tr.appendChild(costCell);

  const expanded = state.expandedPlanEnvId === env.id;
  const btn = el('button', { class: expanded ? '' : 'primary' }, expanded ? 'Close' : 'Plan reset');
  btn.addEventListener('click', () => togglePlan(env.id));
  tr.appendChild(el('td', {}, btn));

  if (expanded) tr.classList.add('expanded');
  return tr;
}

/** @param {TestEnvironment} env @returns {HTMLElement} */
function buildPlanPanelRow(env) {
  const tr = el('tr', { class: 'plan-row' });
  const td = el('td', { colspan: '6' });
  const panel = el('div', { class: 'plan-panel' });
  const plan = state.resetPlans.get(env.id);
  if (!plan) {
    panel.appendChild(el('div', { class: 'plan-empty' }, 'Computing plan…'));
    // Cold-load case: URL lands on #lifecycle/<envId> with no plan cached.
    // togglePlan didn't run, so we kick the fetch here. ensurePlanFetched
    // is idempotent — repeated renders during loading won't pile up requests.
    ensurePlanFetched(env.id);
  } else {
    renderPlanPanel(panel, plan);
  }
  td.appendChild(panel);
  tr.appendChild(td);
  return tr;
}

/** @param {HTMLElement} panel @param {ResetPlan} plan */
function renderPlanPanel(panel, plan) {
  panel.innerHTML = '';

  // Summary
  const summary = el('div', { class: 'plan-summary' });
  summary.appendChild(el('span', {}, el('strong', {}, plan.steps.length), `step${plan.steps.length === 1 ? '' : 's'}`));
  summary.appendChild(el('span', {}, el('strong', {}, `${Math.round(plan.estimated_total_minutes)}m`), 'estimated'));
  summary.appendChild(el('span', {}, el('strong', {}, `$${plan.estimated_total_cost.toFixed(2)}`), 'cost'));
  if (plan.last_sync_at) {
    summary.appendChild(el('span', {}, el('strong', {}, 'last:'), fmtAge(ageHoursFromIso(plan.last_sync_at))));
  } else {
    summary.appendChild(el('span', {}, el('strong', {}, 'last:'), 'never'));
  }
  panel.appendChild(summary);

  // Blockers (if any) — render before steps so they sit up top.
  if (plan.blockers.length > 0) {
    panel.appendChild(el('div', { class: 'plan-section-head' }, 'Blockers'));
    const blockerWrap = el('div', { class: 'plan-blockers' });
    for (const b of plan.blockers) {
      blockerWrap.appendChild(el('div', { class: 'blocker' },
        el('span', { class: 'kind' }, b.kind.replace(/_/g, ' ')),
        b.message,
      ));
    }
    panel.appendChild(blockerWrap);
  }

  // Steps
  if (plan.steps.length === 0) {
    panel.appendChild(el('div', { class: 'plan-empty' },
      'No data syncs feed this environment. Register a data_sync row in Settings → Sync to enable reset planning.'));
  } else {
    panel.appendChild(el('div', { class: 'plan-section-head' }, 'Procedure'));
    const ol = el('ol', { class: 'plan-steps' });
    for (const s of plan.steps) {
      const li = el('li', {});
      li.appendChild(el('div', { class: 'step-head' },
        el('span', { class: 'step-target' }, `${s.source_label} → ${s.target_env_name}`),
        el('span', { class: 'step-cost' },
          s.estimated_cost > 0
            ? `${Math.round(s.estimated_minutes)}m · $${s.estimated_cost.toFixed(2)}`
            : `${Math.round(s.estimated_minutes)}m`),
      ));
      const meta = el('div', { class: 'step-meta' });
      meta.appendChild(el('span', { class: 'pill' }, s.kind));
      if (s.refresh_policy) meta.appendChild(el('span', { class: 'pill' }, s.refresh_policy));
      if (s.masking_summary) meta.appendChild(el('span', { class: 'pill' }, `masking: ${s.masking_summary}`));
      li.appendChild(meta);
      if (s.predecessor_orders && s.predecessor_orders.length > 0) {
        li.appendChild(el('div', { class: 'step-pred' }, `after step ${s.predecessor_orders.join(', ')}`));
      }
      ol.appendChild(li);
    }
    panel.appendChild(ol);
  }
}

// Idempotent fetch — kicked off from togglePlan AND from buildPlanPanelRow
// (when a cold load lands on #lifecycle/<envId> with no plan cached yet).
const _inFlightPlans = new Set();
/** @param {string} envId */
async function ensurePlanFetched(envId) {
  if (state.resetPlans.has(envId)) return;
  if (_inFlightPlans.has(envId)) return;
  _inFlightPlans.add(envId);
  try {
    const plan = await apiFetch(`/test_environment/${envId}/reset_plan`);
    state.resetPlans.set(envId, plan);
    if (state.screen === 'lifecycle' && state.expandedPlanEnvId === envId) {
      render();
    }
  } catch (e) {
    setError(e);
  } finally {
    _inFlightPlans.delete(envId);
  }
}

/** @param {string} envId */
function togglePlan(envId) {
  if (state.expandedPlanEnvId === envId) {
    state.expandedPlanEnvId = null;
    render();
    return;
  }
  state.expandedPlanEnvId = envId;
  // Blockers (in-flight runs) are live — drop the cache so the open
  // always reflects current state rather than a stale snapshot.
  state.resetPlans.delete(envId);
  render();
  ensurePlanFetched(envId);
}

/** @param {string | null | undefined} iso @returns {string} */
function fmtTimestamp(iso) {
  if (!iso) return '—';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  // Compact form: "May 19 14:32"
  return d.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
}

// ── Screen: Settings (simple CRUD) ───────────────────────────────────────────

const SETTINGS_EMPTY = {
  testInfrastructures: { title: 'No infrastructure yet',  lede: 'The substrate beneath every environment.' },
  mockSources:         { title: 'No mock sources yet',    lede: 'Pretend, until production catches up.' },
  dataSources:         { title: 'No data sources yet',    lede: 'Where test data comes from — and where it goes stale.' },
  testSuites:          { title: 'No test suites yet',     lede: 'Tests are stories about deployables. Catalogue them.' },
};

/** @param {HTMLElement} root @param {string} entityKey */
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
    el('button', { class: 'primary', onClick: () => createNew(entityKey) }, `New ${cfg.label.toLowerCase()}`),
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
      // Cross-app deeplink for test suites: deployable lives in Groundwork.
      const metaText = cfg.rowMeta(item, state.data);
      const metaEl = el('div', { class: 'meta' }, metaText);
      if (entityKey === 'testSuites' && item.deployable_id) {
        const sep = metaText ? ' · ' : '';
        const html = `${esc(metaText)}${esc(sep)}deployable: ${crossLink('groundwork', 'deployables', item.deployable_id, item.deployable_id.slice(0, 8))}`;
        metaEl.innerHTML = html;
      } else if (entityKey === 'testEnvironments' && item.deployable_id) {
        // Same treatment for test environments: deployable lives in Groundwork.
        const sep = metaText ? ' · ' : '';
        const html = `${esc(metaText)}${esc(sep)}deployable: ${crossLink('groundwork', 'deployables', item.deployable_id, item.deployable_id.slice(0, 8))}`;
        metaEl.innerHTML = html;
      }
      list.appendChild(el('div', {
        class: 'row',
        onClick: (e) => {
          // Don't expand the row when the user is clicking a link inside it.
          if (e.target.closest('a')) return;
          state.expandedSettingId = id;
          render();
        },
      },
        el('div', {},
          el('div', {}, cfg.rowLabel(item, state.data)),
          metaEl,
        ),
        el('button', { class: 'ghost' }, 'Edit'),
      ));
    } else {
      const form = el('div', { class: 'form-grid' });
      for (const f of cfg.fields) form.appendChild(yardFieldInput(f, item[f.name]));

      const wrap = el('div', { class: 'row expanded' },
        el('div', {},
          el('div', { style: 'font-weight:600; margin-bottom: 8px;' }, cfg.rowLabel(item, state.data)),
          form,
          el('div', { class: 'row-actions', style: 'margin-top:12px;' },
            el('button', { class: 'primary',
              onClick: async () => {
                const payload = readForm(form);
                try {
                  await updateRecord(entityKey, id, payload);
                  await loadAll();
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
                  await loadAll();
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

/** @returns {string} */
function newEntityKeyForCurrentScreen() {
  if (state.screen === 'environments') return 'testEnvironments';
  if (state.screen === 'runs')         return 'testRuns';
  if (state.screen === 'sync')         return 'dataSyncs';
  if (state.screen.startsWith('settings:')) return state.screen.slice('settings:'.length);
  return 'testEnvironments';
}

// ── Wire-up ──────────────────────────────────────────────────────────────────

/** @param {string | null | undefined} name @returns {boolean} */
function isValidScreen(name) {
  if (!name) return false;
  return Array.from(document.querySelectorAll('[data-screen]'))
    .some(el => el.dataset.screen === name);
}

// Hash format: `#<screen>` or `#<screen>/<id>`. The id, when present,
// targets a specific row/card/plan within that screen so URLs are shareable
// ("here's the env I'm asking about"). We derive the hash from state on
// every render so it always reflects what's on screen; expand/collapse
// uses history.replaceState (not assigning location.hash) to avoid
// piling up a back-button entry per click.

/** @param {string | null | undefined} raw @returns {{ screen: string | null, id: string | null }} */
function parseHash(raw) {
  const s = (raw || '').replace(/^#/, '');
  if (!s) return { screen: null, id: null };
  const slash = s.indexOf('/');
  if (slash < 0) return { screen: s, id: null };
  return { screen: s.slice(0, slash), id: s.slice(slash + 1) || null };
}

// What `id` slot does this screen care about? Returns null for screens
// without per-row focus (none currently — sync uses syncEnvId).
/** @param {string} screen @returns {string | null} */
function expandedIdForScreen(screen) {
  if (screen === 'environments') return state.expandedEnvId;
  if (screen === 'runs')         return state.expandedRunId;
  if (screen === 'sync')         return state.syncEnvId;
  if (screen === 'lifecycle')    return state.expandedPlanEnvId;
  if (screen && screen.startsWith('settings:')) return state.expandedSettingId;
  return null;
}

/** @param {string} screen @param {string | null} id */
function applyExpandedId(screen, id) {
  // Reset the per-screen expanded fields first so leftover state from
  // another screen doesn't leak when the URL routes us elsewhere.
  state.expandedEnvId = null;
  state.expandedRunId = null;
  state.expandedSettingId = null;
  state.expandedPlanEnvId = null;
  // syncEnvId is reassigned (sync is more "selection" than "expansion").
  if (screen === 'sync') {
    state.syncEnvId = id || state.syncEnvId;
  } else if (!id) {
    // no-op
  } else if (screen === 'environments')      state.expandedEnvId = id;
  else if (screen === 'runs')                state.expandedRunId = id;
  else if (screen === 'lifecycle')           state.expandedPlanEnvId = id;
  else if (screen.startsWith('settings:'))   state.expandedSettingId = id;
}

/** @returns {string} */
function buildHashFromState() {
  if (!state.screen) return '';
  const id = expandedIdForScreen(state.screen);
  return id ? `${state.screen}/${id}` : state.screen;
}

// Push current state into the URL. Called from render() so any state
// mutation that leads to a re-render also keeps the URL fresh.
function syncUrl() {
  const want = buildHashFromState();
  if (!want) return;
  if (location.hash.slice(1) !== want) {
    history.replaceState(null, '', `#${want}`);
  }
}

/** @param {string} screen @param {string | null} [id] */
function setScreen(screen, id = null) {
  state.screen = screen;
  state.search = '';
  $('#search').value = '';
  applyExpandedId(screen, id);
  closeMenu();
  render();
}

function initHashRouting() {
  window.addEventListener('hashchange', () => {
    const { screen, id } = parseHash(location.hash);
    if (!screen || !isValidScreen(screen)) return;
    // Avoid render loop when the hash already matches what's on screen.
    if (screen === state.screen && id === expandedIdForScreen(screen)) return;
    setScreen(screen, id);
  });
  const { screen, id } = parseHash(location.hash);
  if (screen && isValidScreen(screen)) {
    state.screen = screen;
    applyExpandedId(screen, id);
  } else {
    history.replaceState(null, '', `#${state.screen}`);
  }
}

function openMenu()  { $('#settings-menu').classList.add('open'); }
function closeMenu() { $('#settings-menu').classList.remove('open'); }
function toggleMenu(){ $('#settings-menu').classList.toggle('open'); }

function bindUI() {
  $$('#primary-nav .tab').forEach(tab => {
    tab.addEventListener('click', () => setScreen(tab.dataset.screen));
  });

  $('#btn-new').addEventListener('click', () => createNew(newEntityKeyForCurrentScreen()));

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

  // Modal Esc + Enter are handled by manifold-ui's promise modal itself;
  // we don't bind anything here.

  document.addEventListener('keydown', (e) => {
    const tag = document.activeElement?.tagName?.toLowerCase();
    const inField = ['input', 'textarea', 'select'].includes(tag);
    const modalOpen = !!document.querySelector('.modal-backdrop');

    if (e.key === 'Escape') {
      // The modal swallows Esc itself; bail if it's open so we don't also
      // collapse expanded rows behind it.
      if (modalOpen)                     { return; }
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
    if (e.key === 'n' && !inField && !modalOpen) {
      e.preventDefault();
      createNew(newEntityKeyForCurrentScreen());
      return;
    }
  });
}

async function init() {
  bindUI();
  initHashRouting();
  state.loading = true;
  setStatus('Loading…', 'info', { sticky: true });
  // /config.json publishes cross-app public URLs; load before first render
  // so cross-app anchors land with the right base.
  await loadManifoldConfig();
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
