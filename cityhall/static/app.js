// cityhall — planner frontend
// Vanilla JS ES module. No build step.

import mermaid from 'https://cdn.jsdelivr.net/npm/mermaid@10/dist/mermaid.esm.min.mjs';

mermaid.initialize({
  startOnLoad: false,
  theme: 'neutral',
  fontFamily: 'ui-sans-serif, system-ui, -apple-system, "SF Pro Text", Inter, sans-serif',
  securityLevel: 'loose',
});

// ── Constants ─────────────────────────────────────────────────────────────────

const ORG_KINDS  = ['enterprise', 'division', 'domain', 'product', 'team'];
const GATE_TYPES = ['AutoGate', 'ApprovalGate', 'WindowGate', 'QuiesceGate', 'FreezePeriod'];
const TIERS      = ['dev', 'integration', 'uat', 'prod'];

const ENDPOINTS = {
  orgNodes: '/org_node/api',
  bylaws: '/bylaw/api',
  changeRequests: '/change_request/api',
  plans: '/deployment_plan/api',
  gantts: '/gantt_output/api',
};

// ── State ─────────────────────────────────────────────────────────────────────

const state = {
  screen: 'org',
  data: { orgNodes: [], bylaws: [], changeRequests: [], plans: [], gantts: [] },
  org: {
    expanded: new Set(),     // node ids expanded in tree
    effective: new Map(),    // node id -> array of effective bylaws
    search: '',
  },
  cr: {
    open: false,
    step: 1,
    draftId: null,
    fields: { summary: '', description: '', tier: 'dev', requested_by: '' },
    targets: [],             // chip array
    plan: null,              // last computed plan envelope
    search: '',
  },
  plans: {
    tier: 'dev',
    expanded: new Set(),
    rendered: new Set(),
    ganttCache: new Map(),   // plan id -> mermaid string
  },
  bylaws: {
    selectedOrg: '',
    sortDir: 'asc',
  },
};

// ── Tiny DOM helpers ──────────────────────────────────────────────────────────

const $  = (sel, root = document) => root.querySelector(sel);
const $$ = (sel, root = document) => Array.from(root.querySelectorAll(sel));

function el(tag, props = {}, children = []) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) {
    if (k === 'class') node.className = v;
    else if (k === 'dataset') Object.assign(node.dataset, v);
    else if (k.startsWith('on') && typeof v === 'function') node.addEventListener(k.slice(2), v);
    else if (k === 'html') node.innerHTML = v;
    else if (v !== null && v !== undefined) node.setAttribute(k, v);
  }
  for (const c of [].concat(children)) {
    if (c == null || c === false) continue;
    node.appendChild(typeof c === 'string' ? document.createTextNode(c) : c);
  }
  return node;
}

