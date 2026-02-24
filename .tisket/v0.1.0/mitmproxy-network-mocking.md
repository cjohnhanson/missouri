---
title: "mitmproxy for mocking network calls"
status: discovery
assignee:
labels: [feature]
depends_on: []
created: "2026-02-23T00:00:00Z"
updated: "2026-02-23T00:00:00Z"
---

Like VCRpy but at the proxy level instead of the library level —
language-agnostic HTTP record/replay. Missouri wraps transition commands
with HTTP_PROXY/HTTPS_PROXY pointed at mitmproxy, so any subprocess making
HTTP calls gets intercepted regardless of language or HTTP library.

A sandbox type alongside flox. Sandboxes are composable — a transition
could run inside flox (packages) + mitmproxy (network) simultaneously.

Network recordings are sandbox configuration, not state. State directories
stay clean — they represent the actual system under test, not the mock
infrastructure around it.

Supports HTTP/1, HTTP/2, HTTP/3, WebSockets, TLS. Requires TLS certificate
management for HTTPS interception.
