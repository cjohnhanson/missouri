---
title: "Improve result printing and summary output"
status: done
assignee:
labels: [enhancement, reporting]
depends_on: []
created: "2026-02-22T21:09:40Z"
updated: "2026-02-22T21:09:40Z"
---

Current output is functional but could be more informative:

```
✓ setup: build tisket
PASS empty → initialized → has-project → has-issue → issue-closed → issue-reopened
PASS empty → initialized → has-project → has-issue → issue-edited
PASS empty → initialized → has-project → has-issue → has-two-issues → one-closed

3 passed, 0 failed, 3 total
```

Discovery areas:
- Summary only shows path-level counts, no step or assertion counts
- Long path names wrap awkwardly — truncation or multiline formatting?
- No timing info (how long did each path/step take?)
- No progress indication during execution (paths can take a while)
- Summary could include more: total steps, total assertions, elapsed time
