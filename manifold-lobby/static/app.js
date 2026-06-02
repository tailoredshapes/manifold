// Lobby frontend — vanilla ES modules, no build step.
//
// openModal (canonical promise-based pattern) is shared via manifold-ui.

import { openModal, loadManifoldConfig, crossLink } from './manifold-ui.js';

/**
 * @typedef {object} Advisory
 * @property {string} id
 * @property {string} kind
 * @property {string} [subject_type]
 * @property {string} [subject_id]
 * @property {string} [subject_name]
 * @property {string} severity
 * @property {string} state
 * @property {string} [rule]
 * @property {string} [explain]
 * @property {string} [raised_at]
 * @property {string} [acknowledged_at]
 * @property {string} [dismissed_at]
 * @property {string} [resolved_at]
 * @property {string} [dismiss_reason]
 * @property {string} [dismiss_note]
 * @property {string} [re_raise_count]
 *   - Stringified by meshql like the rest of the scalar fields.
 * @property {string} [last_action]
 * @property {string} [assignee]
 * @property {string} [programs]
 *   - Comma-separated list of program IDs this advisory has been
 *     associated with (via ProgramMembership), populated by lobby's
 *     resolver. Empty/absent means "no program tag".
 *
 * @typedef {object} Program
 * @property {string} id
 * @property {string} name
 * @property {string} [description]
 * @property {string} [leadership]
 * @property {string} [color]
 *
 * @typedef {object} LifecycleEntry
 * @property {string} id
 * @property {string} advisory_id
 * @property {string} at
 * @property {string} [actor_type]
 * @property {string} [actor_id]
 * @property {string} action
 * @property {string} [reason]
 * @property {string} [note]
 *
 * @typedef {object} Comment
 * @property {string} id
 * @property {string} [author]
 * @property {string} [body]
 * @property {string} [at]
 *
 * @typedef {object} SavedView
 * @property {string} id
 * @property {string} name
 * @property {AdvisoryFilter} filter
 *
 * @typedef {{ kind?: string, severity?: string, state?: string }} AdvisoryFilter
 */

const state = {
  route: 'inbox',
  /** @type {Advisory[]} */
  advisories: [],
  /** @type {Program[]} */
  programs: [],
  /** @type {LifecycleEntry[]} */
  lifecycle: [],
  /** @type {Comment[]} */
  comments: [],
  /** @type {AdvisoryFilter} */
  filter: { kind: '', severity: '', state: 'open' },
  /** @type {string | null} */
  view: null,           // active saved-view id
  /** @type {string | null} */
  selected: null,       // selected advisory id
};

/** @type {SavedView[]} */
const SAVED_VIEWS = [
  { id: 'cto',       name: 'CTO summary',     filter: { severity: 'critical', state: 'open' } },
  { id: 'ea',        name: 'EA: structural',  filter: { kind: 'CircularDependency,UndocumentedInterface', state: 'open' } },
  { id: 'open-warn', name: 'Open warnings',   filter: { severity: 'warn', state: 'open' } },
  { id: 'all',       name: 'All advisories',  filter: { state: '' } },
];

// ── Data fetch ────────────────────────────────────────────────────────────

/**
 * @param {string} path
 * @param {string} query
 * @returns {Promise<any>}
 */
async function gql(path, query) {
  const r = await fetch(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ query }),
  });
  const j = await r.json();
  if (j.errors && j.errors.length) throw new Error(JSON.stringify(j.errors));
  return j.data;
}

/** @returns {Promise<void>} */
async function loadAll() {
  const [adv, prog, life] = await Promise.all([
    gql('/advisory/graph', '{ getAll { id kind subject_type subject_id subject_name severity state rule explain raised_at acknowledged_at dismissed_at resolved_at dismiss_reason dismiss_note re_raise_count last_action assignee } }'),
    gql('/program/graph', '{ getAll { id name description leadership color } }'),
    gql('/lifecycle_entry/graph', '{ getAll { id advisory_id at actor_type actor_id action reason note } }'),
  ]);
  state.advisories = (adv?.getAll || []).sort((a, b) =>
    (a.raised_at || '').localeCompare(b.raised_at || '')
  );
  state.programs = prog?.getAll || [];
  state.lifecycle = (life?.getAll || []).sort((a, b) => (b.at || '').localeCompare(a.at || ''));
}

