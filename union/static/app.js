// Union — foreman dashboard.
// Vanilla JS ES module, no build step, no framework.
// Screens: Teams · Staffing · People · Work (kanban).

// ── Constants ─────────────────────────────────────────────────────────────────

const TEAM_KINDS = ['product', 'platform', 'security', 'domain', 'enterprise', 'infrastructure', 'support'];
const WORK_ORDER_STATUSES = ['proposed', 'in_progress', 'blocked', 'done', 'cancelled'];
const WORK_ORDER_PRIORITIES = ['low', 'medium', 'high', 'urgent'];
const WORK_ORDER_POINTS = [1, 2, 3, 5, 8];
const OPEN_STATUSES = new Set(['proposed', 'in_progress', 'blocked']);

const KANBAN_COLUMNS = [
  { status: 'proposed',    label: 'To Do' },
  { status: 'in_progress', label: 'In Progress' },
  { status: 'blocked',     label: 'Blocked' },
  { status: 'done',        label: 'Done' },
];

const SCREENS = ['teams', 'staffing', 'people', 'work'];

const SCREEN_TITLES = {
  teams:    'Teams',
  staffing: 'Staffing',
  people:   'People',
  work:     'Work',
};

// ── State ─────────────────────────────────────────────────────────────────────

const state = {
  screen: 'teams',
  filter: '',
  data: { people: [], teams: [], members: [], workOrders: [] },
  expandedTeamId: null,
  modalOpen: false,
  modalKind: null, // 'team' | 'person' | 'member' | 'work_order'
  config: {},     // populated from /config.json: cross-app public URLs
};

// ── Cross-app linking ────────────────────────────────────────────────────────

async function loadConfig() {
  try {
    const res = await fetch('/config.json');
    if (res.ok) state.config = await res.json();
  } catch {
    state.config = {};
  }
}

// Build a cross-app anchor pointing at <base>#<screen>[/<id>], or fall back
// to plain escaped text when the target app's public URL is unknown. The
// receiving end may not yet honour the id segment — that's deferred.
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

const ENDPOINTS = {
  people:     '/person/api',
  teams:      '/team/api',
  members:    '/team_member/api',
  workOrders: '/work_order/api',
};

async function loadAll() {
  const [people, teams, members, workOrders] = await Promise.all([
    gqlQuery('/person/graph', '{ getAll { id name contact role } }').then(d => d.getAll),
    gqlQuery('/team/graph', '{ getAll { id name kind description } }').then(d => d.getAll),
    gqlQuery('/team_member/graph', '{ getAll { id person_id team_id role } }').then(d => d.getAll),
    gqlQuery(
      '/work_order/graph',
      '{ getAll { id team_id summary deployable_id deployable { id name } change_request_id status priority story_points } }'
    ).then(d => d.getAll),
  ]);
  state.data.people     = Array.isArray(people)     ? people     : [];
  state.data.teams      = Array.isArray(teams)      ? teams      : [];
  state.data.members    = Array.isArray(members)    ? members    : [];
  state.data.workOrders = Array.isArray(workOrders) ? workOrders : [];
}

