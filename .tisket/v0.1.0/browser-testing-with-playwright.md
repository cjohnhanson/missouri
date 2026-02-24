---
title: "Browser testing with Playwright"
status: discovery
assignee:
labels: [feature]
depends_on: []
created: "2026-02-23T00:00:00Z"
updated: "2026-02-23T00:00:00Z"
---

Bring missouri's "show me state" philosophy to browser testing. States are
directories of serialized browser state — all human-inspectable, diffable,
version-controllable files on disk:

```
logged-in-dashboard/
  storage.json       # Playwright storageState: cookies, localStorage, IndexedDB
  session.json       # sessionStorage (manual dump/restore via addInitScript)
  page.html          # DOM snapshot
  aria.yml           # accessibility tree (Playwright ariaSnapshot)
  screenshot.png     # visual baseline (pixel-diffable via Pixelmatch)
```

Transitions are Playwright automation scripts. State restoration via
`browser.newContext({ storageState })` + `addInitScript()` + navigate.
Comparators use missouri's existing model: file comparators for JSON/YAML,
custom comparators for DOM diffing, pixel-diff for screenshots, ignore
patterns for non-deterministic fields.

Network mocking is NOT part of browser state — that's a separate sandbox
concern (see mitmproxy tisket). Browser state is purely browser state.

## What Playwright can serialize (researched)

| Artifact | Format | Diffable | Restorable |
|----------|--------|----------|------------|
| storageState (cookies + localStorage + IndexedDB) | JSON | yes | yes, via newContext() |
| sessionStorage | JSON | yes | yes, via addInitScript() |
| DOM | HTML | yes | no (reconstructed from storage + navigation) |
| ARIA tree | YAML | yes | no (derived) |
| Screenshots | PNG | pixel-diff | no (derived) |
| Cache API | JSON via CDP | yes | no |

## What CANNOT be captured

- In-memory JS heap — fundamentally opaque
- WebSocket connection state — ephemeral
- Service worker internal state — can block/list, can't snapshot

## Determinism considerations

- Font rendering differs across OS — must run in Docker/Linux for visual
  regression consistency
- Animations — Playwright disables CSS animations for screenshots
- Time — Playwright Clock API mocks Date, setTimeout, etc.
- Randomness — Math.random() not seedable, needs injected PRNG
