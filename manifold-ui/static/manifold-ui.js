// Manifold UI · shared ES module
//
// Vanilla JS. No build step. No framework. Plain ES module exports.
//
// Apps load this from `/static/manifold-ui.js` and import the helpers
// directly. Per-app logic (screens, entity shapes, business rules) stays
// in the app's own `app.js`.
//
// DOM contracts:
//
//  - setStatus/setError require an element with id="status-strip" in the
//    document (style supplied by manifold-ui.css).
//
//  - updateFooterMeta requires id="footer-meta" inside the app's footer.
//
//  - openModal/closeModal/isModalOpen require the modal scaffold:
//      <div id="modal-root" class="modal-backdrop" role="dialog" aria-modal="true">
//        <div class="modal">
//          <h2 id="modal-title"></h2>
//          <div id="modal-fields" class="form-grid"></div>
//          <div class="actions">
//            <button id="modal-cancel">Cancel</button>
//            <button id="modal-save" class="primary">Save</button>
//          </div>
//        </div>
//      </div>
//
//  - crossLink reads from a config object you load once via
//    loadManifoldConfig() (or set explicitly with setManifoldConfig).

// ── DOM selectors ────────────────────────────────────────────────────────

export const $  = (sel, root = document) => root.querySelector(sel);
export const $$ = (sel, root = document) => Array.from(root.querySelectorAll(sel));

// ── Element factory ──────────────────────────────────────────────────────

/**
 * Hyperscript-style DOM constructor.
 *
 * - `class`        → assigns className
 * - `dataset`      → Object.assign onto element.dataset
 * - `html`         → assigns innerHTML (use sparingly; prefer children)
 * - `onClick` etc. → addEventListener(name.slice(2).toLowerCase(), fn)
 * - other keys     → assign as a DOM property when it exists on the node,
 *                    else setAttribute (covers id, aria-*, role, etc.)
 *
 * Children may be strings, numbers, or DOM nodes. Null/false/undefined
 * children are skipped (so you can short-circuit with `cond && el(...)`).
 */
