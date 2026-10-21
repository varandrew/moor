---
"moor": minor
---

feat(gateway): curated tool exposure, risk policy, live tool notifications

- **Curated exposure**: a Profile can switch to curated mode, where only the
  tools you pin are listed to agents. Everything else stays reachable through
  two gateway meta tools, `moor_search_tools` and `moor_call_tool`, so a cached
  tool name never breaks. Direct listing remains the default and is one click
  away.
- **Risk policy**: every discovered tool is classified safe / write /
  destructive / unknown from its name and description, and you can correct the
  classification. A Profile can block destructive tools: they disappear from
  `tools/list` and are refused with a structured error.
- **Tool refresh without a restart**: the server page now re-runs discovery on
  the running process instead of stopping and starting it.
- **Live tool list**: `GET /mcp` finally emits
  `notifications/tools/list_changed` when the profile, pins, tools or risk
  policy change, debounced to one notification per burst.
- **Audit provenance**: `audit_logs` records whether a call was direct or came
  through a meta tool, keeps a record of refused calls, and insights report how
  many calls the risk policy blocked.

**Breaking change for tool-call error handling**: failures that used to
collapse into `-32603` now have their own codes — `-32001` (upstream timeout),
`-32002` (policy denied, with `data.policy`, `data.riskLevel`, `data.hint`),
`-32003` (server unavailable). Clients that branch on the error code should be
updated; `-32602` still means "tool not found or disabled".
