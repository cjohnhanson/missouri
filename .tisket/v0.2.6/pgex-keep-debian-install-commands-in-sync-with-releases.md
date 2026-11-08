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
updated: 2026-11-08T05:10:38Z
---

## Problem

README.md’s Install section downloads and installs v0.2.3 on Debian and Ubuntu, two releases behind v0.2.5. The package version and filename are hard-coded. The release process neither updates nor checks them.

## Change

Update both commands to use `missouri_0.2.5-1_amd64.deb`. Add an automated version check and include it in the release process after the `Cargo.toml` version bump. Update CONTRIBUTING.md accordingly. A `releases/latest/download/` URL alone cannot resolve a filename containing the version.

## Acceptance

1. The README `.deb` URL’s release tag and package filename, including the filename passed to `dpkg`, match the version in `Cargo.toml`.
2. Running both README commands on Debian or Ubuntu amd64 installs a binary whose `missouri --version` matches `Cargo.toml`.
3. Changing either README command’s `.deb` version to differ from `Cargo.toml` makes a test or `scripts/prepublish.sh` fail; matching versions pass.