export function el(tag, attrs = {}, ...children) {
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

/** HTML-escape a value for safe interpolation into raw strings. Escapes
 *  both quote flavours so attribute values are safe regardless of which
 *  quote style the caller wraps them in. */
export function esc(s) {
  return String(s ?? '')
    .replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/'/g, '&#39;')
    .replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

// ── HTTP / GraphQL ───────────────────────────────────────────────────────

/**
 * Thin fetch wrapper that throws on non-2xx with the body in the message.
 * Returns parsed JSON for application/json responses; raw text otherwise;
 * null for 204.
 */
export async function apiFetch(url, opts) {
  const res = await fetch(url, opts);
  if (!res.ok) {
    const body = await res.text().catch(() => '');
    throw new Error(`${opts?.method || 'GET'} ${url} → ${res.status}${body ? ': ' + body : ''}`);
  }
  if (res.status === 204) return null;
  const ctype = res.headers.get('content-type') || '';
  return ctype.includes('application/json') ? res.json() : res.text();
}

/**
 * POST a GraphQL query to `path` and unwrap `data` from the response.
 * Throws on transport failure OR if the response includes `errors`.
 */
export async function gqlQuery(path, query, variables = {}) {
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

// ── Cross-app linking ────────────────────────────────────────────────────
//
// Each Manifold app publishes its peers' public URLs in `/config.json`
// (e.g. `{ "groundwork_public_url": "https://groundwork.tildarc.com" }`).
// Load it once at boot, then crossLink() builds anchors to other apps
// without hardcoding hostnames.

let _config = {};

export function setManifoldConfig(c) { _config = c || {}; }
export function getManifoldConfig() { return _config; }

export async function loadManifoldConfig(url = '/config.json') {
  try {
    const res = await fetch(url);
    if (res.ok) _config = await res.json();
  } catch {
    _config = {};
  }
}

/**
 * Build a cross-app anchor pointing at `<base>#<screen>[/<id>]`. Falls
 * back to plain escaped text when the target app's public URL is unknown,
 * so a missing peer URL never produces a broken link.
 */
export function crossLink(appKey, screen, id, label) {
  const base = _config[`${appKey}_public_url`];
  if (!base) return esc(label);
  const hash = id ? `#${screen}/${encodeURIComponent(id)}` : `#${screen}`;
  return `<a href="${esc(base.replace(/\/$/, ''))}${hash}">${esc(label)}</a>`;
}

// ── Status strip ─────────────────────────────────────────────────────────

let _statusTimer = null;

/**
 * Show a transient message in `#status-strip`. `kind` is "info" or
 * "error". Auto-dismisses after a short delay unless `{ sticky: true }`.
 * Pass an empty/falsy message to hide.
 */
export function setStatus(message, kind = 'info', { sticky = false } = {}) {
  const strip = $('#status-strip');
  if (!strip) return;
  strip.classList.remove('error', 'info');
  if (!message) {
    strip.style.display = 'none';
    strip.textContent = '';
    return;
  }
  strip.classList.add(kind === 'error' ? 'error' : 'info');
  strip.textContent = message;
  strip.style.display = 'block';
  if (_statusTimer) clearTimeout(_statusTimer);
  if (!sticky) {
    _statusTimer = setTimeout(() => {
      strip.style.display = 'none';
      strip.textContent = '';
    }, kind === 'error' ? 6000 : 3000);
  }
}

/** Convenience: show err.message in the status strip as an error. */
export function setError(err) {
  if (!err) return setStatus('');
  setStatus(err.message || String(err), 'error');
}

// ── Footer meta ──────────────────────────────────────────────────────────

/** Set the right-aligned counter text in the page footer. */
export function updateFooterMeta(text) {
  const node = document.getElementById('footer-meta');
  if (node) node.textContent = text || '';
}

// ── Empty card ───────────────────────────────────────────────────────────

/**
 * Editorial-paper empty state. `hintHtml` may contain markup
 * (e.g. `Press <kbd>n</kbd> to start.`) — caller is responsible for
 * escaping any untrusted content.
 */
export function emptyCard({ title, lede, hintHtml }) {
  return el('div', { class: 'empty-card' },
    el('span', { class: 'empty-mark' }, '§'),
    el('h3', {}, title),
    el('p', { class: 'lede' }, lede),
    el('p', { class: 'hint', html: hintHtml || 'Press <kbd>n</kbd> to register the first one.' }),
  );
}

// ── Modal (new-record style) ─────────────────────────────────────────────

/**
 * Open the shared modal with a title and a list of field DOM nodes
 * (typically from fieldInput()). Focuses the first focusable input.
 * Does not manage app state — caller tracks open/closed for keyboard
 * routing, then calls saveModal/closeModal in response.
 */
export function openModal({ title, fields = [] }) {
  const titleEl = $('#modal-title');
  const fieldsEl = $('#modal-fields');
  const root = $('#modal-root');
  if (!titleEl || !fieldsEl || !root) {
    throw new Error('manifold-ui: modal scaffold (#modal-root/#modal-title/#modal-fields) missing');
  }
  titleEl.textContent = title;
  fieldsEl.innerHTML = '';
  for (const node of fields) fieldsEl.appendChild(node);
  root.classList.add('open');
  const first = $('input, select, textarea', fieldsEl);
  if (first) first.focus();
}

export function closeModal() {
  $('#modal-root')?.classList.remove('open');
}

export function isModalOpen() {
  return $('#modal-root')?.classList.contains('open') ?? false;
}

// ── Field rendering ──────────────────────────────────────────────────────
//
// Render a labeled form field from a config object describing the field.
// Supports text / textarea / select / ref. For `ref`, the caller passes
// a `lookupRef(refKey) -> Array<{id, name, …}>` to populate options —
// keeps the shared module free of app-specific state.
//
//   field: { name, label, type, required?, full?, options?, refKey? }

/**
 * Build a `<div class="field">` with a labeled input/select/textarea.
 * Returns the wrapper div; the input itself has the field's `name`.
 */
export function fieldInput(field, value, lookupRef = () => []) {
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
    for (const item of lookupRef(field.refKey) || []) {
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

/** Read every `[name]`d input inside `container` into a plain object,
 *  skipping empty strings. */
export function readForm(container) {
  const out = {};
  $$('[name]', container).forEach(node => {
    const v = node.value.trim();
    if (v !== '') out[node.name] = v;
  });
  return out;
}