async function createRecord(endpoint, payload) {
  return apiFetch(endpoint, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
}

async function updateRecord(endpoint, id, payload) {
  return apiFetch(`${endpoint}/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
}

// ── Utilities ─────────────────────────────────────────────────────────────────

function esc(s) {
  return String(s ?? '')
    .replace(/&/g, '&amp;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

function teamById(id)   { return state.data.teams.find(t => t.id === id); }
function personById(id) { return state.data.people.find(p => p.id === id); }

function teamName(id) {
  const t = teamById(id);
  return t?.name || id || '—';
}

function personName(id) {
  const p = personById(id);
  return p?.name || id || '—';
}

function membersForTeam(teamId) {
  return state.data.members.filter(m => m.team_id === teamId);
}

function teamsForPerson(personId) {
  const teamIds = state.data.members
    .filter(m => m.person_id === personId)
    .map(m => m.team_id)
    .filter(Boolean);
  return teamIds.map(id => teamById(id)).filter(Boolean);
}

function openWorkOrdersForTeam(teamId) {
  return state.data.workOrders.filter(wo =>
    wo.team_id === teamId &&
    OPEN_STATUSES.has(wo.status)
  );
}

function sumStoryPoints(workOrders) {
  return workOrders.reduce((s, wo) => s + (Number.isFinite(wo.story_points) ? wo.story_points : 0), 0);
}

// ── Status strip ──────────────────────────────────────────────────────────────

function setError(msg) {
  const el = document.getElementById('status-strip');
  if (!msg) { el.className = ''; el.textContent = ''; el.style.display = 'none'; return; }
  el.className = 'error';
  el.textContent = msg;
}

function setInfo(msg, ttl = 2000) {
  const el = document.getElementById('status-strip');
  el.className = 'info';
  el.textContent = msg;
  setTimeout(() => {
    if (el.textContent === msg) { el.className = ''; el.textContent = ''; el.style.display = 'none'; }
  }, ttl);
}

// ── Footer meta ───────────────────────────────────────────────────────────────

function updateFooterMeta() {
  const d = state.data;
  document.getElementById('footer-meta').textContent =
    `${d.teams.length} teams · ${d.people.length} people · ${d.workOrders.length} work orders`;
}

// ── Rendering: Teams screen ───────────────────────────────────────────────────

function capacityState(memberCount, openCount) {
  if (memberCount === 0) return { label: 'unstaffed', color: '#6b7280', ratio: 0, unstaffed: true };
  const ratio = openCount / memberCount;
  let color = '#16a34a';
  if (ratio > 5) color = '#dc2626';
  else if (ratio > 3) color = '#f59e0b';
  return { label: `${openCount}/${memberCount}`, color, ratio, unstaffed: false };
}

function donutSvg(state) {
  // 56x56 donut. Stroke arc = ratio of capacity; max ratio capped at 5+ visually.
  const size = 56, r = 22, cx = 28, cy = 28;
  const circ = 2 * Math.PI * r;
  if (state.unstaffed) {
    return `
      <svg class="donut unstaffed" viewBox="0 0 ${size} ${size}" aria-hidden="true">
        <circle cx="${cx}" cy="${cy}" r="${r}" fill="none" stroke="#e5e7eb" stroke-width="5"/>
        <text x="${cx}" y="${cy}">unstaffed</text>
      </svg>`;
  }
  const display = Math.min(state.ratio / 5, 1);
  const dash = circ * display;
  const rest = circ - dash;
  return `
    <svg class="donut" viewBox="0 0 ${size} ${size}" aria-hidden="true">
      <circle cx="${cx}" cy="${cy}" r="${r}" fill="none" stroke="#e5e7eb" stroke-width="5"/>
      <circle cx="${cx}" cy="${cy}" r="${r}" fill="none"
              stroke="${state.color}" stroke-width="5" stroke-linecap="round"
              stroke-dasharray="${dash} ${rest}"
              transform="rotate(-90 ${cx} ${cy})"/>
      <text x="${cx}" y="${cy}">${esc(state.label)}</text>
    </svg>`;
}

function renderTeams() {
  const grid = document.getElementById('teams-grid');
  const needle = state.filter.trim().toLowerCase();

  let teams = state.data.teams.slice();
  if (needle) {
    teams = teams.filter(t => {
      return (t.name || '').toLowerCase().includes(needle) ||
             (t.kind || '').toLowerCase().includes(needle) ||
             (t.description || '').toLowerCase().includes(needle);
    });
  }

  if (teams.length === 0) {
    grid.innerHTML = needle
      ? `<div class="empty">No teams match.</div>`
      : `<div class="empty-card">
           <span class="empty-mark">§</span>
           <h3>No teams yet</h3>
           <p class="lede">A team is a unit of accountability — not a calendar invite.</p>
           <p class="hint">Press <kbd>n</kbd> to register the first one.</p>
         </div>`;
    return;
  }

  grid.innerHTML = teams.map(t => renderTeamCard(t)).join('');

  // wire up clicks
  grid.querySelectorAll('.team-card').forEach(el => {
    el.addEventListener('click', e => {
      // ignore clicks inside expanded detail body
      if (e.target.closest('.team-detail')) return;
      const id = el.dataset.id;
      state.expandedTeamId = state.expandedTeamId === id ? null : id;
      renderTeams();
    });
  });
}

function renderTeamCard(t) {
  const members = membersForTeam(t.id);
  const open = openWorkOrdersForTeam(t.id);
  const pointsInFlight = sumStoryPoints(open);
  const cap = capacityState(members.length, open.length);
  const expanded = state.expandedTeamId === t.id;

  const memberItems = members.map(m => {
    const pname = personName(m.person_id);
    const role = m.role ? ` · ${esc(m.role)}` : '';
    // Intra-app link to People screen — selection-level routing not yet wired,
    // so for now the link just lands on the list.
    const nameHtml = m.person_id ? intraLink('people', m.person_id, pname) : esc(pname);
    return `<li>${nameHtml}${role}</li>`;
  }).join('') || '<li style="color:var(--muted)">No members yet</li>';

  const detail = expanded ? `
    <div class="team-detail">
      <h4>Members (${members.length})</h4>
      <ul class="member-list">${memberItems}</ul>
      <h4>Open work orders (${open.length})</h4>
      ${open.length === 0
        ? '<p style="color:var(--muted);font-size:13px;">No open work.</p>'
        : '<ul class="member-list">' + open.map(w => {
            const pr = w.priority || '';
            const cls = pr === 'urgent' ? 'danger' : pr === 'high' ? 'warn' : pr === 'medium' ? 'primary' : '';
            return `<li><span class="pill ${cls}" style="margin-right:6px">${esc(w.status || '')}</span>${esc(w.summary || '')}</li>`;
          }).join('') + '</ul>'
      }
      ${t.description ? `<p style="margin-top:12px;font-size:13px;color:var(--muted);">${esc(t.description)}</p>` : ''}
    </div>
  ` : '';

  return `
    <div class="team-card${expanded ? ' expanded' : ''}" data-id="${esc(t.id)}">
      <div class="team-card-top">
        <div class="name-block">
          <div class="team-name">${esc(t.name || 'Untitled team')}</div>
          <div class="team-meta">
            ${t.kind ? `<span class="pill">${esc(t.kind)}</span>` : ''}
          </div>
        </div>
        ${donutSvg(cap)}
      </div>
      <div class="team-card-foot">
        <span class="stat"><strong>${members.length}</strong> ${members.length === 1 ? 'member' : 'members'}</span>
        <span class="stat"><strong>${open.length}</strong> open</span>
        <span class="stat"><strong>${pointsInFlight}</strong> pts in flight</span>
      </div>
      ${detail}
    </div>
  `;
}

// ── Rendering: Staffing timeline ──────────────────────────────────────────────

function nextSixMonths() {
  const now = new Date();
  const months = [];
  for (let i = 0; i < 6; i++) {
    const start = new Date(now.getFullYear(), now.getMonth() + i, 1);
    const end = new Date(now.getFullYear(), now.getMonth() + i + 1, 0); // last day
    months.push({
      year: start.getFullYear(),
      month: start.getMonth(),
      label: start.toLocaleString('en-US', { month: 'short' }),
      // Compact date range — e.g. "1 → 31". Year is shown separately so
      // we can collapse same-year repetition in the header strip.
      range: `${start.getDate()}–${end.getDate()}`,
    });
  }
  return months;
}

// We do not have due-date fields on WorkOrder, so we deterministically distribute
// open work across the next 6 months by hashing the WO id. Stable across refreshes.
function bucketForWorkOrder(woId) {
  let h = 0;
  for (let i = 0; i < woId.length; i++) {
    h = ((h << 5) - h + woId.charCodeAt(i)) | 0;
  }
  return Math.abs(h) % 6;
}

function renderStaffing() {
  const root = document.getElementById('staffing-timeline');
  const months = nextSixMonths();

  let teams = state.data.teams.slice().sort((a, b) =>
    (a.name || '').localeCompare(b.name || '')
  );

  const needle = state.filter.trim().toLowerCase();
  if (needle) {
    teams = teams.filter(t => (t.name || '').toLowerCase().includes(needle));
  }

  // Team-id → team-name lookup for tooltips. Built from the full team set
  // (not the filtered list) so dots referencing a hidden team still resolve.
  const teamNameById = new Map(
    state.data.teams.map(t => [t.id, t.name || 'Untitled team'])
  );

  // Filter work orders to proposed/blocked.
  const woByTeamBucket = new Map();
  for (const wo of state.data.workOrders) {
    const status = wo.status;
    if (status !== 'proposed' && status !== 'blocked') continue;
    const tid = wo.team_id;
    if (!tid) continue;
    const bucket = bucketForWorkOrder(wo.id || '');
    const key = `${tid}::${bucket}`;
    if (!woByTeamBucket.has(key)) woByTeamBucket.set(key, []);
    woByTeamBucket.get(key).push(wo);
  }

  if (teams.length === 0) {
    root.innerHTML = needle
      ? `<div class="empty">No teams match.</div>`
      : `<div class="empty-card">
           <span class="empty-mark">§</span>
           <h3>Nothing scheduled</h3>
           <p class="lede">Six months of swimlanes await your first work order.</p>
           <p class="hint">Press <kbd>n</kbd> to register the first one.</p>
         </div>`;
    return;
  }

  // Header strip — month name on top, year + day-range underneath. Year is
  // only repeated on the first month of each calendar year so the strip
  // stays uncluttered for a same-year run.
  let lastYearShown = null;
  const headerCols = months.map(m => {
    const showYear = m.year !== lastYearShown;
    lastYearShown = m.year;
    const sub = showYear ? `${m.year} · ${m.range}` : m.range;
    return `<div>
        <span class="month-label">${esc(m.label)}</span>
        <span class="month-range">${esc(sub)}</span>
      </div>`;
  }).join('');

  const lanes = teams.map(t => {
    const teamName = t.name || 'Untitled team';
    const cells = months.map((_, i) => {
      const dots = (woByTeamBucket.get(`${t.id}::${i}`) || []).map(wo => {
        const pr = wo.priority || 'low';
        const summary = wo.summary || '(no summary)';
        const dep = wo.deployable?.name || wo.deployable_id;
        const cr  = wo.change_request_id;
        let ariaLabel =
          `Open work order: ${summary}. Team ${teamName}. ` +
          `${wo.status || 'proposed'}, ${pr} priority.`;
        if (dep) ariaLabel += ` Deployable ${dep}.`;
        if (cr)  ariaLabel += ` Change request ${cr}.`;
        return (
          `<button type="button" class="dot ${esc(pr)}"` +
          ` data-wo-id="${esc(wo.id || '')}"` +
          ` aria-label="${esc(ariaLabel)}"></button>`
        );
      }).join('');
      return `<div class="lane-cell">${dots}</div>`;
    }).join('');
    return `
      <div class="swimlane">
        <div class="lane-label">${esc(teamName)}</div>
        ${cells}
      </div>`;
  }).join('');

  root.innerHTML = `
    <div class="timeline">
      <div class="timeline-header">
        <div>Team</div>${headerCols}
      </div>
      ${lanes}
    </div>
  `;

  // Tooltip + click wiring. The tooltip is a single shared #tooltip div
  // (already in the DOM) — we just rewrite its innerHTML on enter/focus.
  const tt = document.getElementById('tooltip');
  const woById = new Map(state.data.workOrders.map(wo => [wo.id, wo]));

  const showTipFor = (btn, anchorEvent) => {
    const wo = woById.get(btn.dataset.woId);
    if (!wo) return;
    const teamName = teamNameById.get(wo.team_id) || '—';
    const pr = wo.priority || 'low';
    const status = wo.status || 'proposed';
    const dep = wo.deployable?.name || wo.deployable_id;
    const cr = wo.change_request_id;
    const metas = [
      `Team: ${teamName}`,
      `${status} · ${pr} priority`,
    ];
    if (dep) metas.push(`Deployable: ${dep}`);
    if (cr)  metas.push(`Change request: ${cr}`);
    tt.innerHTML =
      `<div class="tip-summary">${esc(wo.summary || '(no summary)')}</div>` +
      metas.map(m => `<div class="tip-meta">${esc(m)}</div>`).join('');
    tt.style.display = 'block';
    positionTip(tt, btn, anchorEvent);
  };
  const hideTip = () => { tt.style.display = 'none'; };

  root.querySelectorAll('button.dot').forEach(btn => {
    btn.addEventListener('mouseenter', e => showTipFor(btn, e));
    btn.addEventListener('mousemove', e => positionTip(tt, btn, e));
    btn.addEventListener('mouseleave', hideTip);
    // Keyboard parity — surface the tooltip on focus, hide on blur.
    btn.addEventListener('focus', () => showTipFor(btn, null));
    btn.addEventListener('blur', hideTip);
    btn.addEventListener('click', () => {
      hideTip();
      // Match the intraLink convention (#<screen>/<id>) — the hash router
      // resolves the screen prefix today, and selection-level routing on
      // the Work tab will pick up the id segment when it lands. Bonus:
      // shareable URLs and clean back/forward stack entries.
      location.hash = '#work/' + encodeURIComponent(btn.dataset.woId);
    });
  });
}

// Anchor the tooltip near the pointer (or, when invoked without a pointer
// event — e.g. keyboard focus — near the element's bounding box).
function positionTip(tt, anchorEl, ev) {
  if (ev && typeof ev.clientX === 'number') {
    tt.style.left = (ev.clientX + 12) + 'px';
    tt.style.top  = (ev.clientY + 12) + 'px';
    return;
  }
  const r = anchorEl.getBoundingClientRect();
  tt.style.left = (r.right + 8) + 'px';
  tt.style.top  = (r.top) + 'px';
}

// ── Rendering: People ─────────────────────────────────────────────────────────

function availabilityFor(personId) {
  const teamIds = new Set(
    state.data.members
      .filter(m => m.person_id === personId)
      .map(m => m.team_id)
  );
  let open = 0;
  for (const wo of state.data.workOrders) {
    if (OPEN_STATUSES.has(wo.status) && teamIds.has(wo.team_id)) {
      open++;
    }
  }
  if (open === 0) return { label: 'available', cls: 'success', count: open };
  if (open <= 3)  return { label: 'stretched', cls: 'warn',    count: open };
  return            { label: 'overcommitted', cls: 'danger',  count: open };
}

function renderPeople() {
  const grid = document.getElementById('people-grid');
  const needle = state.filter.trim().toLowerCase();

  let people = state.data.people.slice();
  if (needle) {
    people = people.filter(p => {
      return (p.name || '').toLowerCase().includes(needle) ||
             (p.role || '').toLowerCase().includes(needle) ||
             (p.contact || '').toLowerCase().includes(needle);
    });
  }

  if (people.length === 0) {
    grid.innerHTML = needle
      ? `<div class="empty">No people match.</div>`
      : `<div class="empty-card">
           <span class="empty-mark">§</span>
           <h3>No people yet</h3>
           <p class="lede">Catalogue the people before you catalogue the work they do.</p>
           <p class="hint">Press <kbd>n</kbd> to register the first one.</p>
         </div>`;
    return;
  }

  grid.innerHTML = people.map(p => {
    const teams = teamsForPerson(p.id);
    const avail = availabilityFor(p.id);
    const teamPills = teams.length > 0
      ? teams.map(t => `<span class="pill">${esc(t.name || '')}</span>`).join('')
      : '<span style="color:var(--muted);font-size:12px;">No team</span>';
    return `
      <div class="person-card">
        <div class="person-name">${esc(p.name || 'Unnamed')}</div>
        <div class="person-role">${esc(p.role || '—')}</div>
        <div class="person-contact">${esc(p.contact || '')}</div>
        <div class="person-teams">${teamPills}</div>
        <div class="person-foot">
          <span class="pill ${avail.cls}">${esc(avail.label)}</span>
          <span style="font-size:12px;color:var(--muted);">${avail.count} open</span>
        </div>
      </div>
    `;
  }).join('');
}

// ── Rendering: Kanban ─────────────────────────────────────────────────────────

function renderKanban() {
  const root = document.getElementById('kanban');
  const needle = state.filter.trim().toLowerCase();

  let workOrders = state.data.workOrders.slice();
  if (needle) {
    workOrders = workOrders.filter(w => {
      return (w.summary || '').toLowerCase().includes(needle) ||
             teamName(w.team_id).toLowerCase().includes(needle) ||
             (w.priority || '').toLowerCase().includes(needle) ||
             (w.deployable_id || '').toLowerCase().includes(needle);
    });
  }

  // Global empty state: no work orders at all (independent of filter).
  if (state.data.workOrders.length === 0) {
    root.innerHTML = `
      <div class="empty-card" style="grid-column: 1 / -1;">
        <span class="empty-mark">§</span>
        <h3>No work orders yet</h3>
        <p class="lede">A board is only as honest as the work pinned to it.</p>
        <p class="hint">Press <kbd>n</kbd> to register the first one.</p>
      </div>`;
    return;
  }

  const byStatus = {};
  for (const col of KANBAN_COLUMNS) byStatus[col.status] = [];
  for (const wo of workOrders) {
    const s = wo.status;
    if (byStatus[s]) byStatus[s].push(wo);
  }

  root.innerHTML = KANBAN_COLUMNS.map(col => {
    const list = byStatus[col.status];
    const points = sumStoryPoints(list);
    const cards = list.map(woCardHtml).join('') ||
      '<div class="kanban-empty">— nothing here —</div>';
    return `
      <div class="kanban-col" data-status="${esc(col.status)}">
        <div class="kanban-col-header">
          <h3>${esc(col.label)} <span class="kanban-col-points">· ${points} pts</span></h3>
          <span class="kanban-col-count">${list.length}</span>
        </div>
        <div class="kanban-list" data-status="${esc(col.status)}">
          ${cards}
        </div>
      </div>`;
  }).join('');

  wireKanbanDnd();
}

function woCardHtml(wo) {
  const pr = wo.priority || '';
  const cls = pr === 'urgent' ? 'danger' : pr === 'high' ? 'warn' : pr === 'medium' ? 'primary' : '';
  const depLabel = wo.deployable?.name || wo.deployable_id;
  // Cross-app link to Groundwork — deployable detail lives there. Receiving
  // end may not yet focus the entity; the URL is still informative.
  const depHtml = depLabel && wo.deployable_id
    ? crossLink('groundwork', 'deployables', wo.deployable_id, depLabel)
    : (depLabel ? esc(depLabel) : '');
  const dep = depLabel ? `<div class="wo-deployable">deployable: ${depHtml}</div>` : '';
  const crHtml = wo.change_request_id
    ? `<div class="wo-deployable">change: ${crossLink('cityhall', 'changes', wo.change_request_id, wo.change_request_id.slice(0, 8))}</div>`
    : '';
  const pts = Number.isFinite(wo.story_points)
    ? `<span class="pill pts" title="story points">${wo.story_points} pts</span>`
    : '';
  return `
    <div class="wo-card" draggable="true" data-id="${esc(wo.id)}">
      <div class="wo-summary">${esc(wo.summary || '(no summary)')}</div>
      <div class="wo-meta">
        <span class="wo-team">${esc(teamName(wo.team_id))}</span>
        ${pr ? `<span class="pill ${cls}">${esc(pr)}</span>` : ''}
        ${pts}
      </div>
      ${dep}
      ${crHtml}
    </div>
  `;
}

function wireKanbanDnd() {
  const root = document.getElementById('kanban');
  let dragId = null;

  root.querySelectorAll('.wo-card').forEach(card => {
    card.addEventListener('dragstart', e => {
      dragId = card.dataset.id;
      card.classList.add('dragging');
      e.dataTransfer.effectAllowed = 'move';
      // For Firefox compatibility
      try { e.dataTransfer.setData('text/plain', dragId); } catch (_) {}
    });
    card.addEventListener('dragend', () => {
      card.classList.remove('dragging');
      dragId = null;
    });
  });

  root.querySelectorAll('.kanban-col').forEach(col => {
    col.addEventListener('dragover', e => {
      e.preventDefault();
      e.dataTransfer.dropEffect = 'move';
      col.classList.add('drag-over');
    });
    col.addEventListener('dragleave', e => {
      // Only remove when leaving column entirely
      if (!col.contains(e.relatedTarget)) col.classList.remove('drag-over');
    });
    col.addEventListener('drop', async e => {
      e.preventDefault();
      col.classList.remove('drag-over');
      const id = dragId || e.dataTransfer.getData('text/plain');
      const newStatus = col.dataset.status;
      if (!id || !newStatus) return;
      await moveWorkOrder(id, newStatus);
    });
  });
}

async function moveWorkOrder(id, newStatus) {
  const idx = state.data.workOrders.findIndex(w => w.id === id);
  if (idx === -1) return;
  const wo = state.data.workOrders[idx];
  if (wo.status === newStatus) return;

  const payload = { ...wo, status: newStatus };
  // Drop federated read-side fields — they're hydrated by /graph and must not
  // be written back into the REST record.
  delete payload.deployable;
  delete payload.change_request;
  // Drop empty optional strings to avoid stomping enums.
  for (const k of Object.keys(payload)) {
    if (payload[k] === '' || payload[k] == null) delete payload[k];
  }
  // Always re-set status (in case it was cleared above).
  payload.status = newStatus;

  try {
    await updateRecord(ENDPOINTS.workOrders, id, payload);
    // Refetch via /graph so federated fields (deployable, change_request)
    // stay accurate; in-place state mutation would silently lose them.
    await loadAll();
    setError('');
    setInfo(`Moved to ${newStatus}`);
    renderKanban();
    updateFooterMeta();
  } catch (err) {
    setError(err.message);
  }
}

// ── Modal: new record ─────────────────────────────────────────────────────────

const MODAL_FORMS = {
  team: {
    title: 'New team',
    endpoint: ENDPOINTS.teams,
    fields: [
      { name: 'name', label: 'Name', type: 'text', required: true },
      { name: 'kind', label: 'Kind', type: 'select', required: true, options: TEAM_KINDS },
      { name: 'description', label: 'Description', type: 'textarea' },
    ],
  },
  person: {
    title: 'New person',
    endpoint: ENDPOINTS.people,
    fields: [
      { name: 'name', label: 'Name', type: 'text', required: true },
      { name: 'contact', label: 'Contact', type: 'text' },
      { name: 'role', label: 'Role', type: 'text' },
    ],
  },
  member: {
    title: 'New team member',
    endpoint: ENDPOINTS.members,
    fields: [
      { name: 'person_id', label: 'Person', type: 'ref', required: true, source: () => state.data.people, labelOf: p => p.name || p.id },
      { name: 'team_id', label: 'Team', type: 'ref', required: true, source: () => state.data.teams, labelOf: t => t.name || t.id },
      { name: 'role', label: 'Role on team', type: 'text' },
    ],
  },
  work_order: {
    title: 'New work order',
    endpoint: ENDPOINTS.workOrders,
    fields: [
      { name: 'team_id', label: 'Team', type: 'ref', required: true, source: () => state.data.teams, labelOf: t => t.name || t.id },
      { name: 'summary', label: 'Summary', type: 'text', required: true },
      { name: 'status', label: 'Status', type: 'select', options: WORK_ORDER_STATUSES, default: 'proposed' },
      { name: 'priority', label: 'Priority', type: 'select', options: WORK_ORDER_PRIORITIES, default: 'medium' },
      { name: 'deployable_id', label: 'Deployable id', type: 'text' },
      { name: 'change_request_id', label: 'Change request id', type: 'text' },
      { name: 'story_points', label: 'Story points', type: 'select', options: WORK_ORDER_POINTS, cast: 'integer' },
    ],
  },
};

function modalKindForCurrentScreen() {
  switch (state.screen) {
    case 'teams':    return 'team';
    case 'staffing': return 'work_order';
    case 'people':   return 'person';
    case 'work':     return 'work_order';
    default:         return 'team';
  }
}

function openModal(kind) {
  const cfg = MODAL_FORMS[kind];
  if (!cfg) return;

  state.modalKind = kind;
  state.modalOpen = true;

  document.getElementById('modal-title').textContent = cfg.title;
  const fieldsEl = document.getElementById('modal-fields');
  fieldsEl.innerHTML = cfg.fields.map(f => renderModalField(f)).join('');

  document.getElementById('modal-backdrop').classList.add('visible');

  // Focus first input
  const first = fieldsEl.querySelector('input, select, textarea');
  if (first) first.focus();
}

function renderModalField(f) {
  const req = f.required ? ' <span class="req">*</span>' : '';
  if (f.type === 'select') {
    const opts = ['<option value="">—</option>'].concat(
      f.options.map(o => `<option value="${esc(o)}"${o === f.default ? ' selected' : ''}>${esc(o)}</option>`)
    ).join('');
    return `
      <div class="form-row">
        <label for="f-${esc(f.name)}">${esc(f.label)}${req}</label>
        <select id="f-${esc(f.name)}" name="${esc(f.name)}">${opts}</select>
      </div>`;
  }
  if (f.type === 'ref') {
    const items = f.source() || [];
    const opts = ['<option value="">— select —</option>'].concat(
      items.map(it => `<option value="${esc(it.id)}">${esc(f.labelOf(it))}</option>`)
    ).join('');
    return `
      <div class="form-row">
        <label for="f-${esc(f.name)}">${esc(f.label)}${req}</label>
        <select id="f-${esc(f.name)}" name="${esc(f.name)}">${opts}</select>
      </div>`;
  }
  if (f.type === 'textarea') {
    return `
      <div class="form-row">
        <label for="f-${esc(f.name)}">${esc(f.label)}${req}</label>
        <textarea id="f-${esc(f.name)}" name="${esc(f.name)}" rows="3"></textarea>
      </div>`;
  }
  return `
    <div class="form-row">
      <label for="f-${esc(f.name)}">${esc(f.label)}${req}</label>
      <input id="f-${esc(f.name)}" name="${esc(f.name)}" type="text" autocomplete="off" />
    </div>`;
}

function closeModal() {
  state.modalOpen = false;
  state.modalKind = null;
  document.getElementById('modal-backdrop').classList.remove('visible');
}

async function submitModal(e) {
  e.preventDefault();
  const cfg = MODAL_FORMS[state.modalKind];
  if (!cfg) return;
  const fieldsEl = document.getElementById('modal-fields');
  const fieldsByName = Object.fromEntries(cfg.fields.map(f => [f.name, f]));
  const payload = {};
  fieldsEl.querySelectorAll('[name]').forEach(el => {
    const v = el.value.trim();
    if (!v) return;
    const f = fieldsByName[el.name];
    if (f?.cast === 'integer') {
      const n = parseInt(v, 10);
      if (Number.isFinite(n)) payload[el.name] = n;
    } else {
      payload[el.name] = v;
    }
  });

  for (const f of cfg.fields) {
    if (f.required && !payload[f.name]) {
      setError(`'${f.label}' is required`);
      const input = fieldsEl.querySelector(`[name="${f.name}"]`);
      if (input) input.focus();
      return;
    }
  }

  try {
    await createRecord(cfg.endpoint, payload);
    // Refetch via /graph so federated fields (e.g. work_order.deployable)
    // are populated; in-place state mutation can only carry what REST
    // returned, which omits federated reads.
    await loadAll();
    setError('');
    setInfo('Saved');
    closeModal();
    rerender();
    updateFooterMeta();
  } catch (err) {
    setError(err.message);
  }
}

// ── Screen switching ──────────────────────────────────────────────────────────

function setScreen(name) {
  if (!SCREENS.includes(name)) return;
  state.screen = name;
  state.filter = '';
  state.expandedTeamId = null;
  document.getElementById('search').value = '';

  document.querySelectorAll('#tabs button').forEach(b => {
    b.classList.toggle('active', b.dataset.screen === name);
  });
  SCREENS.forEach(s => {
    document.getElementById(`screen-${s}`).classList.toggle('active', s === name);
  });
  document.getElementById('screen-title').textContent = SCREEN_TITLES[name] || name;
  rerender();

  // Preserve any #<screen>/<id> suffix when the screen prefix already matches —
  // setScreen is called from the hashchange handler when navigating to
  // #work/<id>, and we don't want to clobber the id segment back to bare #work.
  const currentPrefix = location.hash.slice(1).split('/')[0];
  if (currentPrefix !== name) {
    location.hash = name;
  }
}

function initHashRouting() {
  // Routing convention: #<screen> and #<screen>/<id> both land on the screen.
  // The id segment is reserved for selection-level routing per screen (not
  // wired everywhere yet) — for now we just resolve the screen prefix.
  const screenFromHash = () => location.hash.slice(1).split('/')[0];
  window.addEventListener('hashchange', () => {
    const key = screenFromHash();
    if (SCREENS.includes(key) && key !== state.screen) {
      setScreen(key);
    }
  });
  const initial = screenFromHash();
  if (SCREENS.includes(initial)) {
    state.screen = initial;
  } else {
    location.replace('#' + state.screen);
  }
}

function rerender() {
  switch (state.screen) {
    case 'teams':    renderTeams();    break;
    case 'staffing': renderStaffing(); break;
    case 'people':   renderPeople();   break;
    case 'work':     renderKanban();   break;
  }
}

// ── Keyboard shortcuts ────────────────────────────────────────────────────────

function initKeyboard() {
  const search = document.getElementById('search');

  document.addEventListener('keydown', e => {
    const tag = document.activeElement?.tagName?.toLowerCase();
    const inInput = tag === 'input' || tag === 'textarea' || tag === 'select';

    if (e.key === 'Escape') {
      if (state.modalOpen) { closeModal(); return; }
      if (document.activeElement === search && search.value) {
        search.value = '';
        state.filter = '';
        rerender();
        return;
      }
      if (state.expandedTeamId) {
        state.expandedTeamId = null;
        if (state.screen === 'teams') renderTeams();
        return;
      }
      return;
    }

    if (e.key === '/' && !inInput && !state.modalOpen) {
      e.preventDefault();
      search.focus();
      search.select();
      return;
    }

    if (e.key === 'n' && !inInput && !state.modalOpen) {
      e.preventDefault();
      openModal(modalKindForCurrentScreen());
      return;
    }
  });

  search.addEventListener('input', () => {
    state.filter = search.value;
    rerender();
  });
}

// ── Bootstrap ─────────────────────────────────────────────────────────────────

function initTabs() {
  document.querySelectorAll('#tabs button').forEach(b => {
    b.addEventListener('click', () => setScreen(b.dataset.screen));
  });
}

function initModal() {
  document.getElementById('modal-cancel').addEventListener('click', closeModal);
  document.getElementById('modal-form').addEventListener('submit', submitModal);
  document.getElementById('modal-backdrop').addEventListener('click', e => {
    if (e.target.id === 'modal-backdrop') closeModal();
  });
  document.getElementById('new-btn').addEventListener('click', () => {
    openModal(modalKindForCurrentScreen());
  });
}

async function init() {
  initTabs();
  initModal();
  initKeyboard();
  initHashRouting();

  // /config.json publishes cross-app public URLs; needed before first render
  // so cross-app anchors land with the right base.
  await loadConfig();

  try {
    await loadAll();
    setError('');
  } catch (err) {
    setError(err.message);
  }

  setScreen(state.screen);
  updateFooterMeta();
}

init();
