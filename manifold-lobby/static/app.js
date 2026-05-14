// Lobby frontend — vanilla ES modules, no build step.

const state = {
  route: 'inbox',
  advisories: [],
  programs: [],
  lifecycle: [],
  comments: [],
  filter: { kind: '', severity: '', state: 'open' },
  view: null,           // active saved-view id
  selected: null,       // selected advisory id
};

const SAVED_VIEWS = [
  { id: 'cto',       name: 'CTO summary',     filter: { severity: 'critical', state: 'open' } },
  { id: 'ea',        name: 'EA: structural',  filter: { kind: 'CircularDependency,UndocumentedInterface', state: 'open' } },
  { id: 'open-warn', name: 'Open warnings',   filter: { severity: 'warn', state: 'open' } },
  { id: 'all',       name: 'All advisories',  filter: { state: '' } },
];

// ── Data fetch ────────────────────────────────────────────────────────────

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

async function loadComments(advisoryId) {
  const d = await gql('/comment/graph',
    `{ getByAdvisoryId(advisory_id: "${advisoryId}") { id author body at } }`);
  state.comments = (d?.getByAdvisoryId || []).sort((a, b) => (a.at || '').localeCompare(b.at || ''));
}

// ── Actions ───────────────────────────────────────────────────────────────

async function post(path, body) {
  const r = await fetch(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body || {}),
  });
  if (!r.ok) throw new Error(`${r.status}: ${await r.text()}`);
  return r;
}

async function ack(id) {
  await post(`/advisory/${id}/acknowledge`, {});
  await refresh();
}

const DISMISS_REASONS = [
  { code: 'false-positive',        label: 'False positive',         help: 'The rule fired but the underlying concern isn\'t real.' },
  { code: 'accepted-risk',         label: 'Accepted risk',          help: 'Acknowledged, weighed, accepted by an owner.' },
  { code: 'deferred',              label: 'Deferred',               help: 'Real, but parked until a future window.' },
  { code: 'compensating-control',  label: 'Compensating control',   help: 'Mitigated by something elsewhere; rule is technically right but irrelevant.' },
  { code: 'other',                 label: 'Other',                  help: 'Use the note field to explain.' },
];

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

// ── Modal ─────────────────────────────────────────────────────────────────

function openModal(spec) {
  return new Promise(resolve => {
    const backdrop = document.createElement('div');
    backdrop.className = 'modal-backdrop';
    const dialog = document.createElement('div');
    dialog.className = 'modal-dialog';
    dialog.setAttribute('role', 'dialog');
    dialog.setAttribute('aria-modal', 'true');
    dialog.setAttribute('aria-labelledby', 'modal-title');
    const close = (value) => { backdrop.remove(); document.removeEventListener('keydown', onKey); resolve(value); };
    const onKey = e => { if (e.key === 'Escape') close(null); };
    document.addEventListener('keydown', onKey);

    const form = document.createElement('form');
    form.innerHTML = `<h3 id="modal-title">${spec.title}</h3>` + (spec.intro ? `<p class="modal-intro">${spec.intro}</p>` : '');
    for (const f of spec.fields) {
      const wrap = document.createElement('div'); wrap.className = 'modal-field';
      const label = document.createElement('label'); label.textContent = f.label; wrap.appendChild(label);
      if (f.type === 'radio') {
        for (const opt of f.options) {
          const id = `r_${f.name}_${opt.code}`;
          const row = document.createElement('label'); row.className = 'modal-radio';
          row.innerHTML = `<input type="radio" name="${f.name}" value="${opt.code}" id="${id}" ${opt.code === f.default ? 'checked' : ''}>
            <span class="opt-label">${opt.label}</span>
            <span class="opt-help">${opt.help || ''}</span>`;
          wrap.appendChild(row);
        }
      } else if (f.type === 'textarea') {
        const t = document.createElement('textarea');
        t.name = f.name; t.placeholder = f.placeholder || ''; t.rows = 4;
        if (f.required) t.required = true;
        wrap.appendChild(t);
      } else {
        const i = document.createElement('input');
        i.type = 'text'; i.name = f.name; i.placeholder = f.placeholder || '';
        if (f.required) i.required = true;
        wrap.appendChild(i);
      }
      form.appendChild(wrap);
    }
    const actions = document.createElement('div'); actions.className = 'modal-actions';
    actions.innerHTML = `
      <button type="button" data-act="cancel">Cancel</button>
      <button type="submit" class="primary">${spec.submit || 'Submit'}</button>`;
    form.appendChild(actions);

    form.addEventListener('submit', e => {
      e.preventDefault();
      const data = new FormData(form);
      const out = {};
      for (const [k, v] of data.entries()) out[k] = v;
      // Validate required
      for (const f of spec.fields) {
        if (f.required && !out[f.name]) { return; }
      }
      close(out);
    });
    form.querySelector('[data-act=cancel]').addEventListener('click', () => close(null));
    backdrop.addEventListener('click', e => { if (e.target === backdrop) close(null); });

    dialog.appendChild(form);
    backdrop.appendChild(dialog);
    document.body.appendChild(backdrop);
    setTimeout(() => form.querySelector('input,textarea')?.focus(), 50);
  });
}
async function deriveNow() {
  await post('/_derive', {});
  await refresh();
}

async function refresh() {
  await loadAll();
  if (state.selected) await loadComments(state.selected);
  render();
}

// ── Filtering / saved views ───────────────────────────────────────────────

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
    const cnt = document.createElement('span'); cnt.className = 'count'; cnt.textContent = n;
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
    const cnt = document.createElement('span'); cnt.className = 'count'; cnt.textContent = n;
    btn.appendChild(cnt);
    btn.classList.toggle('active', state.filter.kind === k);
    btn.onclick = () => { state.filter = { kind: k, severity: '', state: 'open' }; state.view = null; state.selected = null; render(); };
    const li = document.createElement('li'); li.appendChild(btn); f.appendChild(li);
  }
}

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

function subjectSourceLink(a) {
  // Deep-link to the owning app's hash-route. Tildarc domains in prod; in
  // dev these are the docker-compose ports.
  const id = a.subject_id;
  switch (a.subject_type) {
    case 'deployable':       return `<a href="https://groundwork.tildarc.com/#deployables/${id}">${a.subject_name} →</a>`;
    case 'service':          return `<a href="https://groundwork.tildarc.com/#services/${id}">${a.subject_name} →</a>`;
    case 'change_request':   return `<a href="https://cityhall.tildarc.com/#changes/${id}">${a.subject_name} →</a>`;
    case 'deployment_plan':  return `<a href="https://cityhall.tildarc.com/#plans/${id}">${a.subject_name} →</a>`;
    case 'test_environment': return `<a href="https://yard.tildarc.com/#environments/${id}">${a.subject_name} →</a>`;
    case 'team':             return `<a href="https://union.tildarc.com/#teams/${id}">${a.subject_name} →</a>`;
    case 'person':           return `<a href="https://union.tildarc.com/#people/${id}">${a.subject_name} →</a>`;
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
  try {
    await loadAll();
  } catch (e) {
    document.getElementById('content').innerHTML = `<p class="empty">Failed to load: ${e.message}</p>`;
    return;
  }
  render();
})();
