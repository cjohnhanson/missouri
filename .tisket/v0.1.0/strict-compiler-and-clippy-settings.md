---
title: "Strict compiler and clippy settings"
status: discovery
assignee:
labels: [admin]
depends_on: []
created: "2026-02-23T00:00:00Z"
updated: "2026-02-23T00:00:00Z"
---

Maximize strictness on rustc and clippy. Deny warnings, dead code, unused
imports, unused variables, missing docs — everything. Zero tolerance for
sloppiness in the build.

Discovery needed on the full set of lints available and the right way to
configure them (Cargo.toml `[lints]` table vs `#![deny(...)]` in lib.rs vs
clippy.toml vs `.cargo/config.toml`).
