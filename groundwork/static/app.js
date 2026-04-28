// groundwork — service catalog UI
// Vanilla JS, no build step, no framework.

const API = '/application/api';

let allApps = [];
let expandedId = null;

// ── API calls ────────────────────────────────────────────────────────────────

export async function fetchApps() {
  const res = await fetch(API);
  if (!res.ok) throw new Error(`GET ${API} → ${res.status}`);
  return res.json();
}

export async function registerApp(name) {
  const res = await fetch(API, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name }),
  });
  if (!res.ok) throw new Error(`POST ${API} → ${res.status}`);
  return res.json();
}

export async function updateApp(id, fields) {
  const res = await fetch(`${API}/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(fields),
  });
  if (!res.ok) throw new Error(`PUT ${API}/${id} → ${res.status}`);
  return res.json();
}

export async function deleteApp(id) {
  const res = await fetch(`${API}/${id}`, { method: 'DELETE' });
  if (!res.ok) throw new Error(`DELETE ${API}/${id} → ${res.status}`);
}

// ── Rendering ────────────────────────────────────────────────────────────────

export function renderList(apps, filter = '') {
  const list = document.getElementById('app-list');
  const needle = filter.trim().toLowerCase();
  const visible = needle
    ? apps.filter(a => (a.payload?.name ?? a.name ?? '').toLowerCase().includes(needle))
    : apps;

  list.innerHTML = '';

  if (visible.length === 0) {
    list.innerHTML = `<li class="empty-state">${filter ? 'no matches' : 'no apps yet — press n to register one'}</li>`;
    updateCount(apps.length, visible.length, filter);
    return;
  }

  for (const app of visible) {
    list.appendChild(buildRow(app));
  }

  updateCount(apps.length, visible.length, filter);
}

function buildRow(app) {
  const id = app.id;
  const payload = app.payload ?? {};
  const name = payload.name ?? app.name ?? id;

  const li = document.createElement('li');
  li.className = 'app-row' + (expandedId === id ? ' expanded' : '');
  li.dataset.id = id;

  const header = document.createElement('div');
  header.className = 'app-row-header';
  header.setAttribute('tabindex', '0');
  header.setAttribute('role', 'button');
  header.setAttribute('aria-expanded', String(expandedId === id));

  const icon = document.createElement('span');
  icon.className = 'expand-icon';

  const nameEl = document.createElement('span');
  nameEl.className = 'app-name';
  nameEl.textContent = name;

  const idEl = document.createElement('span');
  idEl.className = 'app-id';
  idEl.textContent = id;

  header.append(icon, nameEl, idEl);

  const detail = document.createElement('div');
  detail.className = 'app-detail';
  detail.innerHTML = buildDetailHTML(id, payload);

  li.append(header, detail);

  // expand / collapse
  const toggle = () => expandRow(id === expandedId ? null : id);
  header.addEventListener('click', toggle);
  header.addEventListener('keydown', e => {
    if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); toggle(); }
  });

  // save button inside detail
  detail.querySelector('.btn-save')?.addEventListener('click', () => saveRow(id, li));
  // delete button
  detail.querySelector('.btn-delete')?.addEventListener('click', () => confirmDelete(id));

  return li;
}

function buildDetailHTML(id, payload) {
  const v = (k) => escAttr(payload[k] ?? '');
  return `
    <div class="field-row">
      <label for="f-${id}-desc">description</label>
      <textarea id="f-${id}-desc" name="description" rows="2">${escText(payload.description ?? '')}</textarea>
    </div>
    <div class="field-row">
      <label for="f-${id}-repo">repo_url</label>
      <input id="f-${id}-repo" name="repo_url" type="text" value="${v('repo_url')}" />
    </div>
    <div class="field-row">
      <label for="f-${id}-stack">tech_stack</label>
      <input id="f-${id}-stack" name="tech_stack" type="text" value="${v('tech_stack')}" />
    </div>
    <div class="field-row">
      <label for="f-${id}-team">team</label>
      <input id="f-${id}-team" name="team" type="text" value="${v('team')}" />
    </div>
    <div class="detail-actions">
      <button class="btn-save primary">save</button>
      <button class="btn-delete danger">delete</button>
    </div>
  `;
}

function escAttr(s) {
  return String(s).replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;');
}

function escText(s) {
  return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;');
}

async function saveRow(id, li) {
  const payload = {};
  li.querySelectorAll('[name]').forEach(el => {
    payload[el.name] = el.value.trim();
  });
  // preserve name from original app
  const original = allApps.find(a => a.id === id);
  if (original) payload.name = original.payload?.name ?? original.name ?? payload.name;

  try {
    const updated = await updateApp(id, payload);
    // patch in-memory
    const idx = allApps.findIndex(a => a.id === id);
    if (idx !== -1) allApps[idx] = updated;
    setError('');
  } catch (err) {
    setError(err.message);
  }
}

async function confirmDelete(id) {
  if (!confirm('Delete this app?')) return;
  try {
    await deleteApp(id);
    allApps = allApps.filter(a => a.id !== id);
    if (expandedId === id) expandedId = null;
    renderList(allApps, document.getElementById('search').value);
    setError('');
  } catch (err) {
    setError(err.message);
  }
}

export function expandRow(id) {
  expandedId = id;
  document.querySelectorAll('.app-row').forEach(row => {
    const isTarget = row.dataset.id === id;
    row.classList.toggle('expanded', isTarget);
    const header = row.querySelector('.app-row-header');
    if (header) header.setAttribute('aria-expanded', String(isTarget));
  });
}

// ── Status bar ───────────────────────────────────────────────────────────────

function updateCount(total, shown, filter) {
  const el = document.getElementById('status-count');
  if (!el) return;
  if (filter && shown !== total) {
    el.textContent = `${shown} of ${total} app${total !== 1 ? 's' : ''}`;
  } else {
    el.textContent = `${total} app${total !== 1 ? 's' : ''}`;
  }
}

function setError(msg) {
  const el = document.getElementById('status-error');
  if (el) el.textContent = msg;
}

// ── New-app form ─────────────────────────────────────────────────────────────

function showNewForm() {
  const form = document.getElementById('new-app-form');
  const input = document.getElementById('new-app-name');
  form.classList.add('visible');
  input.value = '';
  input.focus();
}

function hideNewForm() {
  const form = document.getElementById('new-app-form');
  form.classList.remove('visible');
}

async function submitNewApp() {
  const input = document.getElementById('new-app-name');
  const name = input.value.trim();
  if (!name) { hideNewForm(); return; }

  try {
    const created = await registerApp(name);
    allApps.unshift(created);
    renderList(allApps, document.getElementById('search').value);
    hideNewForm();
    // auto-expand the new row
    expandRow(created.id);
    setError('');
  } catch (err) {
    setError(err.message);
  }
}

// ── Keyboard shortcuts ───────────────────────────────────────────────────────

function initKeyboard() {
  const search = document.getElementById('search');
  const newName = document.getElementById('new-app-name');

  document.addEventListener('keydown', e => {
    const tag = document.activeElement?.tagName?.toLowerCase();
    const inInput = tag === 'input' || tag === 'textarea';

    // Esc: cancel / close
    if (e.key === 'Escape') {
      if (document.getElementById('new-app-form').classList.contains('visible')) {
        hideNewForm();
        return;
      }
      if (expandedId !== null) {
        expandRow(null);
        return;
      }
      if (document.activeElement === search) {
        search.value = '';
        renderList(allApps, '');
      }
      return;
    }

    // / : focus search (when not in an input)
    if (e.key === '/' && !inInput) {
      e.preventDefault();
      search.focus();
      search.select();
      return;
    }

    // n : new app (when not in an input)
    if (e.key === 'n' && !inInput) {
      e.preventDefault();
      showNewForm();
      return;
    }
  });

  // search filters list inline
  search.addEventListener('input', () => {
    renderList(allApps, search.value);
  });

  // new-app form: Enter → submit, Esc → cancel
  newName.addEventListener('keydown', e => {
    if (e.key === 'Enter') { e.preventDefault(); submitNewApp(); }
    if (e.key === 'Escape') { e.preventDefault(); hideNewForm(); }
  });
}

// ── Bootstrap ────────────────────────────────────────────────────────────────

async function init() {
  initKeyboard();
  try {
    allApps = await fetchApps();
    renderList(allApps, '');
    setError('');
  } catch (err) {
    setError(err.message);
    document.getElementById('status-count').textContent = '0 apps';
  }
  document.getElementById('search').focus();
}

init();
