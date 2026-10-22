# Assistant Runtime 0.4.38 — protected Git metadata in automatic contracts

The Python package now depends on `kitt-agent-cli>=0.88.49,<0.89`; `uv.lock` and both CI checkout references resolve to the merged Agent commit `7c1fe6dc56664fc291077b9ec99d599ba84e3755`.

The 2026-10-22 Wine MBS failure `Automatic contract planning failed: Protected workspace path component: .git` came from model-generated planning paths being validated by the protected workspace filesystem. Agent CLI 0.88.49 converts this failure into a bounded, actionable validation error so the planner can request a corrected worktree scope. Direct access to `.git` is still rejected.

Reverse Proxy 5.4.22 addresses the separate empty `{}` Gemini response by declining speculative repair; its merged main SHA is `95da67eb2211c4157aa5c97ea11f64a697dbfd10`. Neither change authorizes protected metadata access or a broader Git/network command surface.

## Verification

Agent CLI PR #141 passed critical tests and Docker/Podman/Windows smoke jobs. Assistant CI must validate `uv lock --check`, Python runtime integration and immutable Agent checkout. End-to-end behavior on an authenticated Gemini Web UI and cloning/Oracle testing in the operator workspace remain outside the CI evidence.