function esc(s) {
  return String(s ?? '').replace(/&/g, '&amp;').replace(/"/g, '&quot;')
    .replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

// ── API ───────────────────────────────────────────────────────────────────────

async function api(url, opts) {
  const res = await fetch(url, opts);
  if (!res.ok) {
    const body = await res.text().catch(() => '');
    throw new Error(`${opts?.method || 'GET'} ${url} → ${res.status}${body ? ': ' + body : ''}`);
  }
  if (res.status === 204) return null;
  const ct = res.headers.get('content-type') || '';
  return ct.includes('application/json') ? res.json() : res.text();
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

const apiList   = (key)            => api(ENDPOINTS[key]);
const apiCreate = (key, body)      => api(ENDPOINTS[key], { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
const apiUpdate = (key, id, body)  => api(`${ENDPOINTS[key]}/${id}`, { method: 'PUT',    headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
const apiDelete = (key, id)        => api(`${ENDPOINTS[key]}/${id}`, { method: 'DELETE' });
const apiEffectiveBylaws = (id)    => api(`/org_node/${encodeURIComponent(id)}/effective_bylaws`);
const apiComputePlan = (crId, tier) =>
  api(`/change_request/${encodeURIComponent(crId)}/plan`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ tier }) });
const apiRenderGantt = (planId) =>
  api(`/deployment_plan/${encodeURIComponent(planId)}/gantt`, { method: 'POST' });

async function loadAll() {
  const [orgNodes, bylaws, changeRequests, plans, gantts] = await Promise.all([
    gqlQuery('/org_node/graph', '{ getAll { id name kind parent_id team_id } }')
      .then(d => d.getAll).catch(() => []),
    gqlQuery(
      '/bylaw/graph',
      '{ getAll { id org_node_id gate_type priority description conditions window quiesce_for approvers } }'
    ).then(d => d.getAll).catch(() => []),
    gqlQuery(
      '/change_request/graph',
      '{ getAll { id summary description tier status target_deployables target_versions requested_by } }'
    ).then(d => d.getAll).catch(() => []),
    gqlQuery(
      '/deployment_plan/graph',
      '{ getAll { id change_request_id tier summary steps blockers computed_at } }'
    ).then(d => d.getAll).catch(() => []),
    gqlQuery(
      '/gantt_output/graph',
      '{ getAll { id deployment_plan_id tier mermaid } }'
    ).then(d => d.getAll).catch(() => []),
  ]);
  state.data.orgNodes       = Array.isArray(orgNodes) ? orgNodes : [];
  state.data.bylaws         = Array.isArray(bylaws) ? bylaws : [];
  state.data.changeRequests = Array.isArray(changeRequests) ? changeRequests : [];
  state.data.plans          = Array.isArray(plans) ? plans : [];
  state.data.gantts         = Array.isArray(gantts) ? gantts : [];
}

// ── Status strip ──────────────────────────────────────────────────────────────

let statusTimer = null;
function flash(message, kind = 'info', persistMs = 3000) {
  const strip = $('#status-strip');
  strip.className = '';
  strip.classList.add(kind, 'visible');
  strip.textContent = message;
  if (statusTimer) clearTimeout(statusTimer);
  if (persistMs > 0) {
    statusTimer = setTimeout(() => strip.classList.remove('visible'), persistMs);
  }
}
function clearFlash() {
  const strip = $('#status-strip');
  strip.classList.remove('visible');
  if (statusTimer) { clearTimeout(statusTimer); statusTimer = null; }
}

// ── Screen routing ────────────────────────────────────────────────────────────

const SCREENS = ['org', 'changes', 'plans', 'bylaws'];

function setScreen(name) {
  state.screen = name;
  $$('.screen').forEach(s => s.classList.toggle('active', s.id === `screen-${name}`));
  $$('.tab').forEach(t => t.classList.toggle('active', t.dataset.screen === name));
  switch (name) {
    case 'org':     renderOrgTree(); break;
    case 'changes': renderChangeRequests(); break;
    case 'plans':   renderPlans(); break;
    case 'bylaws':  renderBylawsScreen(); break;
  }
  updateFooterMeta();

  if (location.hash.slice(1) !== name) {
    location.hash = name;
  }
}

function initHashRouting() {
  window.addEventListener('hashchange', () => {
    const key = location.hash.slice(1);
    if (SCREENS.includes(key) && key !== state.screen) {
      setScreen(key);
    }
  });
  const initial = location.hash.slice(1);
  if (SCREENS.includes(initial)) {
    state.screen = initial;
  } else {
    location.replace('#' + state.screen);
  }
}

// ── Footer meta ───────────────────────────────────────────────────────────────

function updateFooterMeta() {
  const host = $('#footer-meta');
  if (!host) return;
  const d = state.data;
  const orgCount = d.orgNodes.length;
  const crCount  = d.changeRequests.length;
  const planCount = d.plans.length;
  const bylawCount = d.bylaws.length;
  let text = '';
  switch (state.screen) {
    case 'org':
      text = `${orgCount} org node${orgCount === 1 ? '' : 's'} · ${bylawCount} bylaw${bylawCount === 1 ? '' : 's'}`;
      break;
    case 'changes':
      text = `${crCount} change request${crCount === 1 ? '' : 's'} · ${planCount} plan${planCount === 1 ? '' : 's'}`;
      break;
    case 'plans': {
      const tierCount = d.plans.filter(p => (p.tier || '') === state.plans.tier).length;
      text = `${tierCount} ${state.plans.tier} plan${tierCount === 1 ? '' : 's'} · ${planCount} total`;
      break;
    }
    case 'bylaws':
      text = `${bylawCount} bylaw${bylawCount === 1 ? '' : 's'} · ${orgCount} org node${orgCount === 1 ? '' : 's'}`;
      break;
    default:
      text = `${orgCount} org nodes · ${planCount} plans`;
  }
  host.textContent = text;
}

// ── Editorial empty-state helper ──────────────────────────────────────────────

function emptyCard({ title, lede, hint }) {
  const card = el('div', { class: 'empty-card' });
  card.appendChild(el('span', { class: 'empty-mark' }, '§'));
  card.appendChild(el('h3', {}, title));
  card.appendChild(el('p', { class: 'lede' }, lede));
  if (hint) {
    const p = el('p', { class: 'hint', html: hint });
    card.appendChild(p);
  }
  return card;
}

// ── ORG screen ────────────────────────────────────────────────────────────────

function nodeName(id) {
  const n = state.data.orgNodes.find(n => n.id === id);
  return n?.name || id || '—';
}

function buildOrgIndex() {
  const byParent = new Map();
  byParent.set(null, []);
  for (const n of state.data.orgNodes) {
    const parent = n.parent_id || null;
    if (!byParent.has(parent)) byParent.set(parent, []);
    byParent.get(parent).push(n);
  }
  for (const arr of byParent.values()) arr.sort((a, b) => (a.name || '').localeCompare(b.name || ''));
  return byParent;
}

function renderOrgTree() {
  const tree = $('#org-tree');
  tree.innerHTML = '';
  const needle = state.org.search.trim().toLowerCase();
  const byParent = buildOrgIndex();
  const enterprises = state.data.orgNodes.filter(n => (n.kind === 'enterprise') && !n.parent_id);
  enterprises.sort((a, b) => (a.name || '').localeCompare(b.name || ''));

  // Roots: enterprises (no parent). Anything else without a known parent we still surface
  const knownIds = new Set(state.data.orgNodes.map(n => n.id));
  const orphanRoots = state.data.orgNodes.filter(n =>
    n.kind !== 'enterprise' &&
    n.parent_id &&
    !knownIds.has(n.parent_id)
  );
  const noParent = state.data.orgNodes.filter(n =>
    n.kind !== 'enterprise' && !n.parent_id
  );
  const roots = [...enterprises, ...noParent, ...orphanRoots];

  if (!roots.length) {
    tree.appendChild(emptyCard({
      title: 'No org nodes yet',
      lede: 'An organisation is a tree of bylaws.',
      hint: 'Press <kbd>n</kbd> to plant the root.',
    }));
    $('#org-count').textContent = '';
    return;
  }

  for (const r of roots) tree.appendChild(buildTreeNode(r, byParent, needle));
  $('#org-count').textContent = `${state.data.orgNodes.length} nodes`;
}

function nodeMatches(node, needle, byParent) {
  if (!needle) return true;
  const name = (node.name || '').toLowerCase();
  if (name.includes(needle)) return true;
  // recurse children
  const kids = byParent.get(node.id) || [];
  return kids.some(k => nodeMatches(k, needle, byParent));
}

function buildTreeNode(node, byParent, needle) {
  if (!nodeMatches(node, needle, byParent)) return document.createDocumentFragment();
  const id = node.id;
  const kids = byParent.get(id) || [];
  const isLeaf = node.kind === 'team' || kids.length === 0;
  const isExpanded = state.org.expanded.has(id);

  const wrap = el('div', { class: 'tree-node', dataset: { id } });
  const row = el('div', { class: 'tree-row' });

  const toggle = el('span', { class: 'tree-toggle' }, kids.length ? (isExpanded ? '▼' : '▶') : '·');
  const name = el('span', { class: 'tree-name' }, node.name || id);
  const kindPill = el('span', { class: 'pill' }, node.kind || '?');
  row.append(toggle, name, kindPill);

  if (isLeaf && node.team_id) {
    row.append(el('span', { class: 'tree-team' }, [`team: `, el('span', { class: 'mono' }, node.team_id)]));
  } else if (isLeaf) {
    row.append(el('span', { class: 'tree-team muted' }, 'no team'));
  }

  const addBylawBtn = el('button', {
    class: 'btn subtle sm',
    title: 'Attach a bylaw to this node',
    onclick: (e) => {
      e.stopPropagation();
      state.bylaws.selectedOrg = id;
      setScreen('bylaws');
      $('#bl-org').value = id;
      renderBylawsScreen();
      $('#bl-gate').focus();
    },
  }, '+ bylaw');
  row.append(addBylawBtn);

  row.addEventListener('click', () => toggleNode(id));
  wrap.append(row);

  if (isExpanded) {
    // Effective bylaws block
    const effHost = el('div', { class: 'tree-effective' });
    effHost.appendChild(renderEffective(id));
    wrap.append(effHost);

    if (kids.length) {
      const childrenWrap = el('div', { class: 'tree-children' });
      for (const k of kids) childrenWrap.append(buildTreeNode(k, byParent, needle));
      wrap.append(childrenWrap);
    }
  }
  return wrap;
}

async function toggleNode(id) {
  if (state.org.expanded.has(id)) {
    state.org.expanded.delete(id);
  } else {
    state.org.expanded.add(id);
    if (!state.org.effective.has(id)) {
      try {
        const list = await apiEffectiveBylaws(id);
        state.org.effective.set(id, Array.isArray(list) ? list : []);
      } catch (err) {
        state.org.effective.set(id, []);
        flash(err.message, 'error');
      }
    }
  }
  renderOrgTree();
}

function renderEffective(id) {
  const list = state.org.effective.get(id);
  if (!list) return el('div', { class: 'effective-empty' }, 'Loading effective bylaws…');
  if (!list.length) return el('div', { class: 'effective-empty' }, 'No effective bylaws inherited or attached.');
  const heading = el('h3', { class: 'section', style: 'margin-bottom: 8px;' }, 'Effective bylaws');
  const items = el('div', { class: 'effective-list' });
  for (const b of list) {
    items.appendChild(el('div', { class: 'effective-item' }, [
      el('span', { class: 'pill primary' }, b.gate_type || '?'),
      el('span', {}, b.description || '(no description)'),
      el('span', { class: 'mono', style: 'margin-left: auto;' }, `priority ${b.priority ?? '—'}`),
    ]));
  }
  const wrap = el('div', {});
  wrap.append(heading, items);
  return wrap;
}

// New node form
function showOrgForm() {
  const card = $('#org-form-card');
  card.classList.remove('hidden');
  $('#org-f-name').value = '';
  $('#org-f-kind').value = 'team';
  $('#org-f-team').value = '';
  // populate parent select
  const sel = $('#org-f-parent');
  sel.innerHTML = '<option value="">— none —</option>';
  for (const n of state.data.orgNodes) {
    const opt = el('option', { value: n.id }, `${n.name || n.id} (${n.kind || '?'})`);
    sel.appendChild(opt);
  }
  $('#org-f-name').focus();
}
function hideOrgForm() { $('#org-form-card').classList.add('hidden'); }

async function saveOrgForm() {
  const fields = {
    name: $('#org-f-name').value.trim(),
    kind: $('#org-f-kind').value,
    parent_id: $('#org-f-parent').value || undefined,
    team_id: $('#org-f-team').value.trim() || undefined,
  };
  if (!fields.name) { flash('Name is required', 'error'); $('#org-f-name').focus(); return; }
  for (const k of Object.keys(fields)) if (fields[k] === undefined) delete fields[k];
  try {
    const created = await apiCreate('orgNodes', fields);
    state.data.orgNodes.push(created);
    hideOrgForm();
    renderOrgTree();
    flash(`Created ${created.name}`, 'success');
  } catch (err) { flash(err.message, 'error'); }
}

function initOrg() {
  $('#org-search').addEventListener('input', e => { state.org.search = e.target.value; renderOrgTree(); });
  $('#org-new').addEventListener('click', showOrgForm);
  $('#org-form-cancel').addEventListener('click', hideOrgForm);
  $('#org-form-cancel-2').addEventListener('click', hideOrgForm);
  $('#org-form-save').addEventListener('click', saveOrgForm);
}

// ── CHANGES screen ────────────────────────────────────────────────────────────

function renderChangeRequests() {
  const tbody = $('#cr-tbody');
  tbody.innerHTML = '';
  const needle = state.cr.search.trim().toLowerCase();
  const items = state.data.changeRequests.filter(cr => {
    if (!needle) return true;
    return (cr.summary || '').toLowerCase().includes(needle);
  });

  if (!items.length) {
    const td = el('td', { colspan: 5, style: 'padding: 0; background: var(--surface);' });
    td.appendChild(emptyCard({
      title: 'No change requests yet',
      lede: 'A change request is a promise to perturb production.',
      hint: 'Press <kbd>n</kbd> to draft one.',
    }));
    tbody.appendChild(el('tr', {}, td));
    $('#cr-count').textContent = '';
    return;
  }

  for (const cr of items) {
    const targets = parseTargets(cr.target_deployables);
    tbody.appendChild(el('tr', {}, [
      el('td', {}, [
        el('div', {}, cr.summary || '(no summary)'),
        el('div', { class: 'mono' }, cr.id?.slice(0, 8) || ''),
      ]),
      el('td', {}, cr.tier ? el('span', { class: 'pill' }, cr.tier) : el('span', { class: 'muted' }, '—')),
      el('td', {}, statusPill(cr.status)),
      el('td', {}, el('span', { class: 'mono' }, targets.length ? `${targets.length} targets` : '—')),
      el('td', { style: 'text-align: right;' }, el('button', {
        class: 'btn sm',
        onclick: () => editChangeRequest(cr),
      }, 'View')),
    ]));
  }
  $('#cr-count').textContent = `${items.length} request${items.length === 1 ? '' : 's'}`;
}

function statusPill(status) {
  if (!status) return el('span', { class: 'muted' }, '—');
  const map = {
    draft: 'pill',
    submitted: 'pill primary',
    approved: 'pill success',
    rejected: 'pill danger',
    deployed: 'pill success',
    rolled_back: 'pill warn',
  };
  return el('span', { class: map[status] || 'pill' }, status);
}

function parseTargets(s) {
  if (!s) return [];
  if (Array.isArray(s)) return s;
  try {
    const v = JSON.parse(s);
    if (Array.isArray(v)) return v.map(String);
  } catch { /* fallthrough */ }
  return s.split(',').map(t => t.trim()).filter(Boolean);
}

function editChangeRequest(cr) {
  state.cr.open = true;
  state.cr.draftId = cr.id;
  state.cr.fields = {
    summary: cr.summary || '',
    description: cr.description || '',
    tier: cr.tier || 'dev',
    requested_by: cr.requested_by || '',
  };
  state.cr.targets = parseTargets(cr.target_deployables);
  state.cr.plan = null;
  state.cr.step = 1;
  showWizard();
}

function newChangeRequest() {
  state.cr.open = true;
  state.cr.draftId = null;
  state.cr.fields = { summary: '', description: '', tier: 'dev', requested_by: '' };
  state.cr.targets = [];
  state.cr.plan = null;
  state.cr.step = 1;
  showWizard();
}

function showWizard() {
  $('#cr-wizard').classList.remove('hidden');
  $('#cr-list-card').classList.add('hidden');
  $('#cr-summary').value = state.cr.fields.summary;
  $('#cr-description').value = state.cr.fields.description;
  $('#cr-tier').value = state.cr.fields.tier;
  $('#cr-requested-by').value = state.cr.fields.requested_by;
  renderChips();
  setWizardStep(state.cr.step);
  $('#cr-summary').focus();
}

function hideWizard() {
  state.cr.open = false;
  $('#cr-wizard').classList.add('hidden');
  $('#cr-list-card').classList.remove('hidden');
  renderChangeRequests();
}

function setWizardStep(n) {
  state.cr.step = n;
  $$('.step').forEach(stepEl => {
    const s = parseInt(stepEl.dataset.step, 10);
    stepEl.classList.toggle('active', s === n);
    stepEl.classList.toggle('done', s < n);
  });
  $$('.wizard-pane').forEach(p => p.classList.toggle('hidden', parseInt(p.dataset.pane, 10) !== n));
  $('#cr-back').disabled = n === 1;
  $('#cr-next').textContent = n === 4 ? 'Submit' : 'Next';
  if (n === 3) refreshPlanPane();
  if (n === 4) renderReviewPane();
}

function renderChips() {
  const host = $('#cr-chip-input');
  // remove all but the trailing input
  $$('.chip', host).forEach(c => c.remove());
  const entry = $('#cr-chip-entry');
  for (const t of state.cr.targets) {
    const chip = el('span', { class: 'chip' }, [
      t,
      el('button', { type: 'button', title: 'Remove', onclick: () => { state.cr.targets = state.cr.targets.filter(x => x !== t); renderChips(); } }, '×'),
    ]);
    host.insertBefore(chip, entry);
  }
}

function captureFields() {
  state.cr.fields.summary = $('#cr-summary').value.trim();
  state.cr.fields.description = $('#cr-description').value.trim();
  state.cr.fields.tier = $('#cr-tier').value;
  state.cr.fields.requested_by = $('#cr-requested-by').value.trim();
}

async function persistDraft() {
  captureFields();
  const body = {
    summary: state.cr.fields.summary,
    description: state.cr.fields.description,
    tier: state.cr.fields.tier,
    status: state.cr.draftId ? undefined : 'draft',
    target_deployables: JSON.stringify(state.cr.targets),
    requested_by: state.cr.fields.requested_by || undefined,
  };
  for (const k of Object.keys(body)) if (body[k] === undefined) delete body[k];

  if (state.cr.draftId) {
    const original = state.data.changeRequests.find(c => c.id === state.cr.draftId);
    if (original?.status) body.status = original.status;
    const updated = await apiUpdate('changeRequests', state.cr.draftId, body);
    const idx = state.data.changeRequests.findIndex(c => c.id === state.cr.draftId);
    if (idx >= 0) state.data.changeRequests[idx] = updated;
    return updated;
  } else {
    const created = await apiCreate('changeRequests', body);
    state.cr.draftId = created.id;
    state.data.changeRequests.unshift(created);
    return created;
  }
}

async function nextStep() {
  captureFields();
  if (state.cr.step === 1) {
    if (!state.cr.fields.summary) { flash('Summary is required', 'error'); $('#cr-summary').focus(); return; }
    setWizardStep(2);
  } else if (state.cr.step === 2) {
    if (!state.cr.targets.length) { flash('Add at least one target deployable', 'error'); $('#cr-chip-entry').focus(); return; }
    try {
      await persistDraft();
      setWizardStep(3);
    } catch (err) { flash(err.message, 'error'); }
  } else if (state.cr.step === 3) {
    setWizardStep(4);
  } else if (state.cr.step === 4) {
    try {
      // Update status to submitted
      const original = state.data.changeRequests.find(c => c.id === state.cr.draftId);
      const { id: _id, ...rest } = original || {};
      const body = { ...rest, status: 'submitted' };
      const updated = await apiUpdate('changeRequests', state.cr.draftId, body);
      const idx = state.data.changeRequests.findIndex(c => c.id === state.cr.draftId);
      if (idx >= 0) state.data.changeRequests[idx] = updated;
      flash('Change request submitted', 'success');
      hideWizard();
      // refresh plans data and switch to plans view
      try {
        const plans = await apiList('plans');
        state.data.plans = Array.isArray(plans) ? plans : [];
      } catch { /* ignore */ }
      setScreen('plans');
    } catch (err) { flash(err.message, 'error'); }
  }
}

function backStep() {
  if (state.cr.step > 1) setWizardStep(state.cr.step - 1);
}

async function refreshPlanPane() {
  const host = $('#cr-plan-output');
  host.innerHTML = '';
  host.appendChild(el('div', { class: 'muted', style: 'font-size: 13px;' }, 'Computing plan…'));

  if (!state.cr.draftId) {
    host.innerHTML = '';
    host.appendChild(el('div', { class: 'muted' }, 'No draft id — please advance from step 2 first.'));
    return;
  }
  try {
    const planEnvelope = await apiComputePlan(state.cr.draftId, state.cr.fields.tier || 'dev');
    state.cr.plan = planEnvelope;
    // also stash in plans list (latest)
    state.data.plans.unshift(planEnvelope);
    host.innerHTML = '';
    host.appendChild(renderPlanDetail(planEnvelope));
  } catch (err) {
    host.innerHTML = '';
    host.appendChild(el('div', { class: 'plan-blockers' }, [
      el('h4', {}, 'Could not compute plan'),
      el('div', {}, err.message),
    ]));
  }
}

function renderPlanDetail(envelope) {
  let steps = [];
  let blockers = [];
  try { steps = JSON.parse(envelope?.steps || '[]'); } catch { steps = []; }
  try { blockers = JSON.parse(envelope?.blockers || '[]'); } catch { blockers = []; }

  const wrap = el('div', { class: 'stack' });

  if (!steps.length && !blockers.length) {
    wrap.appendChild(el('div', { class: 'muted', style: 'font-size: 13px;' }, 'Plan computed with no steps.'));
  }

  // Group steps by deployable
  const groups = new Map();
  for (const s of steps) {
    const key = s.deployable_id || s.deployable || 'general';
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(s);
  }
  for (const [deployable, list] of groups) {
    const group = el('div', { class: 'plan-group' });
    group.appendChild(el('h4', {}, deployable));
    const stepsList = el('div', { class: 'plan-steps' });
    for (const s of list) {
      stepsList.appendChild(el('div', { class: 'plan-step-item' }, [
        el('span', { class: 'pill primary' }, s.gate_type || s.kind || 'step'),
        el('div', { class: 'grow' }, [
          el('div', {}, s.description || s.label || s.name || '(step)'),
          el('div', { class: 'gate-name' }, s.bylaw_id ? `bylaw ${s.bylaw_id}` : (s.org_node_id ? `node ${s.org_node_id}` : '')),
        ]),
      ]));
    }
    group.appendChild(stepsList);
    wrap.appendChild(group);
  }

  if (blockers.length) {
    const block = el('div', { class: 'plan-blockers' }, [
      el('h4', {}, `${blockers.length} blocker${blockers.length === 1 ? '' : 's'}`),
      el('ul', { html: blockers.map(b => `<li>${esc(typeof b === 'string' ? b : JSON.stringify(b))}</li>`).join('') }),
    ]);
    wrap.appendChild(block);
  }

  return wrap;
}

function renderReviewPane() {
  const host = $('#cr-summary-review');
  host.innerHTML = '';
  const f = state.cr.fields;
  host.append(
    el('div', {}, [el('strong', {}, 'Summary: '), f.summary || '(none)']),
    el('div', { style: 'margin-top: 6px;' }, [el('strong', {}, 'Tier: '), f.tier]),
    el('div', { style: 'margin-top: 6px;' }, [el('strong', {}, 'Targets: '), state.cr.targets.join(', ') || '(none)']),
    el('div', { style: 'margin-top: 6px;' }, [el('strong', {}, 'Requested by: '), f.requested_by || '(unspecified)']),
  );
}

function initChanges() {
  $('#cr-search').addEventListener('input', e => { state.cr.search = e.target.value; renderChangeRequests(); });
  $('#cr-new').addEventListener('click', newChangeRequest);
  $('#cr-cancel').addEventListener('click', hideWizard);
  $('#cr-back').addEventListener('click', backStep);
  $('#cr-next').addEventListener('click', nextStep);
  $('#cr-recompute').addEventListener('click', refreshPlanPane);

  const entry = $('#cr-chip-entry');
  entry.addEventListener('keydown', e => {
    if (e.key === 'Enter') {
      e.preventDefault();
      const v = entry.value.trim();
      if (v && !state.cr.targets.includes(v)) {
        state.cr.targets.push(v);
        renderChips();
      }
      entry.value = '';
    } else if (e.key === 'Backspace' && !entry.value && state.cr.targets.length) {
      state.cr.targets.pop();
      renderChips();
    }
  });
}

// ── PLANS screen ──────────────────────────────────────────────────────────────

function renderPlans() {
  // Update tier counts
  for (const tier of TIERS) {
    const count = state.data.plans.filter(p => (p.tier || '') === tier).length;
    const span = $(`.tier-count[data-count="${tier}"]`);
    if (span) span.textContent = count;
  }
  // Active tab
  $$('.tier-tab').forEach(t => t.classList.toggle('active', t.dataset.tier === state.plans.tier));

  const host = $('#plan-list');
  host.innerHTML = '';
  const filtered = state.data.plans.filter(p => (p.tier || '') === state.plans.tier);
  if (!filtered.length) {
    host.appendChild(emptyCard({
      title: `No ${state.plans.tier} plans yet`,
      lede: 'A plan is the bylaw chain made executable.',
      hint: 'Submit a change request to compute a deployment plan.',
    }));
    return;
  }
  // Newest first
  filtered.sort((a, b) => (b.computed_at || '').localeCompare(a.computed_at || ''));
  for (const plan of filtered) host.appendChild(buildPlanCard(plan));
}

function buildPlanCard(plan) {
  const id = plan.id;
  const expanded = state.plans.expanded.has(id);
  let blockers = [];
  try { blockers = JSON.parse(plan.blockers || '[]'); } catch {}
  const card = el('div', { class: `plan-card${expanded ? ' expanded' : ''}`, dataset: { id } });
  const head = el('div', { class: 'plan-card-head', onclick: () => togglePlan(id) });
  head.append(
    el('div', {}, [
      el('div', { class: 'plan-card-title' }, plan.summary || `Plan for ${(plan.change_request_id || '').slice(0, 8) || 'unknown'}`),
      el('div', { class: 'plan-card-id' }, [
        `${id?.slice(0, 8) || ''} · `,
        plan.computed_at ? `computed ${plan.computed_at}` : 'no timestamp',
      ]),
    ]),
    el('div', { class: 'row' }, [
      blockers.length ? el('span', { class: 'pill danger' }, `${blockers.length} blockers`) : el('span', { class: 'pill success' }, 'clear'),
      el('span', { class: 'pill primary' }, plan.tier || '?'),
      el('span', { class: 'tree-toggle' }, expanded ? '▼' : '▶'),
    ]),
  );
  card.append(head);

  const body = el('div', { class: 'plan-card-body' });
  body.append(renderPlanDetail(plan));
  body.append(el('div', { style: 'margin-top: 14px;' }, [
    el('h3', { class: 'section', style: 'margin-bottom: 8px;' }, 'Gantt'),
    el('div', { class: 'gantt-host', dataset: { ganttFor: id } }, el('div', { class: 'muted' }, 'Loading Gantt…')),
  ]));
  card.append(body);
  return card;
}

async function togglePlan(id) {
  if (state.plans.expanded.has(id)) {
    state.plans.expanded.delete(id);
    renderPlans();
    return;
  }
  state.plans.expanded.add(id);
  renderPlans();
  await renderGanttFor(id);
}

async function renderGanttFor(id) {
  const host = document.querySelector(`[data-gantt-for="${CSS.escape(id)}"]`);
  if (!host) return;
  let mermaidSrc = state.plans.ganttCache.get(id);
  if (!mermaidSrc) {
    try {
      const env = await apiRenderGantt(id);
      mermaidSrc = env?.mermaid || '';
      state.plans.ganttCache.set(id, mermaidSrc);
    } catch (err) {
      host.innerHTML = '';
      host.appendChild(el('div', { class: 'plan-blockers' }, [
        el('h4', {}, 'Gantt failed'),
        el('div', {}, err.message),
      ]));
      return;
    }
  }
  if (!mermaidSrc || !mermaidSrc.trim()) {
    host.innerHTML = '';
    host.appendChild(el('div', { class: 'muted' }, 'No Gantt content returned.'));
    return;
  }
  host.innerHTML = '';
  const pre = el('pre', { class: 'mermaid' }, mermaidSrc);
  host.appendChild(pre);
  try {
    await mermaid.run({ nodes: [pre] });
  } catch (err) {
    host.innerHTML = '';
    host.appendChild(el('div', { class: 'plan-blockers' }, [
      el('h4', {}, 'Mermaid render failed'),
      el('div', {}, err.message || String(err)),
      el('details', { style: 'margin-top: 8px;' }, [
        el('summary', { class: 'muted' }, 'Source'),
        el('pre', { class: 'mono', style: 'white-space: pre-wrap; margin-top: 6px;' }, mermaidSrc),
      ]),
    ]));
  }
}

function initPlans() {
  $$('.tier-tab').forEach(tab => tab.addEventListener('click', () => {
    state.plans.tier = tab.dataset.tier;
    renderPlans();
    updateFooterMeta();
  }));
}

// ── BYLAWS screen ─────────────────────────────────────────────────────────────

function renderBylawsScreen() {
  // Org select
  const sel = $('#bl-org');
  const cur = state.bylaws.selectedOrg;
  sel.innerHTML = '<option value="">— select —</option>';
  for (const n of state.data.orgNodes) {
    const opt = el('option', { value: n.id }, `${n.name || n.id} (${n.kind || '?'})`);
    if (n.id === cur) opt.selected = true;
    sel.appendChild(opt);
  }
  applyConditional();
  renderBylawsTable();
}

function applyConditional() {
  const gate = $('#bl-gate').value;
  $$('.field-cond').forEach(host => {
    const required = host.dataset.cond.split(' ').includes(gate);
    host.classList.toggle('hidden', !required);
  });
}

function renderBylawsTable() {
  const tbody = $('#bl-tbody');
  tbody.innerHTML = '';
  const orgId = state.bylaws.selectedOrg;
  const items = state.data.bylaws.filter(b => !orgId || b.org_node_id === orgId);
  const dir = state.bylaws.sortDir === 'asc' ? 1 : -1;
  items.sort((a, b) => {
    const pa = parseInt(a.priority ?? '0', 10) || 0;
    const pb = parseInt(b.priority ?? '0', 10) || 0;
    return (pa - pb) * dir;
  });
  if (!items.length) {
    const td = el('td', { colspan: 5, style: 'padding: 0; background: var(--surface);' });
    td.appendChild(emptyCard({
      title: orgId ? 'No bylaws on this node' : 'No org node selected',
      lede: 'Govern from the top down — children may add, never override.',
      hint: orgId ? 'Use the form above to attach a bylaw.' : 'Pick an org node from the select above.',
    }));
    tbody.appendChild(el('tr', {}, td));
    $('#bl-count').textContent = '';
    return;
  }
  for (const b of items) {
    const detailParts = [];
    if (b.window) detailParts.push(`window: ${b.window}`);
    if (b.quiesce_for) detailParts.push(`quiesce ${b.quiesce_for}`);
    if (b.approvers) detailParts.push(`approvers: ${b.approvers}`);
    if (b.conditions) detailParts.push(`cond: ${b.conditions}`);
    tbody.appendChild(el('tr', {}, [
      el('td', { class: 'priority-cell' }, String(b.priority ?? '—')),
      el('td', {}, el('span', { class: 'pill primary' }, b.gate_type || '?')),
      el('td', {}, b.description || el('span', { class: 'muted' }, '—')),
      el('td', { class: 'mono' }, detailParts.join(' · ') || '—'),
      el('td', { style: 'text-align: right;' }, el('button', {
        class: 'btn sm danger',
        onclick: () => deleteBylaw(b.id),
      }, 'Delete')),
    ]));
  }
  $('#bl-count').textContent = `${items.length} bylaw${items.length === 1 ? '' : 's'}`;

  // Wire sort header
  $$('th[data-sort]').forEach(th => {
    th.onclick = () => {
      state.bylaws.sortDir = state.bylaws.sortDir === 'asc' ? 'desc' : 'asc';
      renderBylawsTable();
    };
  });
}

async function deleteBylaw(id) {
  if (!confirm('Delete this bylaw?')) return;
  try {
    await apiDelete('bylaws', id);
    state.data.bylaws = state.data.bylaws.filter(b => b.id !== id);
    renderBylawsTable();
    flash('Bylaw deleted', 'success');
  } catch (err) { flash(err.message, 'error'); }
}

async function saveBylaw() {
  const orgId = $('#bl-org').value;
  const gate = $('#bl-gate').value;
  if (!orgId) { flash('Select an org node first', 'error'); return; }
  if (!gate) { flash('Choose a gate type', 'error'); return; }

  const body = {
    org_node_id: orgId,
    gate_type: gate,
    priority: $('#bl-priority').value.trim() || undefined,
    description: $('#bl-desc').value.trim() || undefined,
    window: $('#bl-window').value.trim() || undefined,
    quiesce_for: $('#bl-quiesce').value.trim() || undefined,
    approvers: $('#bl-approvers').value.trim() || undefined,
    conditions: $('#bl-conditions').value.trim() || undefined,
  };

  // Conditional validation
  if ((gate === 'WindowGate' || gate === 'FreezePeriod') && !body.window) {
    flash(`${gate} requires a window`, 'error'); $('#bl-window').focus(); return;
  }
  if (gate === 'QuiesceGate' && !body.quiesce_for) {
    flash('QuiesceGate requires quiesce_for', 'error'); $('#bl-quiesce').focus(); return;
  }
  if (gate === 'ApprovalGate' && !body.approvers) {
    flash('ApprovalGate requires approvers', 'error'); $('#bl-approvers').focus(); return;
  }

  for (const k of Object.keys(body)) if (body[k] === undefined) delete body[k];

  try {
    const created = await apiCreate('bylaws', body);
    state.data.bylaws.unshift(created);
    // Bust effective cache for affected node
    state.org.effective.delete(orgId);
    flash('Bylaw saved', 'success');
    resetBylawForm();
    renderBylawsTable();
  } catch (err) { flash(err.message, 'error'); }
}

function resetBylawForm() {
  for (const id of ['bl-priority', 'bl-desc', 'bl-window', 'bl-quiesce', 'bl-approvers', 'bl-conditions']) {
    $(`#${id}`).value = '';
  }
}

function initBylaws() {
  $('#bl-org').addEventListener('change', e => {
    state.bylaws.selectedOrg = e.target.value;
    renderBylawsTable();
  });
  $('#bl-gate').addEventListener('change', applyConditional);
  $('#bl-save').addEventListener('click', saveBylaw);
  $('#bl-reset').addEventListener('click', () => {
    resetBylawForm();
    $('#bl-gate').value = 'AutoGate';
    applyConditional();
  });
}

// ── Keyboard shortcuts ────────────────────────────────────────────────────────

function initKeyboard() {
  document.addEventListener('keydown', e => {
    const tag = (document.activeElement?.tagName || '').toLowerCase();
    const inField = tag === 'input' || tag === 'textarea' || tag === 'select';

    if (e.key === 'Escape') {
      clearFlash();
      if (state.screen === 'changes' && state.cr.open) { hideWizard(); return; }
      if (state.screen === 'org' && !$('#org-form-card').classList.contains('hidden')) { hideOrgForm(); return; }
      if (inField) document.activeElement.blur();
      return;
    }

    if (!inField) {
      if (e.key === '/') {
        e.preventDefault();
        const search = ({
          org: '#org-search',
          changes: '#cr-search',
        })[state.screen];
        if (search) $(search)?.focus();
        return;
      }
      if (e.key === 'n') {
        e.preventDefault();
        if (state.screen === 'org') showOrgForm();
        else if (state.screen === 'changes') newChangeRequest();
        else if (state.screen === 'bylaws') $('#bl-org')?.focus();
      }
    }
  });
}

// ── Bootstrap ─────────────────────────────────────────────────────────────────

function initTabs() {
  $$('.tab').forEach(t => t.addEventListener('click', () => setScreen(t.dataset.screen)));
}

async function init() {
  initTabs();
  initOrg();
  initChanges();
  initPlans();
  initBylaws();
  initKeyboard();
  initHashRouting();

  try {
    await loadAll();
  } catch (err) {
    flash(err.message, 'error', 6000);
  }
  setScreen(state.screen);
  updateFooterMeta();
}

init();
