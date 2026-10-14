# Assistant Runtime 0.3.22 — Agent CLI 0.87.10 alignment

This dependency-only runtime release pins
`rfdetoni/kitt-agent-cli@6a76c27f4e71962a644d174e9187f4d386f794f7`
(v0.87.10), and uses the previously pinned Protocol 0.13.2
(`0b9300e418557b00e371eeb2c70ef91fbf50855d`).
Assistant native 0.1.41 is unchanged. CI cross-checkouts now match the
immutable Python runtime dependency lock.

The new Agent operation `sandbox.python` runs only the existing restricted
`python_compute` AST interpreter; no Assistant-side code execution privilege,
IPC changes, or additional OS permissions are introduced.
