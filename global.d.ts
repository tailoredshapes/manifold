// Global ambient declarations for the Manifold suite's static JS.
//
// These describe runtime-supplied identifiers that aren't ES module imports:
//
//   - `cytoscape`: groundwork and yard include the cytoscape vendor bundle via
//     a plain <script> tag, which leaves `cytoscape` as a global function.
//
//   - `mermaid` (cityhall): imported as an ES module from a CDN URL. tsc can't
//     fetch URLs, so the import is declared as a typeless module ambient
//     below to keep the rest of cityhall's app.js checkable.

// eslint-disable-next-line @typescript-eslint/no-explicit-any
declare const cytoscape: any;

declare module "https://cdn.jsdelivr.net/npm/mermaid@10/dist/mermaid.esm.min.mjs" {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const m: any;
  export default m;
}

// ── DOM API leniency ────────────────────────────────────────────────────────
//
// querySelector returns `Element | null`, and the input-flavoured properties
// (.value / .checked / .dataset / .focus / .blur / .closest) live on the
// HTML* subtypes. Strictly correct, but in plain-JS-with-JSDoc we rarely
// narrow at the call site — every `$('#search').value` would otherwise need
// a cast. Augment Element with the methods we actually use across the suite
// so tsc can still catch real bugs (undefined imports, wrong call shapes,
// missing properties on plain objects) without drowning them in DOM noise.
//
// As individual files get full JSDoc annotations, prefer typed casts at the
// call site (/** @type {HTMLInputElement} */ (el)) — the relaxation here is
// a pragmatic catch-net for everything else.

interface Element {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  value?: any;
  checked?: boolean;
  dataset?: DOMStringMap;
  focus?: () => void;
  blur?: () => void;
  select?: () => void;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  onclick?: any;
  style?: CSSStyleDeclaration;
  hidden?: boolean;
}

interface EventTarget {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  value?: any;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  closest?: (sel: string) => any;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  dataset?: any;
  id?: string;
}

interface Event {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  dataTransfer?: any;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  relatedTarget?: any;
}
