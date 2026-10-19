# Assistant 0.1.57 / Runtime 0.4.12 — Agent 0.88.12 guest MCP compatibility

Pins Agent CLI 0.88.12 at its immutable Git revision and aligns Assistant's Rust Cargo and Python uv locks. The Agent now offers an opt-in host-side, read-only MCP adapter for managed RE guest status and Windows Oracle readiness; this introduces no new Assistant daemon/child-agent authority.

MCP remains disabled unless operator-configured. Clean-Room implementer agents must never be given the RE guest MCP bridge. The Reverse Proxy rich-editor fix is independently released as 5.4.10 and pinned by the KITT ecosystem manifest. Windows guest / Wine conformance not claimed.