/** @param {string} advisoryId @returns {Promise<void>} */
async function loadComments(advisoryId) {
  const d = await gql('/comment/graph',
    `{ getByAdvisoryId(advisory_id: "${advisoryId}") { id author body at } }`);
  state.comments = (d?.getByAdvisoryId || []).sort((a, b) => (a.at || '').localeCompare(b.at || ''));
}

// ── Actions ───────────────────────────────────────────────────────────────

/**
 * @param {string} path
 * @param {Record<string, any>} [body]
 * @returns {Promise<Response>}
 */
async function post(path, body) {
  const r = await fetch(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body || {}),
  });
  if (!r.ok) throw new Error(`${r.status}: ${await r.text()}`);
  return r;
}

/** @param {string} id */
async function ack(id) {
  await post(`/advisory/${id}/acknowledge`, {});
  await refresh();
}

/** @type {Array<{code:string,label:string,help?:string}>} */
const DISMISS_REASONS = [
  { code: 'false-positive',        label: 'False positive',         help: 'The rule fired but the underlying concern isn\'t real.' },
  { code: 'accepted-risk',         label: 'Accepted risk',          help: 'Acknowledged, weighed, accepted by an owner.' },
  { code: 'deferred',              label: 'Deferred',               help: 'Real, but parked until a future window.' },
  { code: 'compensating-control',  label: 'Compensating control',   help: 'Mitigated by something elsewhere; rule is technically right but irrelevant.' },
  { code: 'other',                 label: 'Other',                  help: 'Use the note field to explain.' },
];

/** @param {string} id */
async function dismiss(id) {
  const adv = state.advisories.find(a => a.id === id);
  const choice = await openModal({
    title: 'Dismiss advisory',
    intro: adv ? `${adv.kind} on ${adv.subject_name || adv.subject_id}` : '',
    fields: [
      { name: 'reason',  type: 'radio',    label: 'Reason',     options: DISMISS_REASONS, required: true, default: 'false-positive' },
      { name: 'note',    type: 'textarea', label: 'Note (optional)', placeholder: 'Context for the audit trail…' },
    ],
    submit: 'Dismiss',
  });
  if (!choice) return;
  await post(`/advisory/${id}/dismiss`, { reason: choice.reason, note: choice.note || undefined });
  await refresh();
}
/** @param {string} id */
async function escalate(id) {
  const choice = await openModal({
    title: 'Escalate advisory',
    fields: [
      { name: 'to',   type: 'text',     label: 'Escalate to', placeholder: 'person id or role (e.g. director-of-release)', required: true },
      { name: 'note', type: 'textarea', label: 'Note (optional)' },
    ],
    submit: 'Escalate',
  });
  if (!choice) return;
  await post(`/advisory/${id}/escalate`, { to: choice.to, note: choice.note || undefined });
  await refresh();
}
/** @param {string} id */
async function assign(id) {
  const choice = await openModal({
    title: 'Assign advisory',
    fields: [{ name: 'assignee', type: 'text', label: 'Assignee', placeholder: 'person id', required: true }],
    submit: 'Assign',
  });
  if (!choice) return;
  await post(`/advisory/${id}/assign`, { assignee: choice.assignee });
  await refresh();
}
/** @param {string} id */
async function comment(id) {
  const choice = await openModal({
    title: 'Add comment',
    fields: [{ name: 'body', type: 'textarea', label: 'Comment', required: true }],
    submit: 'Post comment',
  });
  if (!choice) return;
  await post(`/advisory/${id}/comment`, { body: choice.body });
  await loadComments(id);
  render();
}

// ── Derived actions ───────────────────────────────────────────────────────

async function deriveNow() {
  await post('/_derive', {});
  await refresh();
}

/** @returns {Promise<void>} */
async function refresh() {
  await loadAll();
  if (state.selected) await loadComments(state.selected);
  render();
}

// ── Filtering / saved views ───────────────────────────────────────────────

/** @param {Advisory} a @returns {boolean} */
function matchesFilter(a) {
  const f = state.filter;
  if (f.state === 'open' && (a.state === 'resolved' || a.state === 'dismissed')) return false;
  if (f.state && f.state !== 'open' && a.state !== f.state) return false;
  if (f.severity && a.severity !== f.severity) return false;
  if (f.kind) {
    const kinds = f.kind.split(',').map(s => s.trim());
    if (!kinds.includes(a.kind)) return false;
  }
  return true;
}

