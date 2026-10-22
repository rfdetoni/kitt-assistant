# Assistant Runtime 0.4.39 — network Git shell-loop recovery

Pins merged Agent CLI 0.88.50 at immutable `168e21b396e72163b45ed6cd0e83e8115c446d46` in `uv.lock`, both CI checkouts and Python dependency range `>=0.88.50,<0.89`.

A 2026-10-22 Wine MBS run proposed `process.run` with `bash -c` composing directory creation, Git initialization and two public clone commands, marked `network=true`. The narrow public Git delegation intentionally rejects arbitrary network shells; the agent entered a manual approval wait for 84.3 seconds. The fix fails this specific shell-wrapped Git checkout early with actionable feedback to use `repo.create_directory`, direct `git init` and independent `git clone` calls. This does not grant network authority or execute a model-authored shell.

Agent CLI PR #142 passed critical contracts, Docker, Podman and Windows smoke. Assistant CI verifies uv metadata and runtime cross-package behavior. Authenticating Gemini Web and running Wine clone on the actual host remain operator-environment acceptance tests.
