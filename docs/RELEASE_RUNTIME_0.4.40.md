# Assistant Runtime 0.4.40 — Gemini empty-action retry containment

Pins Agent CLI **0.88.51**, immutable merged main commit `04c57fd2e4efff892d0373e71f2eb2805827ca42`, throughout `pyproject.toml`, `uv.lock` and both CI checkouts.

Evidence from the 2026-10-22 `mbs-wine` run: after a valid `ls -la mbs-wine` host action the Gemini Web provider emitted `{}` repeatedly; the previous autonomous contract retried the same objective 5 times with delays up to 110 seconds for a single request. Agent 0.88.51 allows failover once to another preconfigured, endpoint-trusted model only when no host action and no partial output exist, or returns BLOCKED_ENV for explicit operator continuation. No model-generated command is invented or replayed. Structured `repo.list` is preferred and file tools use root-relative paths such as `mbs-wine/.gitignore`.

Agent PR #143 passed critical contracts, Docker, Podman and Windows smoke. This release does not claim to fix Gemini Web response generation or UI tap correlation; authenticated provider/Windows Oracle validation remains external.
