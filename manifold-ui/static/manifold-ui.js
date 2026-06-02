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

/**
 * @param {string} sel
 * @param {ParentNode} [root]
 * @returns {Element | null}
 */
export const $  = (sel, root = document) => root.querySelector(sel);

/**
 * @param {string} sel
 * @param {ParentNode} [root]
 * @returns {Element[]}
 */
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
 *
 * @param {string} tag
 * @param {Record<string, any>} [attrs]
 * @param {...(Node | string | number | false | null | undefined | Array<Node | string | number | false | null | undefined>)} children
 * @returns {HTMLElement}
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
    // c is string | number | Node here (null/false filtered above). Branch
    // on Node-ness so tsc can narrow; the runtime check is unchanged.
    if (typeof c === 'object' && /** @type {Node} */ (c).nodeType) {
      node.appendChild(/** @type {Node} */ (c));
    } else {
      node.appendChild(document.createTextNode(String(c)));
    }
  }
  return node;
}

/** HTML-escape a value for safe interpolation into raw strings. Escapes
 *  both quote flavours so attribute values are safe regardless of which
 *  quote style the caller wraps them in.
 *  @param {*} s
 *  @returns {string}
 */
export function esc(s) {
  return String(s ?? '')
    .replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/'/g, '&#39;')
    .replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

// ── Base path / URL building ─────────────────────────────────────────────
//
// Manifold runs in two deployment shapes (see
// docs/superpowers/specs/2026-06-01-path-based-deployment-design.md):
//
//   - domain mode: each app owns an origin   (https://groundwork.example.com/…)
//   - path mode:   all apps share one origin (https://example.com/groundwork/…)
//
// We never hardcode which. Instead the app derives its own base from where
// THIS module was loaded — `import.meta.url` is the fully-resolved URL the
// browser fetched manifold-ui.js from, so stripping the well-known
// `/static/manifold-ui.js` suffix yields the app's served root in either
// mode. apiUrl() then turns a root-relative app path into an absolute URL,
// while leaving cross-app absolute URLs (from /config.json) untouched.

const APP_BASE = import.meta.url.replace(/\/static\/manifold-ui\.js.*$/, '');

/**
 * Resolve an app-local path against the app's served base. Absolute URLs
 * (cross-app links built from *_public_url) pass through unchanged; only
 * root-relative paths ("/x/graph") get the base prefix. In domain mode
 * APP_BASE is just the origin, so this is a no-op versus today's behaviour.
 *
 * @param {string} path
 * @returns {string}
 */
export function apiUrl(path) {
  if (/^https?:\/\//.test(path)) return path;
  return path.startsWith('/') ? APP_BASE + path : path;
}

// ── HTTP / GraphQL ───────────────────────────────────────────────────────

/**
 * Thin fetch wrapper that throws on non-2xx with the body in the message.
 * Returns parsed JSON for application/json responses; raw text otherwise;
 * null for 204.
 *
 * @param {string} url
 * @param {RequestInit} [opts]
 * @returns {Promise<any>}
 */
export async function apiFetch(url, opts) {
  const res = await fetch(apiUrl(url), opts);
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
 *
 * @param {string} path
 * @param {string} query
 * @param {Record<string, any>} [variables]
 * @returns {Promise<any>}
 */
export async function gqlQuery(path, query, variables = {}) {
  const res = await fetch(apiUrl(path), {
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

/**
 * @typedef {object} ManifoldConfig
 * @property {string} [groundwork_public_url]
 * @property {string} [union_public_url]
 * @property {string} [cityhall_public_url]
 * @property {string} [yard_public_url]
 * @property {string} [lobby_public_url]
 * @property {string} [manifold_public_url]
 */

/** @type {ManifoldConfig} */
let _config = {};

/** @param {ManifoldConfig} c */
export function setManifoldConfig(c) { _config = c || {}; }

/** @returns {ManifoldConfig} */
export function getManifoldConfig() { return _config; }

/**
 * @param {string} [url]
 * @returns {Promise<void>}
 */
export async function loadManifoldConfig(url = '/config.json') {
  try {
    const res = await fetch(apiUrl(url));
    if (res.ok) _config = await res.json();
  } catch {
    _config = {};
  }
  applyHubLink();
}

/**
 * Point every `a.hub-link` (the "Manifold" brand link back to the landing
 * page) at the deployment's configured `manifold_public_url`. The static
 * HTML carries a tildarc.com fallback for the no-config case; any real
 * deployment — domain or path mode — supplies the right URL via config.
 */
export function applyHubLink() {
  const href = _config.manifold_public_url;
  if (!href) return;
  for (const a of $$('a.hub-link')) a.setAttribute('href', href);
}

/**
 * Build a cross-app anchor pointing at `<base>#<screen>[/<id>]`. Falls
 * back to plain escaped text when the target app's public URL is unknown,
 * so a missing peer URL never produces a broken link.
 *
 * @param {string} appKey   - one of "groundwork" / "union" / "cityhall" / "yard" / "lobby"
 * @param {string} screen   - hash route within the target app
 * @param {string | null | undefined} id
 * @param {string} label
 * @returns {string} an `<a>` HTML fragment, or escaped text if the target URL isn't known
 */
export function crossLink(appKey, screen, id, label) {
  const base = _config[`${appKey}_public_url`];
  if (!base) return esc(label);
  const hash = id ? `#${screen}/${encodeURIComponent(id)}` : `#${screen}`;
  // Trailing slash before the hash so path-mode links land on the app's
  // served root (`/groundwork/#…`) without an extra edge redirect hop.
  return `<a href="${esc(base.replace(/\/$/, ''))}/${hash}">${esc(label)}</a>`;
}

// ── Status strip ─────────────────────────────────────────────────────────

let _statusTimer = null;

/**
 * Show a transient message in `#status-strip`. `kind` is "info" or
 * "error". Auto-dismisses after a short delay unless `{ sticky: true }`.
 * Pass an empty/falsy message to hide.
 *
 * @param {string} message
 * @param {'info' | 'error'} [kind]
 * @param {{ sticky?: boolean }} [opts]
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

/**
 * Convenience: show err.message in the status strip as an error.
 * @param {Error | string | null | undefined | false} err
 */
export function setError(err) {
  if (!err) return setStatus('');
  setStatus((/** @type {Error} */ (err)).message || String(err), 'error');
}

// ── Footer meta ──────────────────────────────────────────────────────────

/**
 * Set the right-aligned counter text in the page footer.
 * @param {string} text
 */
export function updateFooterMeta(text) {
  const node = document.getElementById('footer-meta');
  if (node) node.textContent = text || '';
}

// ── Empty card ───────────────────────────────────────────────────────────

/**
 * Editorial-paper empty state. `hintHtml` may contain markup
 * (e.g. `Press <kbd>n</kbd> to start.`) — caller is responsible for
 * escaping any untrusted content.
 *
 * @param {object} spec
 * @param {string} spec.title
 * @param {string} spec.lede
 * @param {string} [spec.hintHtml]
 * @returns {HTMLElement}
 */
export function emptyCard({ title, lede, hintHtml }) {
  return el('div', { class: 'empty-card' },
    el('span', { class: 'empty-mark' }, '§'),
    el('h3', {}, title),
    el('p', { class: 'lede' }, lede),
    el('p', { class: 'hint', html: hintHtml || 'Press <kbd>n</kbd> to register the first one.' }),
  );
}

// ── Modal (promise-based) ────────────────────────────────────────────────
//
// Single canonical modal pattern across the suite. The modal is built on
// demand and resolves a promise with the captured form data on Submit,
// or `null` on Cancel / Esc / backdrop click.
//
//   const data = await openModal({
//     title: 'New environment',
//     intro: 'Optional subtitle paragraph in italic.',
//     submit: 'Create',                  // button label, default 'Submit'
//     lookupRef: (refKey) => [...],      // optional, for type:'ref' fields
//     fields: [
//       { name: 'name',  type: 'text',     label: 'Name', required: true },
//       { name: 'kind',  type: 'select',   label: 'Kind', options: [...], default: 'mock' },
//       { name: 'note',  type: 'textarea', label: 'Note', placeholder: '…' },
//       { name: 'team',  type: 'ref',      label: 'Team', refKey: 'teams', required: true },
//       { name: 'why',   type: 'radio',    label: 'Reason',
//         options: [{ code: 'foo', label: 'Foo', help: '…' }, …],
//         required: true, default: 'foo' },
//     ],
//   });
//   if (!data) return;     // cancelled
//
// No persistent #modal-root scaffold in markup — the modal builds and
// removes its own DOM. Caller doesn't need to track open/close state or
// wire keyboard handlers; Esc and backdrop click both resolve `null`.

/**
 * @typedef {object} FieldSpec
 * @property {string} name
 * @property {string} label
 * @property {string} [type]
 *   - "text" (default), "textarea", "select", "radio", "ref"
 * @property {boolean} [required]
 * @property {boolean} [full]
 * @property {string} [placeholder]
 * @property {string} [default]
 * @property {Array<string | {code:string,label:string,help?:string}>} [options]
 *   - string[] for "select"; structured objects for "radio".
 * @property {string} [refKey]
 *
 * @typedef {object} OpenModalSpec
 * @property {string} title
 * @property {string} [intro]
 * @property {string} [submit]
 * @property {FieldSpec[]} fields
 * @property {(refKey: string) => any[]} [lookupRef]
 *
 * @param {OpenModalSpec} spec
 * @returns {Promise<Record<string, string> | null>}
 */
export function openModal(spec) {
  return new Promise(resolve => {
    const backdrop = document.createElement('div');
    backdrop.className = 'modal-backdrop open';
    const dialog = document.createElement('div');
    dialog.className = 'modal-dialog';
    dialog.setAttribute('role', 'dialog');
    dialog.setAttribute('aria-modal', 'true');
    dialog.setAttribute('aria-labelledby', 'modal-title');

    const close = (value) => {
      backdrop.remove();
      document.removeEventListener('keydown', onKey);
      resolve(value);
    };
    const onKey = (e) => { if (e.key === 'Escape') close(null); };
    document.addEventListener('keydown', onKey);

    const form = document.createElement('form');
    form.innerHTML =
      `<h2 id="modal-title">${esc(spec.title)}</h2>` +
      (spec.intro ? `<p class="modal-intro">${esc(spec.intro)}</p>` : '');

    for (const f of spec.fields) {
      const wrap = document.createElement('div');
      wrap.className = 'modal-field';
      if (f.full) wrap.classList.add('full');

      const label = document.createElement('label');
      label.textContent = f.label + (f.required ? ' *' : '');
      wrap.appendChild(label);

      if (f.type === 'radio') {
        // Radio's options are structured {code, label, help?} objects, not strings.
        const radioOpts = /** @type {Array<{code:string,label:string,help?:string}>} */ (
          f.options || []
        );
        for (const opt of radioOpts) {
          const id = `r_${f.name}_${opt.code}`;
          const row = document.createElement('label');
          row.className = 'modal-radio';
          row.innerHTML =
            `<input type="radio" name="${esc(f.name)}" value="${esc(opt.code)}" id="${id}"` +
              `${opt.code === f.default ? ' checked' : ''}>` +
            `<span class="opt-label">${esc(opt.label)}</span>` +
            (opt.help ? `<span class="opt-help">${esc(opt.help)}</span>` : '');
          wrap.appendChild(row);
        }
      } else if (f.type === 'textarea') {
        const t = document.createElement('textarea');
        t.name = f.name;
        t.placeholder = f.placeholder || '';
        t.rows = 3;
        if (f.required) t.required = true;
        if (f.default != null) t.value = f.default;
        wrap.appendChild(t);
      } else if (f.type === 'select') {
        const s = document.createElement('select');
        s.name = f.name;
        if (f.required) s.required = true;
        s.appendChild(new Option('—', ''));
        // Select's options are plain strings (vs radio's structured objects).
        const selectOpts = /** @type {string[]} */ (f.options || []);
        for (const opt of selectOpts) {
          const o = new Option(opt, opt);
          if (opt === f.default) o.selected = true;
          s.appendChild(o);
        }
        wrap.appendChild(s);
      } else if (f.type === 'ref') {
        const s = document.createElement('select');
        s.name = f.name;
        if (f.required) s.required = true;
        s.appendChild(new Option('— none —', ''));
        const items = (spec.lookupRef || (() => []))(f.refKey) || [];
        for (const item of items) {
          const lbl = item.name || item.target_env_id || item.id;
          const o = new Option(lbl, item.id);
          if (item.id === f.default) o.selected = true;
          s.appendChild(o);
        }
        wrap.appendChild(s);
      } else {
        // text (default)
        const i = document.createElement('input');
        i.type = 'text';
        i.name = f.name;
        i.placeholder = f.placeholder || '';
        i.autocomplete = 'off';
        i.spellcheck = false;
        if (f.required) i.required = true;
        if (f.default != null) i.value = f.default;
        wrap.appendChild(i);
      }
      form.appendChild(wrap);
    }

    // Buttons assembled as element refs (not via innerHTML + querySelector)
    // so the cancel-button listener can never bind to null — defensive
    // after a session in which a botched module migration ate the cancel
    // event and produced a confusing "addEventListener of null" trace.
    const cancelBtn = document.createElement('button');
    cancelBtn.type = 'button';
    cancelBtn.textContent = 'Cancel';
    cancelBtn.addEventListener('click', () => close(null));

    const submitBtn = document.createElement('button');
    submitBtn.type = 'submit';
    submitBtn.className = 'primary';
    submitBtn.textContent = spec.submit || 'Submit';

    const actions = document.createElement('div');
    actions.className = 'modal-actions';
    actions.appendChild(cancelBtn);
    actions.appendChild(submitBtn);
    form.appendChild(actions);

    form.addEventListener('submit', (e) => {
      e.preventDefault();
      const data = new FormData(form);
      const out = {};
      for (const [k, v] of data.entries()) {
        const s = String(v).trim();
        if (s !== '') out[k] = s;
      }
      // Required-field guard (browser usually catches this, defence in depth).
      for (const f of spec.fields) {
        if (f.required && !out[f.name]) return;
      }
      close(out);
    });
    backdrop.addEventListener('click', (e) => { if (e.target === backdrop) close(null); });

    dialog.appendChild(form);
    backdrop.appendChild(dialog);
    document.body.appendChild(backdrop);
    setTimeout(() => form.querySelector('input,textarea,select')?.focus(), 50);
  });
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
 *
 * @param {FieldSpec} field
 * @param {*} value
 * @param {(refKey: string) => any[]} [lookupRef]
 * @returns {HTMLElement}
 */
export function fieldInput(field, value, lookupRef = () => []) {
  const id = `f-${field.name}-${Math.random().toString(36).slice(2, 8)}`;
  let input;
  if (field.type === 'textarea') {
    input = el('textarea', { id, name: field.name, rows: 2 }, value ?? '');
  } else if (field.type === 'select') {
    input = el('select', { id, name: field.name });
    input.appendChild(el('option', { value: '' }, '—'));
    const selectOpts = /** @type {string[]} */ (field.options || []);
    for (const opt of selectOpts) {
      const o = /** @type {HTMLOptionElement} */ (el('option', { value: opt }, opt));
      if (opt === value) o.selected = true;
      input.appendChild(o);
    }
  } else if (field.type === 'ref') {
    input = el('select', { id, name: field.name });
    input.appendChild(el('option', { value: '' }, '— none —'));
    for (const item of lookupRef(field.refKey || '') || []) {
      const lbl = item.name || item.target_env_id || item.id;
      const o = /** @type {HTMLOptionElement} */ (el('option', { value: item.id }, lbl));
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
  /** @type {Record<string, string>} */
  const out = {};
  $$('[name]', container).forEach(/** @type {Element} */ n => {
    const node = /** @type {HTMLInputElement} */ (n);
    const v = (node.value || '').trim();
    if (v !== '') out[node.name] = v;
  });
  return out;
}
