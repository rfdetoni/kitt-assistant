# Assistant Runtime v0.3.36 — Agent CLI 0.87.29 integration

- Pin the Python uv runtime lock to the new Agent CLI main SHA `cb530c2ecddaafa1241e957c3d379ea853f43863` and package version `0.87.29`.
- Update compatible Agent checkout in Python and platform CI checks.
- Keep Rust workspace Cargo pins and locks unchanged: no Rust or protocol contracts changed.
- Use the official `uv lock --check` gate for reproducible runtime dependencies; continue Python 3.14 and runtime smoke validation.
- Ollama local E5 setup is explicit and optional; no automated model pull or additional resident LLM is introduced by Assistant.