/** @param {string} id */
function applySavedView(id) {
  const v = SAVED_VIEWS.find(v => v.id === id);
  if (!v) return;
  state.filter = { kind: v.filter.kind || '', severity: v.filter.severity || '', state: v.filter.state ?? '' };
  state.view = id;
  state.selected = null;
  state.route = 'inbox';
  window.location.hash = 'inbox';
  render();
}

function clearView() {
  state.view = null;
  state.filter = { kind: '', severity: '', state: 'open' };
  state.selected = null;
  render();
}

// ── Render ────────────────────────────────────────────────────────────────

function setActiveNav() {
  document.querySelectorAll('header nav a').forEach(a => {
    a.classList.toggle('active', a.dataset.route === state.route);
  });
}

function renderSidebar() {
  const sv = document.getElementById('saved-views');
  sv.innerHTML = '';
  for (const v of SAVED_VIEWS) {
    const n = state.advisories.filter(a => {
      const prev = state.filter; state.filter = v.filter;
      const m = matchesFilter(a); state.filter = prev; return m;
    }).length;
    const btn = document.createElement('button');
    btn.textContent = v.name;
    const cnt = document.createElement('span'); cnt.className = 'count'; cnt.textContent = String(n);
    btn.appendChild(cnt);
    btn.classList.toggle('active', state.view === v.id);
    btn.onclick = () => applySavedView(v.id);
    const li = document.createElement('li'); li.appendChild(btn); sv.appendChild(li);
  }

  const f = document.getElementById('filters');
  f.innerHTML = '';
  const kinds = [...new Set(state.advisories.map(a => a.kind))].sort();
  for (const k of kinds) {
    const n = state.advisories.filter(a => a.kind === k && (a.state !== 'resolved' && a.state !== 'dismissed')).length;
    const btn = document.createElement('button');
    btn.textContent = k;
    const cnt = document.createElement('span'); cnt.className = 'count'; cnt.textContent = String(n);
    btn.appendChild(cnt);
    btn.classList.toggle('active', state.filter.kind === k);
    btn.onclick = () => { state.filter = { kind: k, severity: '', state: 'open' }; state.view = null; state.selected = null; render(); };
    const li = document.createElement('li'); li.appendChild(btn); f.appendChild(li);
  }
}

/** @param {string | null | undefined} raisedAt @returns {string} */
function ageOf(raisedAt) {
  if (!raisedAt) return '—';
  const t = Date.parse(raisedAt);
  if (isNaN(t)) return raisedAt;
  const dms = Date.now() - t;
  const days = Math.floor(dms / 86400000);
  if (days > 0) return `${days}d`;
  const hrs = Math.floor(dms / 3600000);
  if (hrs > 0) return `${hrs}h`;
  const mins = Math.floor(dms / 60000);
  return `${mins}m`;
}

function renderInbox() {
  const c = document.getElementById('content');
  c.innerHTML = '';
  const h = document.createElement('h2'); h.textContent = 'Inbox';
  c.appendChild(h);
  const sub = document.createElement('p'); sub.className = 'subtitle';
  const filtered = state.advisories.filter(matchesFilter);
  sub.textContent = `${filtered.length} advisories${state.view ? ` — ${SAVED_VIEWS.find(v => v.id === state.view).name}` : ''}`;
  c.appendChild(sub);

  const bar = document.createElement('div'); bar.className = 'toolbar';
  bar.innerHTML = `
    <label>Severity:
      <select id="f-sev">
        <option value="">all</option><option value="critical">critical</option>
        <option value="warn">warn</option><option value="info">info</option>
      </select>
    </label>
    <label>State:
      <select id="f-state">
        <option value="open">open (default)</option>
        <option value="">all</option>
        <option value="raised">raised</option>
        <option value="acknowledged">acknowledged</option>
        <option value="dismissed">dismissed</option>
        <option value="resolved">resolved</option>
      </select>
    </label>
    <button id="b-derive" type="button">Derive now</button>
    ${state.view ? '<button id="b-clear" type="button">Clear view</button>' : ''}
  `;
  c.appendChild(bar);
  document.getElementById('f-sev').value = state.filter.severity;
  document.getElementById('f-state').value = state.filter.state;
  document.getElementById('f-sev').onchange = e => { state.filter.severity = e.target.value; state.view = null; render(); };
  document.getElementById('f-state').onchange = e => { state.filter.state = e.target.value; state.view = null; render(); };
  document.getElementById('b-derive').onclick = deriveNow;
  if (state.view) document.getElementById('b-clear').onclick = clearView;

  if (filtered.length === 0) {
    const p = document.createElement('p'); p.className = 'empty';
    p.textContent = 'No advisories match the current filter.';
    c.appendChild(p);
    return;
  }

  const ul = document.createElement('ul'); ul.className = 'advisory-list';
  for (const a of filtered) {
    const li = document.createElement('li');
    li.dataset.id = a.id;
    li.innerHTML = `
      <span class="sev ${a.severity}">${a.severity}</span>
      <span class="kind">${a.kind}</span>
      <span class="subject">${a.subject_name || a.subject_id}<span class="state">${a.state}${a.assignee ? ' — ' + a.assignee : ''}</span></span>
      <span class="age">${ageOf(a.raised_at)}</span>
      <p class="explain">${a.explain || ''}</p>
    `;
    li.onclick = () => { state.selected = a.id; loadComments(a.id).then(render); };
    ul.appendChild(li);
    if (state.selected === a.id) {
      const drawer = renderDrawer(a);
      const wrap = document.createElement('li'); wrap.style.cursor = 'default'; wrap.style.gridColumn = '1 / -1';
      wrap.appendChild(drawer); ul.appendChild(wrap);
    }
  }
  c.appendChild(ul);
}

