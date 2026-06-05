//! Shared frontend kit for the Manifold suite.
//!
//! Two static strings exported as compile-time constants:
//!
//! - [`CSS`] — design tokens + primitive component styles. Each app serves
//!   this on its own `/static/manifold-ui.css` route. Apps load it before
//!   their own inline `<style>` block, then override or extend with
//!   app-specific rules (e.g. yard's env-card, cityhall's gantt).
//!
//! - [`JS`] — ES module exporting `el`, `esc`, `apiFetch`, `gqlQuery`,
//!   modal/status helpers, and the cross-app linking utility. Apps serve
//!   it on `/static/manifold-ui.js` and import directly from there.
//!
//! Why a Rust crate to ship CSS/JS? Three reasons:
//!
//! 1. **Single source of truth.** Five apps; one place to update the
//!    design tokens. Clients can ship a single `client.css` overriding
//!    `--ink`, `--font-serif`, etc. and white-label every app at once.
//!
//! 2. **No build step.** `include_str!` bakes the assets into each
//!    binary; cargo's dependency tracker rebuilds the app when the
//!    asset changes. No bundler, no CDN, no cross-origin headaches.
//!
//! 3. **Single-origin.** Each app serves its own copy of the kit from
//!    the same origin as its data, so the browser doesn't need to be
//!    told anything new about CORS.

/// Shared CSS — design tokens + primitive components. Serve as
/// `application/css` at `/static/manifold-ui.css`.
pub const CSS: &str = include_str!("../static/manifold-ui.css");

/// Shared ES module — `el`, `esc`, `apiFetch`, `gqlQuery`, modal +
/// status helpers, `crossLink`. Serve as `application/javascript` at
/// `/static/manifold-ui.js`.
pub const JS: &str = include_str!("../static/manifold-ui.js");

/// Shared favicon (the Manifold mark, 512×512 PNG). Each app serves it at
/// `/static/favicon.png` and `/favicon.ico` so the whole suite shares one icon.
pub const FAVICON: &[u8] = include_bytes!("../static/favicon.png");

/// Inject a `<base href="<prefix>/">` into an app's index HTML so the relative
/// asset references (`static/app.js`, …) resolve under the app's path prefix
/// even when the page is loaded WITHOUT a trailing slash — e.g. a link to
/// `/groundwork` (not `/groundwork/`), where relative paths would otherwise
/// resolve against the origin root and 404.
///
/// The prefix comes from `MANIFOLD_BASE_PATH` (e.g. `/groundwork`). In domain
/// mode it's unset/empty and the HTML is returned unchanged.
pub fn index_html(raw: &str) -> String {
    let base = std::env::var("MANIFOLD_BASE_PATH").unwrap_or_default();
    let base = base.trim().trim_end_matches('/');
    if base.is_empty() {
        return raw.to_string();
    }
    match raw.find("<head>") {
        Some(idx) => {
            let at = idx + "<head>".len();
            format!("{}\n  <base href=\"{base}/\">{}", &raw[..at], &raw[at..])
        }
        None => raw.to_string(),
    }
}
