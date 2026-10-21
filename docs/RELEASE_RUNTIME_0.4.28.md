# Assistant Runtime 0.4.28

- Pin Agent CLI 0.88.37: `2a8aaad67446553c1b0c226040aab8208e3123d3`.
- Python package, `uv.lock` and both immutable CI checkouts are synchronized. Rust workspace and Cargo.lock remain 0.1.61.
- Existing host execution and Goal persistence now expose read-only flow/program/tool receipt guidance and strict C/C++ native verification; no new privilege or unsandboxed fallback is introduced.
- Validate Assistant CI and root integration; the target workstation's Wine build environment and authenticated browser are outside source CI.