/** @param {Advisory} a @returns {HTMLElement} */
function renderDrawer(a) {
  const drawer = document.createElement('div');
  drawer.className = 'drawer';
  const sourceLink = subjectSourceLink(a);
  drawer.innerHTML = `
    <h3>${a.kind} — ${a.subject_name || a.subject_id}</h3>
    <dl>
      <dt>severity</dt><dd>${a.severity}</dd>
      <dt>state</dt><dd>${a.state}${a.dismiss_reason ? ' (' + a.dismiss_reason + ')' : ''}</dd>
      <dt>rule</dt><dd>${a.rule}</dd>
      <dt>raised at</dt><dd>${a.raised_at || '—'}</dd>
      ${a.assignee ? `<dt>assignee</dt><dd>${a.assignee}</dd>` : ''}
      ${a.re_raise_count && a.re_raise_count !== '0' ? `<dt>re-raised</dt><dd>${a.re_raise_count}×</dd>` : ''}
      <dt>subject</dt><dd>${sourceLink}</dd>
    </dl>
    <p>${a.explain || ''}</p>
    <div class="actions">
      <button class="primary" data-act="ack"      ${a.state==='acknowledged'?'disabled':''}>Acknowledge</button>
      <button data-act="dismiss"  ${a.state==='dismissed'?'disabled':''}>Dismiss…</button>
      <button data-act="escalate">Escalate…</button>
      <button data-act="assign">Assign…</button>
      <button data-act="comment">Comment…</button>
    </div>
  `;
  drawer.querySelector('[data-act=ack]').onclick      = e => { e.stopPropagation(); ack(a.id); };
  drawer.querySelector('[data-act=dismiss]').onclick  = e => { e.stopPropagation(); dismiss(a.id); };
  drawer.querySelector('[data-act=escalate]').onclick = e => { e.stopPropagation(); escalate(a.id); };
  drawer.querySelector('[data-act=assign]').onclick   = e => { e.stopPropagation(); assign(a.id); };
  drawer.querySelector('[data-act=comment]').onclick  = e => { e.stopPropagation(); comment(a.id); };

  // Lifecycle for this advisory
  const lifecycle = state.lifecycle.filter(le => le.advisory_id === a.id);
  if (lifecycle.length) {
    const ld = document.createElement('div'); ld.className = 'lifecycle';
    ld.innerHTML = '<h4>Lifecycle</h4>';
    const ol = document.createElement('ol');
    for (const le of lifecycle) {
      const li = document.createElement('li');
      li.innerHTML = `<span style="color:var(--text-soft);font-family:var(--font-mono);font-size:11px">${le.at?.slice(0,19) || ''}</span> &nbsp; <b>${le.action}</b> by ${le.actor_id} ${le.reason ? '(' + le.reason + ')' : ''} ${le.note ? '— ' + le.note : ''}`;
      ol.appendChild(li);
    }
    ld.appendChild(ol);
    drawer.appendChild(ld);
  }

  // Comments
  if (state.comments.length) {
    const cd = document.createElement('div'); cd.className = 'lifecycle';
    cd.innerHTML = '<h4>Comments</h4>';
    const ol = document.createElement('ol');
    for (const cm of state.comments) {
      const li = document.createElement('li');
      li.innerHTML = `<b>${cm.author}</b> · ${cm.at?.slice(0,19) || ''}<br>${cm.body}`;
      ol.appendChild(li);
    }
    cd.appendChild(ol);
    drawer.appendChild(cd);
  }
  return drawer;
}

