# manifold-landing

The Manifold suite's landing page — a single, self-contained `index.html`
(inline styles + one ES module). It shows live KPIs by querying each app's
graphlette and links out to the apps.

It is **mode-agnostic**: the app links + the `gql()` base URLs come from the
`APPS` map, which a deployment rewrites to its own addresses:

- **Domain mode** (e.g. tildarc): `https://groundwork.tildarc.com`, …
- **Path mode** (e.g. the tailoredshapes showcase): `/groundwork`, … — the
  `manifold-showcase` repo's `scripts/build-landing.sh` does this rewrite and
  serves the page from a tiny Lambda at the site root.

It's a deployable, not docs — keep it in the repo.
