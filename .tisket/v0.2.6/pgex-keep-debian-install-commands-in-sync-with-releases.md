---
title: Keep Debian install commands in sync with releases
status: todo
priority: null
assignee: null
due_date: null
labels:
- docs
depends_on: []
created: 2026-11-08T05:10:38Z
updated: 2026-11-08T06:00:00Z
---

## Problem

README.md's Debian/Ubuntu commands pin v0.2.3, two releases behind v0.2.5. No release step updates or checks them. The `.deb` filename contains the version, so `releases/latest/download/` alone cannot fix this. Unpublished `Cargo.toml` versions produce 404s; the latest published release tag is the reference.

## Change

Update both commands to v0.2.5. Add a check that reads the latest published release tag and rejects mismatches in either command, independently of releases. Add a CONTRIBUTING.md Releases step: after `release.yml` publishes the tag, the maintainer updates both commands.

## Acceptance

1. The URL tag, URL filename, and `dpkg` filename all name the latest published release.
2. On Debian or Ubuntu amd64, both commands install a binary whose `missouri --version` matches that tag.
3. The check fails when either command names another version and passes when both match.
4. CONTRIBUTING.md places the README update after publication.