/** @param {Advisory} a @returns {string} */
function subjectSourceLink(a) {
  // Deep-link to the owning app's hash-route. crossLink resolves the target
  // app's public URL from /config.json, so the same code works whether apps
  // are deployed as subdomains or as path prefixes on one origin.
  const id = a.subject_id;
  const label = `${a.subject_name} →`;
  switch (a.subject_type) {
    case 'deployable':       return crossLink('groundwork', 'deployables', id, label);
    case 'service':          return crossLink('groundwork', 'services', id, label);
    case 'change_request':   return crossLink('cityhall', 'changes', id, label);
    case 'deployment_plan':  return crossLink('cityhall', 'plans', id, label);
    case 'test_environment': return crossLink('yard', 'environments', id, label);
    case 'team':             return crossLink('union', 'teams', id, label);
    case 'person':           return crossLink('union', 'people', id, label);
    default: return a.subject_name || a.subject_id;
  }
}

function renderPrograms() {
  const c = document.getElementById('content');
  c.innerHTML = '';
  c.appendChild(Object.assign(document.createElement('h2'), { textContent: 'Programs' }));
  const sub = document.createElement('p'); sub.className = 'subtitle';
  sub.textContent = `${state.programs.length} programs registered`;
  c.appendChild(sub);
  if (state.programs.length === 0) {
    const p = document.createElement('p'); p.className = 'empty';
    p.innerHTML = 'No programs yet. Programs are cross-cutting tags applied across deployables, change requests, and environments.<br>Create one via the MCP <code>create_program</code> tool or POST <code>/program/api</code>.';
    c.appendChild(p);
    return;
  }
  const ul = document.createElement('ul'); ul.className = 'program-list';
  for (const p of state.programs) {
    const advCount = state.advisories.filter(a =>
      (a.programs || '').split(',').includes(p.id) && a.state !== 'resolved' && a.state !== 'dismissed'
    ).length;
    const li = document.createElement('li');
    li.innerHTML = `<b>${p.name}</b><div class="desc">${p.description || ''}</div>
      <div style="color: var(--text-muted); font-size: 13px; margin-top: 4px;">${advCount} open advisories · ${p.leadership || 'no lead'}</div>`;
    ul.appendChild(li);
  }
  c.appendChild(ul);
}

function renderAudit() {
  const c = document.getElementById('content');
  c.innerHTML = '';
  c.appendChild(Object.assign(document.createElement('h2'), { textContent: 'Audit' }));
  const sub = document.createElement('p'); sub.className = 'subtitle';
  sub.textContent = `${state.lifecycle.length} lifecycle entries (most recent first)`;
  c.appendChild(sub);
  const ul = document.createElement('ul'); ul.className = 'audit-list';
  for (const le of state.lifecycle.slice(0, 200)) {
    const adv = state.advisories.find(a => a.id === le.advisory_id);
    const li = document.createElement('li');
    li.innerHTML = `
      <span class="at">${le.at?.slice(0,19) || ''}</span>
      <span class="action">${le.action}</span>
      <span>${adv ? (adv.kind + ' on ' + (adv.subject_name || adv.subject_id)) : le.advisory_id}</span>
      <span style="color:var(--text-soft);">${le.actor_id}</span>
    `;
    ul.appendChild(li);
  }
  c.appendChild(ul);
}

// ── Routing ───────────────────────────────────────────────────────────────

function render() {
  setActiveNav();
  renderSidebar();
  if (state.route === 'programs') renderPrograms();
  else if (state.route === 'audit') renderAudit();
  else renderInbox();
}

function routeFromHash() {
  const h = window.location.hash.slice(1).split('/')[0];
  state.route = ['inbox', 'programs', 'audit'].includes(h) ? h : 'inbox';
}

window.addEventListener('hashchange', () => { routeFromHash(); render(); });

(async function init() {
  routeFromHash();
  // Cross-app public URLs — needed before render so subjectSourceLink can
  // resolve deep-links to the owning app.
  await loadManifoldConfig();
  try {
    await loadAll();
  } catch (e) {
    document.getElementById('content').innerHTML = `<p class="empty">Failed to load: ${e.message}</p>`;
    return;
  }
  render();
})();
