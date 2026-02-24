---
title: "Better assertion visibility in run output"
status: done
assignee:
labels: [enhancement, reporting]
depends_on: []
created: "2026-02-22T21:09:39Z"
updated: "2026-02-22T21:09:39Z"
---

Currently assertions are invisible in default output. A passing run with
assertions looks identical to one without them:

```
PASS empty → initialized → has-project → has-issue → issue-closed → issue-reopened
```

No indication that assertions ran, how many, or what they checked. Only `-v`
shows assertion names. `--check-only` also gives no assertion detail by default.

Need to surface assertion info without requiring verbose mode. Discovery needed
on what the right default verbosity is — count in summary? Inline per-step?
Something else?
