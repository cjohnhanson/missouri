---
title: Run the dependency audit in the required CI gate
status: todo
priority: null
assignee: null
due_date: null
labels:
- ci
- security
depends_on: []
created: 2026-11-08T05:10:38Z
updated: 2026-11-08T06:00:00Z
---

## Problem

Pull requests never report RustSec advisories or yanked crates in `Cargo.lock`. Only the manual `scripts/prepublish.sh` runs `cargo audit`; neither `gate` nor the release workflow does. For release 0.2.4, prepublish refused on advisories CI had never reported, and the maintainer had to update the lockfile in a separate pull request before the release could continue.

## Change

Add prepublish's audit to the required `gate` check. Declare it in `.gaff/gaff.yml` and regenerate `.github/workflows/gate.yml` with `gaff init --github`.

## Acceptance

1. On pull requests, `gate` runs `cargo audit --deny yanked --deny warnings`, matching prepublish.
2. A pull request whose `Cargo.lock` pins a crate with a RustSec advisory or a yanked crate fails `gate`.
3. `.gaff/gaff.yml` declares the audit step, the regenerated workflow includes it, and `gaff check --github` reports no drift.
4. A pull request with a clean lockfile and passing existing checks passes `gate`.
