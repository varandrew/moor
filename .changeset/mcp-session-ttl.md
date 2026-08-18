---
"moor": minor
---

feat(gateway): MCP session idle TTL, capacity cap & SSE stream lifetime

- Sessions now expire after an idle TTL (new setting `advanced.mcpSessionIdleTtlMs`, default 1h, valid range 5min–24h); validation on POST/GET refreshes liveness and a 60s background sweeper reclaims sessions leaked by crashed clients.
- `initialize` beyond 128 concurrent sessions returns HTTP 503 instead of growing unboundedly.
- GET SSE keep-alive streams close after a 30min total lifetime; clients reconnect per the Streamable HTTP spec.

Closes #62
