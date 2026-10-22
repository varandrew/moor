---
"moor": minor
---

Curated tool exposure, profile risk policies, live tool updates, and audit provenance.

- **Curated tool exposure:** Pin tools for direct listing and discover other enabled tools through `moor_search_tools` and `moor_call_tool`. A guided picker helps configure the initial selection.
- **Profile risk policies:** Review inferred tool risk levels, override classifications, and optionally hide and block destructive tools. User overrides survive rediscovery.
- **Live tool updates:** Refresh tools without restarting servers. Initialized MCP sessions receive debounced `notifications/tools/list_changed` notifications when the exposed catalog changes.
- **Audit provenance:** Distinguish direct calls from meta-tool calls, retain policy-denied calls, and report blocked-call counts in insights.
- **Reliability improvements:** Apply curated-mode transitions atomically, serialize profile governance changes, preserve consistent profile snapshots during concurrent operations, and reject stale discovery results without replacing the current catalog.

### Compatibility Notes

- Existing profiles retain direct listing and advisory risk policies. Database upgrades preserve settings, disabled-tool lists, and audit history.
- **Tool-call error handling has changed:** Moor-generated errors use `-32001` for upstream timeouts, `-32002` for policy denials, and `-32003` for unavailable servers. Unknown or disabled tools continue to use `-32602`. Downstream JSON-RPC errors retain their original codes and data.
- Risk classification uses tool names and descriptions and can be corrected manually.
