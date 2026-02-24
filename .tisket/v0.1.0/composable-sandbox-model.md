---
title: "Composable sandbox model"
status: discovery
assignee:
labels: [feature]
depends_on: []
created: "2026-02-23T00:00:00Z"
updated: "2026-02-23T00:00:00Z"
---

Currently `Sandbox` is an enum: `None` or `Flox { flox_bin, project_root }`.
Adding mitmproxy (and potentially Playwright browser contexts) means
sandboxes need to compose — a transition might run inside flox (packages)
+ mitmproxy (network) + a browser context simultaneously.

The sandbox model needs to become a stack/list of wrappers rather than a
single enum variant. Each sandbox layer wraps command execution with its
own environment (env vars, process setup, cleanup).
