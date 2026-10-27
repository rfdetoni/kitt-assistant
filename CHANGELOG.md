# Changelog — K.I.T.T. Assistant

## Runtime 0.4.46 — Agent isolated worktree compatibility

- Pin Agent CLI 0.89.4 at `eaa4f198a14c24356c4d0578e9d41f54466c533b`, aligning runtime dependency, Python uv lock, and both immutable CI checkouts.
- The Agent's isolated task worktree execution requires Docker or an explicitly trusted Compose broker and preserves the original checkout.
- No changes to sibling Protocol, Rust or HUD component locks.


## Runtime 0.4.45 — validated persistent Autopilot broker

- Synchronize Agent CLI 0.89.3 at immutable SHA `1b17a65f04723b2135d876d9eb4b96432412c430` across Python dependency, uv lock, and two CI source checkouts.
- Real Docker integration now checks persistent toolchain cache, verified worker state, control-plane isolation, readonly profiles and automatic worker reprovisioning.
- No changes to Rust/Protocol/Node locks or separate publication authority.


## Runtime 0.4.44 — GitHub Autopilot publisher compatibility

- Pin Agent CLI 0.89.2 at immutable `105ebd7eaf7477ab6cc6ec6cf789d2c707afc614` in dependency constraint, uv.lock and two CI source checkouts.
- GitHub `autopilot.publish` is disabled by default and requires operator authorization, test evidence and non-force ref updates.
- Other Rust/Protocol/HUD dependency locks unchanged.


## Runtime 0.4.43 — Agent Autopilot 0.89.1 compatibility

- Raise Python runtime dependency to `kitt-agent-cli>=0.89.1,<0.90`, matching the shipped Autopilot broker/checkpoint/network capabilities.
- Pin the Agent CLI immutable main SHA `0ddae4475cdd05d1c0c14059759ac0fd8d3ade88` in uv.lock and both CI checkouts.
- Keep Protocol SHA and all Rust/Cargo/Node dependency locks unchanged; require passing Assistant runtime tests and installation `pip check`.


## Runtime 0.4.42 — FreeLLMAPI model discovery

Pins Agent CLI 0.88.53 at immutable main commit `2ce5c7bb4d4a6dfbfe7a4831dfcdc81c28db665f` in the minimum dependency, uv lock and both CI checkouts. FreeLLMAPI now appears in the provider catalog and model picker, authenticates model discovery, and respects the workspace endpoint in the models CLI. See [release notes](docs/RELEASE_RUNTIME_0.4.42.md).

## Runtime 0.4.41 — FreeLLMAPI planning and native tools

Pins Agent CLI 0.88.52 at immutable main commit `bc4694586ee826f057d034701c2e2589960788f6` in the minimum dependency, uv lock and both CI checkouts. Read-only plan responses no longer become inferred file writes. OpenAI-compatible native tools, usage and stream errors now reach the existing host execution pipeline, with bounded correction of multiple calls and bounded outer gateway retries. Rust runtime and Cargo dependencies are unchanged. See [release notes](docs/RELEASE_RUNTIME_0.4.41.md).

## Runtime 0.4.40 — prevent repeat Gemini empty-action submissions

Pins merged Agent CLI 0.88.51 at SHA `04c57fd2e4efff892d0373e71f2eb2805827ca42` in Python dependency range, `uv.lock`, and both CI checkouts. The Agent now tries only configured, trusted fallback profiles after the proxy confirms an empty action without host execution; with no working fallback, it marks the contract item BLOCKED_ENV instead of retrying the same Gemini Web request rapidly. Root-relative repository file guidance is clarified. The Reverse Proxy remains 5.4.22 with its existing safe no-action detection. Rust/Cargo and sandbox permissions are unchanged. [Release notes](docs/RELEASE_RUNTIME_0.4.40.md).

## Runtime 0.4.39 — recover shell-wrapped Git checkout without approval loops

Pins merged Agent CLI 0.88.50 at SHA `168e21b396e72163b45ed6cd0e83e8115c446d46` with a synchronized Python minimum dependency, uv lock and both CI Agent checkouts. The runtime prevents the model from triggering generic network approvals for public Git clones bundled in `bash -c`, instead returning actionable guidance to invoke separate workspace directory, Git init and narrow direct HTTPS clone tools. General network shell commands still require explicit approval. Rust daemon, Cargo locks and existing sandbox restrictions remain unchanged. [Release notes](docs/RELEASE_RUNTIME_0.4.39.md).

## Runtime 0.4.38 — recover protected Git planning scopes

Pins merged Agent CLI 0.88.49 at immutable SHA `7c1fe6dc56664fc291077b9ec99d599ba84e3755` in the Python runtime dependency, uv lock and both CI checkouts. Invalid model-generated `.git` validation scopes now trigger a bounded contract revision instead of terminating the autonomous goal; direct workspace metadata access remains forbidden. Reverse Proxy 5.4.22 independently rejects empty Gemini actions without speculative browser repairs. Rust daemon and Cargo locks remain unchanged. [Release notes](docs/RELEASE_RUNTIME_0.4.38.md).

## Runtime 0.4.37 — Agent CLI 0.88.48 nested Git inspection fix

Pins merged Agent CLI 0.88.48 at SHA `9f612193689006d79b2106a88b98e3f1633ac1a4` in Python package dependency, `uv.lock`, and both CI checkouts. Fixes read-only `git -C mbs-wine status` being denied by PolicyEngine in autonomous contract runs. No changes to Rust runtime, Cargo.lock or Reverse Proxy. The independent Reverse Proxy child-session capacity rejection at 1.10 GB versus 768 MB is still a resource constraint, not resolved by this release. [Release notes](docs/RELEASE_RUNTIME_0.4.37.md).

## Runtime 0.4.36 — autonomous Git network delegation in Agent CLI 0.88.47

Pins the immutable Agent CLI 0.88.47 main commit `e2065939b5b798073130854c88b5ba281f672ffd` in Python dependency, uv.lock and both CI checkouts. The Agent's default workspace autonomous mode can fetch source repositories from GitHub/GitLab via restricted HTTPS Git commands in a strong sandbox without opening a recurring approval modal. All other network operations, unsafe paths, child principals and weaker sandboxes retain capability/approval requirements. Does not change native daemon, Rust locks, CDP transport or reverse-proxy memory policy. [Notes](docs/RELEASE_RUNTIME_0.4.36.md).

## Runtime 0.4.35 — Agent CLI 0.88.46 SQLite index startup recovery

Pins Agent CLI 0.88.46 at immutable merged main SHA `737ad5cefaaa4a04e8db22b910f3513194505f56` in Python dependencies, `uv.lock` and both CI checkout references. The Agent now recovers from persistent repository index SQLite writer contention by opening a non-persistent in-memory index for the contending KITT process. Original disk index and WAL are untouched, and permanent database errors still propagate. Rust runtime 0.1.61, Cargo.lock and other component pins remain unchanged. [Release notes](docs/RELEASE_RUNTIME_0.4.35.md).

## Runtime 0.4.34 — Agent CLI 0.88.45 Begin Patch navigation

Pins the immutable Agent CLI 0.88.45 main commit (`eb11a323`) across the Python dependency range, uv.lock and both CI checkouts, retaining other component locks and Rust runtime 0.1.61. This includes safe handling of context-only `@@` navigation sections in model-generated `*** Begin Patch` updates, following the Wine-MBS incident of 21 October 2026. The parser still rejects no-op-only patches, unanchored insertions and out-of-workspace targets. Network process permissions and unattributed WebChat capture are unaffected. [Release notes](docs/RELEASE_RUNTIME_0.4.34.md).

## Runtime 0.4.33 — Agent CLI 0.88.43 Begin Patch compatibility

Pins Agent CLI 0.88.43 (`21706d0`) in Python dependency range, `uv.lock` and both immutable CI checkouts. The Agent now accepts bounded content-anchored `*** Begin Patch` Update File hunks through its existing secure patch engine, avoiding the parse failure observed in the Wine-MBS win32u run. The separate ChatGPT Web `ui_capture_unverified` safety rule remains unchanged. Rust runtime 0.1.61 and Cargo.lock are unchanged. [Notes](docs/RELEASE_RUNTIME_0.4.33.md).

## Runtime 0.4.32 — pin Agent CLI 0.88.42 Goal checkpoints

Synchronizes the Assistant Python dependency, uv lock and both immutable CI checkouts with Agent CLI 0.88.42 (`de7c0ad`). This includes automatic Goal budget-checkpoint continuation without consuming a failed item attempt, plus earlier Agent CLI 0.88.41 patch parsing and batch diagnostic fixes. A new regression asserts consistency of the dependency range, immutable lock SHA, CI Agent revisions and Assistant package version. The Rust runtime remains at 0.1.61 and Cargo.lock is unchanged. [Notes](docs/RELEASE_RUNTIME_0.4.32.md).

## Runtime 0.4.31 — durable Goal read-graph evidence and scoped memory

Pins Agent CLI 0.88.40 (`78d7eaa`), with Goal evidence surviving bounded flow/program reads and semantic recall focused on the original task rather than Goal scaffolding. Python dependency, uv.lock and immutable CI Agent checkouts are synchronized. The existing Rust runtime stays at 0.1.61 with Cargo.lock unchanged. [Notes](docs/RELEASE_RUNTIME_0.4.31.md).


## Runtime 0.4.30 — Agent CLI 0.88.39 verification diagnostics

Pins Agent CLI 0.88.39 (`f915302`) to ensure unavailable Wine/C builds are surfaced with actionable host diagnostics rather than generic `verification failed`, while retaining safe typed WebChat continuation and read-only flow support. Updates Python runtime dependency, uv.lock and both immutable CI Agent checkouts. Rust 0.1.61/Cargo.lock remain unchanged. [Notes](docs/RELEASE_RUNTIME_0.4.30.md).


## Runtime 0.4.29 — typed WebChat continuation and Agent 0.88.38

Pins Agent CLI 0.88.38 (`0d28f07`), including safe GoalScheduler handling of `ui_capture_unverified` from Reverse Proxy 5.4.17. Keeps 0.88.37 host flow and C/Wine verification improvements, updates Python package, uv.lock and both immutable Agent CI checkouts. Rust components remain 0.1.61 and Cargo.lock unchanged. [Notes](docs/RELEASE_RUNTIME_0.4.29.md).


## Runtime 0.4.28 — Agent CLI 0.88.37 verification and flow guidance

Pins Agent CLI 0.88.37 (`2a8aaad`) with model-visible read-only flow/program tooling, bounded Goal evidence reuse, empty memory diagnostics, independence-aware planning, and strict Makefile-backed C/C++ verification. Updates Python package metadata, uv.lock and both immutable CI Agent checkouts. Rust remains 0.1.61 and Cargo.lock unchanged. [Notes](docs/RELEASE_RUNTIME_0.4.28.md).


## Runtime 0.4.27 — Agent CLI 0.88.36 Bubblewrap resolver fix

Pins Agent CLI 0.88.36 (`d76ef6a`), fixing sandbox startup on hosts where `/etc/resolv.conf` is a symlink into `/run` after private runtime socket masking. Python package metadata, uv.lock and two immutable Agent checkouts in CI now agree. The Rust workspace stays on 0.1.61 with Cargo.lock unchanged; no tool authorization or host policy changes. [Notes](docs/RELEASE_RUNTIME_0.4.27.md).


## Runtime 0.4.26 — pin Agent CLI 0.88.35 automatic Bubblewrap selection

Updates the Python runtime package and lock to Agent CLI 0.88.35 (`89d30ef`), aligning both immutable Agent source checkouts in CI. Functional Bubblewrap discovery now checks /usr/local/bin, /usr/bin, /bin and PATH instead of stopping at the first binary, and surfaces host AppArmor namespace denials. Existing host sandbox and approval policies remain unchanged; Rust 0.1.61 and Cargo.lock do not require modification. [Notes](docs/RELEASE_RUNTIME_0.4.26.md).


## Runtime 0.4.25 — verified host-write review paths

Pins Agent CLI 0.88.34 (`66e8b49`) to include canonical `repo.write_file` mutations in Goal review snapshots, with Markdown reports eligible for verification. All bounded durable host-evidence reuse from 0.88.33 remains present. Updates Python dependency, uv.lock and the two immutable CI source pins; Rust 0.1.61 and Cargo.lock are unchanged. [Notes](docs/RELEASE_RUNTIME_0.4.25.md).


## Runtime 0.4.24 — reuse previous Goal host evidence

Pins Agent CLI 0.88.33 (`dc1333b`) with bounded durable host-read evidence across Goal retries, selective mutation invalidation and partial batch recovery. The Python runtime version, dependency floor, uv.lock and both CI immutable Agent checkouts are synchronized. Rust 0.1.61 and Cargo.lock remain unchanged. [Notes](docs/RELEASE_RUNTIME_0.4.24.md).


## Runtime 0.4.23 — Agent CLI 0.88.32 no-progress and blocker classification

Pins Agent CLI 0.88.32 (`f399bf6`) with read-only batch stall detection and precise POLICY/ENVIRONMENT categorization of unrecoverable host denials. Synchronizes Python package, uv.lock and both immutable CI checkouts. Rust 0.1.61, its Cargo.lock and daemon wire contracts remain unchanged. [Notes](docs/RELEASE_RUNTIME_0.4.23.md).


## Runtime 0.4.22 — Agent CLI 0.88.31 sandbox preflight

Pins Agent CLI 0.88.31 (`fbb6029`) to expose required host strong-sandbox availability in the model's tool catalog and runtime capabilities before subprocess attempts. Aligns Python runtime metadata, uv.lock and immutable CI Agent source checkouts. Rust daemon remains 0.1.61 with unchanged Cargo.lock; no new privileges or implicit sandbox bypass. [Notes](docs/RELEASE_RUNTIME_0.4.22.md).


## Runtime 0.4.21 — Agent CLI 0.88.30 reliability backlog

Pins Agent CLI 0.88.30 (`dd63d72`) with corrected autonomous policy defaults, retained specialist egress validation, bounded context rules, precise host retry categories, batched read/search operations and a shadow-only local decision benchmark. Updates runtime metadata, Python uv.lock, both immutable CI Agent checkouts and release notes. Rust remains 0.1.61 and Cargo.lock is unchanged. [Notes](docs/RELEASE_RUNTIME_0.4.21.md).


## Runtime 0.4.20 — resumable Agent budget checkpoints (0.88.28)

Pins Agent CLI 0.88.28 (`ca28fbf`) with budget checkpoint recovery for durable Goal contracts, bounded batch ID parsing and more realistic large-task planning. Aligns Python package metadata, `uv.lock` and immutable Agent source checkouts in CI. Rust components remain 0.1.61 with unchanged Cargo.lock and no new authority grants. [Notes](docs/RELEASE_RUNTIME_0.4.20.md).


## Runtime 0.4.19 — Agent CLI 0.88.27 host progress and transport safety

Pins Agent CLI 0.88.27 (`67bc827`) for non-repeated missing-sandbox actions, mixed failure/observation cycle detection and non-replayable ambiguous WebChat timeouts. Updates Python dependency, uv.lock and both immutable CI checkouts. The Rust workspace remains at 0.1.61 with unchanged Cargo.lock; no daemon protocol or policy grants are altered. [Notes](docs/RELEASE_RUNTIME_0.4.19.md).


## Runtime 0.4.18 — Agent CLI 0.88.24 plan-contract diagnostics

Pins Agent CLI 0.88.24 (`58ffe92`, PR #130) for explicit plan.submit proposal guidance, missing-plan diagnostics and actionable host sandbox errors. Updates Python runtime metadata and uv.lock, plus immutable CI checkout refs. The Rust workspace and Cargo.lock are unchanged at 0.1.61; existing approval authority and strong host sandbox requirements remain intact. [Notes](docs/RELEASE_RUNTIME_0.4.18.md).

## Runtime 0.4.17 — Agent CLI 0.88.23 approval integration

The standalone Python daemon consumes the audited Agent CLI 0.88.23 source (`fa1ab23`, PR #129), with workspace-wide allow-all, scoped tool-only approvals, and strict separation of user approval from host sandbox/capability enforcement. Updates the Python lock and CI source pins; the Rust workspace remains at 0.1.61 with its unchanged Cargo.lock. [Notes](docs/RELEASE_RUNTIME_0.4.17.md).

## 0.1.61 / Runtime 0.4.16 — default autonomous workspace / guest policy

Pins Agent CLI 0.88.22 at `53a8a22` (PR #128). New workspaces start with autonomous coding preferences, while saved ASK/DENY choices remain authoritative. Host subprocesses automatically launched by the agent must have a strong OS sandbox; authenticated guest/VM channels retain their own constraints. Updates Rust/Python version locks, workflow pins and integration tests. [Notes](docs/RELEASE_0.1.61.md).


## 0.1.60 / Runtime 0.4.15 — Agent CLI 0.88.21 release source alignment

Updates the immutable Agent CLI checkout/uv pin to `0484f2e` (PR #127), including the corrected source-tree fallback version. Bumps Rust workspace/package versions to 0.1.60 and Assistant runtime to 0.4.15; CI now checks against the exact release-candidate source. No authority or daemon behavior changes from 0.1.59. [Notes](docs/RELEASE_0.1.60.md).


## 0.1.59 / Runtime 0.4.14 — daemon autonomy readback and approval identity

Expose a read-only `runtime.get_autonomy` to prevent the TUI from clobbering the persisted daemon policy on attach; include trusted executor identity in persisted approval queue results, allowing scoped remembered approvals to match across refreshes. [Notes](docs/RELEASE_RUNTIME_0.4.14.md).


## 0.1.58 / Runtime 0.4.13 — Wine Staging toolbox pin

Pins Agent 0.88.13 with Wine/Wine64/Wine-Staging image 0.86.11 and safe Linux lab upgrade. Cargo/Python locks aligned, no new guest authority. [Notas](docs/RELEASE_0.1.58.md).

## 0.1.57 / Runtime 0.4.12 — Pin Agent 0.88.12 for RE guest MCP

Updates immutable Agent revision and all affected Python/Rust metadata. RE guest MCP is opt-in/read-only and does not expand Assistant authority. [Notas](docs/RELEASE_0.1.57.md).

## 0.1.56 / Runtime 0.4.11 — Agent/Protocol pin reconciliation

Aligned Agent 0.88.11, Protocol 0.14.2, Rust/HUD npm and Python locks; no daemon contract change. [Notas completas](docs/RELEASE_0.1.56.md).

## Notas de versão migradas do README

## Assistant 0.1.55 / Runtime 0.4.10 — Agent 0.88.10 Clean-Room RE

Pins offline Clean-Room orchestration without expanding Assistant authority. [Release notes](docs/RELEASE_0.1.55.md).

## Assistant 0.1.54 / Runtime 0.4.9 — bounded Clean-Room RE support

Pins Agent 0.88.9 and Protocol 0.14.1 for reviewed black-box contract sealing and optional networkless OCI implementer execution. Operational certification is not implied. [Release notes](docs/RELEASE_0.1.54.md).

## Assistant 0.1.51 / Python Runtime 0.4.6 — Windows Oracle Docker bootstrap

Consumes Agent CLI 0.88.6 including operator-authorized ephemeral Windows/Dockur OEM bootstrap, Windows OpenSSH forced-command probe runner and fixed benign Win32/Wine comparison. Rust workspace crates and Cargo lock updated to 0.1.51; runtime metadata and uv lock updated to 0.4.6. No Rust wire-contract, Protocol or daemon code is changed.

[Release notes](docs/RELEASE_0.1.51.md).

## Assistant 0.1.50 / Runtime 0.4.5 — Docker RE lab lifecycle in Agent CLI 0.88.5

Consumes Agent CLI 0.88.5 for automatic Docker/Podman engine choice, safe discovery and operator-controlled reuse of installer-owned Linux and Dockur Windows RE containers. Runtime version, workspace Cargo version, Cargo and Python locks, CI Agent commit pins and release notes are aligned. The daemon/Rust APIs, HUD and shared Protocol wire remain unchanged; an authenticated Windows probe channel is still required.

[Release details](docs/RELEASE_0.1.50.md).

## Assistant 0.1.49 / Runtime 0.4.4 — Agent CLI 0.88.4 integration

Pins Agent CLI 0.88.4 at an immutable commit, updates the Python runtime constraint and uv lock, and aligns standalone Assistant CI with the same release. Durable approval registration, cancellation cleanup and bounded Goal parallelism are provided by Agent CLI; Rust, daemon APIs, Protocol, Memory and HUD dependencies are unchanged.

[Release details](docs/RELEASE_0.1.49.md).

## Assistant 0.1.48 / Runtime 0.4.3 — pin complete Agent approval flow

Requires Agent CLI 0.88.3. Its autonomous process policy now honors inline interpreter commands, pending actions retain the concrete host tool, and contract Goal approvals resume exactly once after evidence persistence. This closes the follow-up modal/lease errors that were missed by tests using a versioned interpreter path and manually constructed pending actions.

[Release details](docs/RELEASE_0.1.48.md).

## Assistant 0.1.47 / Runtime 0.4.2 — acknowledge approved decisions after execution failure

The daemon now acknowledges a valid Goal approval decision even when execution fails. It reports `execution_status=failed` and `execution_error` separately, so the TUI can display the failure and close the decided request. Rejected/unknown approvals remain RPC errors. Successful Goal approvals resume the same durable contract item.

[Release details](docs/RELEASE_0.1.47.md).

## Assistant 0.1.46 / Runtime 0.4.1 — bounded Agent planning

Runtime requires Agent CLI >=0.88.1,<0.89 and locks its exact planning fix commit. CI exercises the same Agent revision. Automatic plans are checked locally with at most one structural correction; independent verification of executed tasks remains active. Native crate versions follow the coordinated release; HUD and third-party dependencies are unchanged.

[Release details](docs/RELEASE_0.1.46.md).

## Assistant 0.1.45 / Runtime 0.4.0 — JSON ecosystem

Align the Protocol and Agent dependency locks with Agent contract v5 and the JSON model wire migration. Runtime requires Agent CLI >=0.88.0,<0.89. Native domain contracts remain typed JSON; the application does not load the archived KAP parsers. Update the full KITT composition together.


[Release details](docs/RELEASE_0.1.45.md).

## Assistant Runtime v0.3.36 — Agent semantic release integration

The Assistant runtime now validates Agent CLI `0.87.29` from immutable main commit `cb530c2ecddaafa1241e957c3d379ea853f43863`, including the opt-in multilingual semantic stage. The runtime lock was refreshed without changing Rust packages or the protocol contract. Ollama remains optional and user-managed. [Release notes](docs/RELEASE_RUNTIME_0.3.36.md).

## Assistant Runtime 0.3.34 — stage-aware Agent planning UX

Pins Agent CLI **0.87.27** at `33c83bbb6b05d06ea4dc91020ac9d7e2f78dfed3`, with explicit planning/review/revision attempt status, elapsed provider wait and no fabricated planning percentage. No change to the Rust assistant API or wire protocol. [Release note](docs/RUNTIME_0.3.34.md).

## Assistant Runtime 0.3.33 — planner contract convergence

Python runtime CI and lock pin Agent CLI **0.87.26** at `cea70a4227b3ba66455ef968cddcbc3468f65e61`. This prevents pre-mutation contract review from receiving an implicitly expanded FINAL step; individual task verification and host policy remain unchanged. See [release note](docs/RUNTIME_0.3.33.md).

## Assistant Runtime 0.3.32 — independent specialist LLM profiles

Python runtime lock and CI now pin Agent CLI **0.87.25** at `4138895c84b673c8dfef54ddb8f7276ee4ca1eb4`. Specialist workers can independently use registered direct or reverse-proxy model profiles, including bounded fallback and privacy restrictions. Native Assistant/Protocol APIs are unchanged. See [runtime release notes](docs/RUNTIME_0.3.32.md).

## Assistant Runtime 0.3.31 — specialist Agent integration

Python runtime lock and CI checkouts pin Agent CLI **0.87.23** at `4e643b11f00902d7afc7ee2e6315b4724cfec4f4`, providing specialized plugins, team collaboration and the new Agent Manager. Native runtime and Protocol remain unchanged. See [runtime release](docs/RUNTIME_0.3.31.md).

## Assistant Runtime 0.3.30 — indexed lexical routing and graph edits

Python uv.lock/CI now pin Agent CLI 0.87.22 (`7a010b2dfba3e3356b7b49aeb32186b69605df7f`). Native Assistant 0.1.44 and Memory 0.10.2 remain unchanged. [Release notes](docs/RUNTIME_0.3.30.md).

## Assistant Runtime 0.3.29 — nonblocking Agent repository index

The Python runtime lock and CI compatible Agent pin now consume Agent CLI 0.87.21 (`872c7f89d6a9dcce8e014b9141ee507fb9d234e3`) with shorter SQLite index transactions. Native Assistant remains 0.1.44, pinned to Memory 0.10.2. [Notes](docs/RUNTIME_0.3.29.md).

## Assistant 0.1.44 / Runtime 0.3.28 — guarded vector memory

Native Cargo.lock now pins Memory 0.10.2 at `e236c18323f2e8cdb839d02710eaf7514a060c8e`; Python runtime lock and compatibility CI pin Agent CLI 0.87.20 at `bae99751b082523bc3d44bc26f249f085a25fc3a`. Release-channel Assistant lock validation and revision-safe vector backfill are aligned. No change to host IPC or trust. [Notes](docs/RELEASE_0.1.44.md).

## Assistant 0.1.43 — Rust Memory 0.10.0 release-pin correction

The native `Cargo.lock` now resolves `kitt-memory-core` and `kitt-memory-sqlite` at version 0.10.0 and immutable SHA `eee55669ef7400b2c1f31b6e658a28f018b77723`, matching KITT 0.15.42. This unblocks release-channel installation using `--locked`; no host API or memory policy changes. [Release notes](docs/RELEASE_0.1.43.md).

## Assistant Runtime 0.3.27 — optional hybrid memory Agent alignment

Python runtime package, uv.lock and CI compatible-Agent checkout now pin Agent CLI **0.87.19** at `5d6f6b2ac4a10826ab7b551694146928e06f2023`. This exposes optional local Ollama vector-memory queries through the Agent without changing native Rust Assistant and Protocol APIs. See [runtime release notes](docs/RUNTIME_0.3.27.md).

## Assistant Runtime 0.3.26 — Agent CLI index performance alignment

The Python runtime lock and compatible-agent CI checkouts now pin Agent CLI **0.87.18** at `7d8e821352964addb9ab25acef8aa9f96f3461f6`. The native Assistant, Protocol and host IPC wire contracts are unchanged. See [runtime release notes](docs/RUNTIME_0.3.26.md).

## Assistant 0.1.41 / Runtime 0.3.21 — Protocol lock alignment

Align both Rust `Cargo.lock` files with Protocol 0.13.2, Agent CLI 0.87.9 in the Python runtime `uv.lock`, and HUD's package metadata. This repairs the root installer release-pin mismatch without changing runtime behavior. See [release 0.1.41](docs/RELEASE_0.1.41.md).

## Python runtime 0.3.3 — Agent validation fixes

The companion requires Agent CLI 0.86.2 and pins its tested commit in uv.lock. Native/HUD 0.1.34 is unchanged. See [release notes](docs/RELEASE_RUNTIME_0.3.3.md).

## Python runtime 0.3.2 — Agent 0.86.1 parallel subagents

The Assistant companion Python runtime now pins validated Agent CLI **0.86.1** in `uv.lock` and CI. The native Assistant and Protocol remain **0.1.34** and **0.11.0**, respectively. This synchronization supports KITT ecosystem 0.15.2 release-channel installation with automatic parallel Goal children, live agent monitoring and direct-argv Allow All command regression coverage. See [release notes](docs/RELEASE_RUNTIME_0.3.2.md).

## Release 0.1.33 / runtime 0.3.0 — Agent contract v3 composition

Runtime requires Agent 0.85.x and locks its immutable revision with Protocol 0.10.0. Native Protocol locks and runtime CI match the structured-result composition. See [release notes](docs/RELEASE_0.1.33.md).

## Release 0.1.32 / runtime 0.2.45 — WebChat token ownership

Runtime locks and CI select Agent CLI 0.84.10, which delegates reverse-proxy token limits to WebChat. Remote context telemetry shows WebChat ownership without a fabricated local capacity.

See [release notes](docs/RELEASE_0.1.32.md).

## Release 0.1.31 / runtime 0.2.44 — Immediate lifecycle events and cancellation

The daemon flushes TurnStarted and thinking lifecycle events immediately in execution and continuation paths. Cancellation owns a single terminal notification; late producer events cannot appear after the cancellation. The runtime aligns its Agent and Protocol locks with this release.

See [release notes](docs/RELEASE_0.1.31.md).

## Release 0.1.30 / runtime 0.2.43 — Agent CLI 0.84.8 progress alignment

Python runtime **0.2.43** locks Agent CLI **0.84.8** at `809151b40ef4600c8364c73583bd38717b1665cf`. Daemon-owned `mode=auto` turns now forward bounded inner Goal progress to the outer user turn, so planning, context, tool activity and edits remain visible while durable contract items execute.

Agent 0.84.8 also bounds interactive memory recall and makes temporary kitt-memoryd unavailability fail-soft for prompt enrichment. Assistant daemon, IPC, Protocol and native contracts are unchanged.

## Release 0.1.29 / runtime 0.2.42 — Agent CLI 0.84.7 TUI alignment

Python runtime **0.2.42** locks Agent CLI **0.84.7** at `f9c07452dd211359e754c2b9a1eb54e3854c41a1`. The TUI now leaves the home screen immediately when the first prompt is submitted, before daemon attachment and automatic-contract bootstrap finish, so daemon-owned `mode=auto` execution exposes `STARTING` progress instead of appearing frozen.

No Assistant daemon, IPC, Protocol or Memory contract changed.

## Release 0.1.28 / runtime 0.2.41 — Agent CLI 0.84.6 loop alignment

Python runtime **0.2.41** locks Agent CLI **0.84.6** at `0f14d68b5fbdd6848660f68707230d7d6c3db584`. This brings the complete Goals/TaskPlan ownership fix into daemon-owned `mode=auto` execution: GOAL items without a nested TaskPlan no longer receive TaskPlan host-verification context or its completion gate, leaving post-turn checks to `GoalStepVerifier`.

The stale-daemon startup-identity fix from 0.1.27/runtime 0.2.40 remains unchanged. Protocol, Memory and native daemon contracts are unchanged.

## Release 0.1.27 — stale daemon rejection and Agent CLI 0.84.4

Python runtime **0.2.40** freezes both the Agent and Assistant-runtime versions when the daemon process starts. The authenticated ping exposes that immutable startup identity, and clients reject daemons that do not report the exact current runtime identity. This fixes in-place upgrades where old daemon code stayed resident while `importlib.metadata` began reporting newly installed package versions.

An incompatible authenticated daemon is recycled by the existing daemon bootstrap path; no second lifecycle manager was added. The runtime lock and CI pin Agent CLI **0.84.4** at `fc985bc6d842d3c684ad1188ff2fc8a0c427e6a8`, including durable-loop hardening. See [release notes](docs/RELEASE_0.1.27.md).

## Release 0.1.26 — Agent CLI 0.84.3 managed Proxy logging alignment

Python runtime **0.2.39** locks Agent CLI **0.84.3**, including the stale Reverse Proxy control-plane refresh that makes managed Proxy logging honor the Agent log directory after upgrades. Native daemon, Protocol, Memory and IPC contracts are unchanged.

## Release 0.1.25 — automatic contract daemon routing

Python runtime **0.2.38** routes persisted daemon-owned `mode=auto` turns through Agent CLI **0.84.2**'s durable Plan → Execute → Validate → Retry contract wrapper. Explicit `ask`, `plan` and no-history turns remain direct TurnProcessor turns.

Goal-owned approvals are resolved inside the daemon and resume the same contract item instead of creating a disconnected foreground continuation. Cancellation of the outer daemon turn cancels the durable contract and its active inner turn. The Agent lock is pinned to the validated 0.84.2 main revision. See [release notes](docs/RELEASE_0.1.25.md).

## Release 0.1.24 — Agent CLI 0.84.1 verification refactor alignment

Python runtime **0.2.37** locks Agent CLI **0.84.1** at its validated main revision. The supported Agent range remains `>=0.81.0,<0.85`; native daemon, Protocol and Memory contracts are unchanged. See [release notes](docs/RELEASE_0.1.24.md).

## Release 0.1.23 — Agent CLI 0.84 contract compatibility

Python runtime **0.2.36** expands its Agent compatibility range to `>=0.81.0,<0.85` and locks Agent CLI **0.84.0** at its validated main revision. The native daemon/control-center behavior and shared Protocol/Memory contracts are unchanged; this release is a consumer-alignment bump only. See [release notes](docs/RELEASE_0.1.23.md).

## Release 0.1.22 — unified model selection endpoint trust

Python runtime 0.2.35 locks Agent CLI 0.83.19, covering both managed Reverse Proxy role binding and the general model picker. Runtime and lifecycle CI use the same immutable Agent revision. See [release notes](docs/RELEASE_0.1.22.md).

## Release 0.1.21 — managed proxy endpoint trust in the locked Agent

Python runtime 0.2.34 locks Agent CLI 0.83.18 at its validated main revision, so frozen Assistant environments include the fix for explicitly selected Reverse Proxy endpoints on newly allocated ports. Native workspace metadata is 0.1.21; Protocol and other dependencies retain their locked revisions. See [release notes](docs/RELEASE_0.1.21.md).

## Release 0.1.20 — consistent Protocol consumer locks

Align the HUD npm lock with Protocol 0.9.1 at the same immutable revision used by both Cargo locks and the Python runtime lock. A regression checks all four Protocol consumers together in the runtime CI suite. Native workspace version is 0.1.20; Python runtime remains 0.2.33 and HUD package metadata remains unchanged.

See [release notes](docs/RELEASE_0.1.20.md).

## Release 0.1.19 — execution boundary hardening

Publish Python runtime 0.2.33 with authoritative connection health, explicit ConnectionError on EOF (request outcome may be unknown), awaited reader cleanup and reconnect-ready resync state. Pair with Agent 0.83.16, Protocol 0.9.1 and Memory 0.9.2.

See [release notes](docs/RELEASE_0.1.19.md).

## Assistant 0.1.18 / Python runtime 0.2.32

Validated with Agent CLI **0.83.12**, Protocol **0.9.0** and native Memory **0.9.1**. Assistant 0.1.18/runtime 0.2.32 makes resident lifecycle fail-closed: daemon readiness is reported only after the required Agent runtime starts, native service commands propagate OS failures, and lifecycle/correlation state is observable without taking policy or memory authority from Agent/Memory. The companion keeps the reviewed Agent range `>=0.81.0,<0.84`; HUD package version metadata remains unchanged, while the native HUD Protocol dependency is aligned to Protocol 0.9.0.

<p align="center">
  <strong>Resident local assistant, control center and voice/HUD runtime for K.I.T.T.</strong><br>
  Rust daemon · authenticated loopback IPC · web Control Center · voice · ephemeral HUD · Python runtime
</p>

<p align="center">
  <a href="https://github.com/rfdetoni/kitt-assistant/blob/main/LICENSE"><img alt="License MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
  <img alt="Rust" src="https://img.shields.io/badge/Rust-resident%20daemon-000000?logo=rust&logoColor=white">
  <img alt="Loopback" src="https://img.shields.io/badge/network-loopback--first-6f42c1">
  <img alt="Control Center" src="https://img.shields.io/badge/UI-Control%20Center-3178C6">
</p>

K.I.T.T. Assistant is the long-lived local runtime of the K.I.T.T. ecosystem. It provides a low-overhead native daemon, authenticated IPC, the K.I.T.T. Control Center web application, voice orchestration, ephemeral desktop HUD output and the separately packaged Python runtime used by K.I.T.T. Agent for daemon/remote integration.

The implementation follows Clean Architecture boundaries:

```text
domain <- application <- infrastructure <- apps
```

---

## Assistant 0.1.5 / runtime 0.2.17 — consistency hardening

- The Python daemon protocol now imports one `DAEMON_PROTOCOL_VERSION` authority from the Agent package. Handshake metadata and `DaemonEvent.protocol_version` can no longer drift independently.
- HUD event fan-out snapshots subscriber sockets under the mutex and performs bounded socket writes after releasing it, so a slow HUD client does not stall all subscribers or subscription management.
- Rust memory dependencies historically used fixed revisions; current Assistant manifests follow `kitt-memory` `main`.
- Standalone Python-runtime CI uses an immutable known-compatible Agent 0.74.4 SHA rather than mutable `main`; the root ecosystem integration remains authoritative for the exact promoted component set.

## Assistant 0.1.6 / runtime 0.2.18 — current-interpreter alignment

- Python runtime metadata now requires Python 3.14+, matching the interpreter continuously validated by the ecosystem.
- Rust daemon, CLI, HUD and CI are pinned to KITT Protocol 0.2.1; wire protocol v1 remains unchanged.
- HUD package/Tauri metadata and Cargo/npm locks are aligned with the same 0.1.6/Protocol 0.2.1 snapshot.

## Assistant 0.1.7 / runtime 0.2.19 — semantic Surface renderer

Assistant now consumes KITT Protocol 0.3.0 and Memory 0.3.0. The Python daemon negotiates the host Surface component catalog, exposes a narrow semantic action route and persists `SurfaceAction` events. Remote Web renders validated Surface snapshots using a dedicated declarative renderer with bounded graph traversal and safe text DOM APIs; buttons send only semantic actions back to the daemon. Arbitrary model HTML/JavaScript and generic browser-side SafeRuntime execution remain unsupported.

## 0.1.8 — optional native voice build

The resident Assistant no longer requires Linux ALSA development packages merely to install the KITT ecosystem. The `kittd` voice-capture stack is now the Cargo feature `voice` (enabled by default for normal source builds), while the installer may compile `kittd` with `--no-default-features` when Linux native audio development dependencies are unavailable.

Disabling the build feature only removes resident microphone capture/wake-word processing. Core Assistant APIs, model routing, memory, Control Center, HUD transport, transcription requests and system TTS abstractions remain available.

## 0.1.9 / runtime 0.2.20 — external memory authority

The Assistant remains an optional UX/runtime component. Durable Agent memory is no longer hosted by Assistant/kittd: Agent CLI 0.76+ talks to standalone `kitt-memoryd`, owned by the kitt-memory repository. Assistant runtime compatibility is aligned to Agent 0.76.x without becoming a dependency of the minimal Agent + Reverse Proxy installation.

## 0.1.10 / runtime 0.2.21 — Agent 0.77 compatibility

The Python companion runtime now accepts `kitt-agent-cli>=0.76.0,<0.78`, covering the promoted Agent CLI 0.77.0 evidence-first execution release while preserving 0.76.x compatibility. Resident daemon ownership, approval durability and memory boundaries are unchanged.

Workspace/HUD version metadata and Cargo/npm locks are aligned to Assistant 0.1.10.

## 0.1.11 — voice-disabled build hotfix

- Fixed `kittd` compilation when the `voice` feature is disabled: Control Center voice-overlay types are now compiled only when `feature = "voice"` is active.
- CI now runs the same `cargo build --workspace --no-default-features --locked` path used by the ecosystem installer when Linux ALSA development headers are unavailable.
- Core Assistant APIs, model routing, memory integration, Control Center, HUD transport, transcription requests and system TTS remain available in the voice-disabled build.
- Assistant workspace and HUD metadata are aligned at `0.1.11`. The Python companion runtime remains `0.2.21` because its code and compatibility contract are unchanged.

## 0.1.12 / runtime 0.2.22 — Agent 0.78 compatibility

- Widen the Python companion runtime range to `kitt-agent-cli>=0.76.0,<0.79` after validating Agent CLI 0.78.0.
- Pin standalone Python-runtime CI to Agent CLI 0.78.0 revision `f51dbba8a0506e90366ddb4e95026dc6c6614699`.
- Align Rust workspace, HUD/Tauri metadata and lockfiles at Assistant 0.1.12.
- Preserve approval durability, protocol versions, memory ownership and daemon/remote API contracts.

## 0.1.13 / runtime 0.2.23 — Memory 0.5 / Agent 0.78.3 alignment

- Pin native Assistant memory dependencies to KITT Memory 0.5.0, keeping standalone `kitt-memoryd` as the durable Agent-memory authority.
- Validate the Python companion runtime against Agent CLI 0.78.3 revision `89a63da16368a59eac5eee185373bfbf89f516b1`.
- Align Rust workspace, HUD/Tauri metadata and lockfiles at Assistant 0.1.13.
- Preserve daemon/remote APIs, approval durability, Protocol 0.4.0 and the existing `kitt-agent-cli>=0.76.0,<0.79` compatibility range.

## 0.1.14 / runtime 0.2.24 — Agent 0.78.4 staged execution

Standalone CI now validates Agent CLI 0.78.4 revision `7d56faec43fa6f5c0e4b1f63c0c18e269a0eb9d8`. The Assistant runtime keeps its existing Agent 0.78 compatibility range while validating the reduced bootstrap/delta reverse-proxy prompt contract.

## Agent 0.78.6 compatibility

Assistant runtime 0.2.24 remains API-compatible with Agent CLI 0.78.6. CI now validates the composed Python namespace against revision `d6eae285b62e22add5b870fbfb14dbab1a6b4186`, including the structural reverse-proxy tool-schema transport.

## Assistant 0.1.15 / runtime 0.2.25 — main-first ecosystem dependencies

Native Assistant dependencies now follow `kitt-memory` and `kitt-protocol` `main` instead of embedding cross-repository SHAs. The HUD follows `kitt-protocol#main`, and the root ecosystem installer refreshes those K.I.T.T. dependencies inside its temporary build checkout before compiling. Component lockfiles remain local build artifacts, not ecosystem revision authorities.

## Release 0.3.37: local multilingual lexical resources

Assistant Runtime 0.3.37 locks Agent CLI 0.87.30 at immutable main SHA 4736660981d3d07f8a29adda4127b652d82e9229. Adds offline lexical compiler support through Agent without changing runtime protocol, Rust crates or execution permissions. Optional UniMorph/WordNet sources remain separately acquired; refer to the Agent SEMANTIC_LEXICON.md for license terms. [Release notes](docs/RELEASE_RUNTIME_0.3.37.md).

---

## Histórico anterior do CHANGELOG

# Assistant 0.1.53 / Runtime 0.4.8 — private RE probe staging

Updates to Agent 0.88.8 private Win32 build/Wine input staging and pins the merged immutable Agent commit. No daemon/IPC protocol changes.

[Release details](docs/RELEASE_0.1.53.md).

# Assistant 0.1.52 / Runtime 0.4.7 — Agent RE Lab autosetup

Pins Agent 0.88.7 for the isolated Wine pilot, canonical Docker/OEM lab lifecycle and HMAC scoped reset. No cross-language daemon protocol changes. Windows Oracle remains unverified without authorized host E2E.

[Release details](docs/RELEASE_0.1.52.md).

# Assistant 0.1.49 / Runtime 0.4.4 — Agent CLI 0.88.4 integration

Pins Agent CLI 0.88.4 at an immutable commit, updates the Python runtime constraint and uv lock, and aligns standalone Assistant CI with the same release. Durable approval registration, cancellation cleanup and bounded Goal parallelism are provided by Agent CLI; Rust, daemon APIs, Protocol, Memory and HUD dependencies are unchanged.

[Release details](docs/RELEASE_0.1.49.md).

# Assistant 0.1.48 / Runtime 0.4.3 — pin complete Agent approval flow

Requires Agent CLI 0.88.3. Its autonomous process policy now honors inline interpreter commands, pending actions retain the concrete host tool, and contract Goal approvals resume exactly once after evidence persistence. This closes the follow-up modal/lease errors that were missed by tests using a versioned interpreter path and manually constructed pending actions.

[Release details](docs/RELEASE_0.1.48.md).

# Assistant 0.1.47 / Runtime 0.4.2 — acknowledge approved decisions after execution failure

The daemon now acknowledges a valid Goal approval decision even when execution fails. It reports `execution_status=failed` and `execution_error` separately, so the TUI can display the failure and close the decided request. Rejected/unknown approvals remain RPC errors. Successful Goal approvals resume the same durable contract item.

[Release details](docs/RELEASE_0.1.47.md).

# Assistant 0.1.46 / Runtime 0.4.1 — bounded Agent planning

Runtime requires Agent CLI >=0.88.1,<0.89 and locks its exact planning fix commit. CI exercises the same Agent revision. Automatic plans are checked locally with at most one structural correction; independent verification of executed tasks remains active. Native crate versions follow the coordinated release; HUD and third-party dependencies are unchanged.

[Release details](docs/RELEASE_0.1.46.md).

# Assistant 0.1.45 / Runtime 0.4.0 — JSON ecosystem

Align the Protocol and Agent dependency locks with Agent contract v5 and the JSON model wire migration. Runtime requires Agent CLI >=0.88.0,<0.89. Native domain contracts remain typed JSON; the application does not load the archived KAP parsers. Update the full KITT composition together.

# Changelog

## runtime 0.3.10 — 2026-10-13

- Align Agent 0.86.12 immutable pin and dependency range. REA 6.4.0 workflows now recommend actual MCP tool identifiers, without changing the host approval boundary.
- No daemon/Rust/protocol changes; full Linux RE image remains pinned to tag 0.86.10 because the image contents are unchanged.

## runtime 0.3.9 — 2026-10-12

- Align immutable Agent dependency and CI to Agent 0.86.11, which ensures a private writable HOME/XDG cache for REA/Ghidra tools running with arbitrary non-root UIDs in the full Linux toolbox.
- Reuse prebuilt REA 6.4.0 / Ghidra / Frida image tag 0.86.10; no protocol or daemon changes.

## runtime 0.3.8 — 2026-10-12

- Align Agent CLI dependency to 0.86.10, enabling REA 6.4.0 in the optional full Linux RE toolbox and evidence-backed workflow guidance.
- No changes to assistant daemon, Rust runtime or protocol contracts.

## runtime 0.3.7 — 2026-10-12

- Align Agent dependency and immutable checkout with Agent CLI 0.86.9, hardening QEMU Docker/Podman web authentication, image pins, guest probe channel and offline snapshots.
- No daemon, protocol, or native runtime changes.

## runtime 0.3.6 — 2026-10-12

- Align immutable Agent CLI revision 0.86.8 with Docker/Podman QEMU guests and prebuilt reverse-engineering Linux images.
- Runtime/daemon/protocol contracts remain unchanged.

## runtime 0.3.5 — 2026-10-12

- Align runtime dependency and immutable Agent lock with Agent CLI 0.86.7 (interactive Windows/Linux VM creation helper).
- No daemon, protocol or native Rust changes.

## runtime 0.3.4 — 2026-10-12

- Align Agent CLI >=0.86.6, its immutable uv.lock SHA and the new optional Linux QEMU VM autonomy and reverse-engineering lab.
- Assistant daemon and wire contracts remain unchanged; this is a runtime lock/compatibility update.


## 0.1.32 / runtime 0.2.45 — 2026-10-08

Align runtime locks and CI with Agent CLI 0.84.10 for provider-managed token limits. Remote telemetry labels WebChat context ownership.

## 0.1.31 / runtime 0.2.44 — 2026-10-07

The daemon flushes TurnStarted and thinking lifecycle events immediately in execution and continuation paths. Cancellation owns a single terminal notification; late producer events cannot appear after the cancellation. The runtime aligns its Agent and Protocol locks with this release.

## 0.1.30 / runtime 0.2.43 — 2026-10-07

- Lock daemon/runtime CI to Agent CLI 0.84.8.
- Carry live durable-contract progress into daemon-owned `mode=auto` execution.
- Preserve Assistant daemon, IPC, Protocol and native behavior unchanged.

## 0.1.29 / runtime 0.2.42 — 2026-10-07

- Lock daemon/runtime CI to Agent CLI 0.84.7.
- Carry the immediate first-prompt TUI session transition into daemon-owned execution.
- No Assistant daemon, IPC, Protocol or Memory behavior change.

## 0.1.28 / runtime 0.2.41 — 2026-10-07

- Lock daemon/runtime CI to Agent CLI 0.84.6, including the GOAL-owned durable-contract completion fix.
- Preserve the immutable daemon startup identity introduced in 0.1.27/runtime 0.2.40.
- No Protocol, Memory, IPC or native daemon behavior change.

## 0.1.27 / runtime 0.2.40 — 2026-10-07

- Freeze Agent/runtime compatibility identity at daemon startup instead of rereading mutable distribution metadata on every ping.
- Reject stale resident daemons that lack or mismatch the Assistant runtime startup identity, allowing the existing authenticated bootstrap to recycle them after upgrades.
- Validate and lock Agent CLI 0.84.4, including durable automatic-contract hardening.

## 0.1.20 — 2026-10-06

- Align the HUD npm Protocol lock with the same 0.9.1 revision as native workspace, native HUD and Python runtime locks. The immutable release installer previously rejected the stale 0.9.0 npm pin.
- Add a runtime CI regression comparing Protocol version and immutable revision across all four consumers. Preserve the root installer's mismatch guard.
- Native workspace metadata is 0.1.20; Python runtime remains 0.2.33 and HUD package metadata remains unchanged.

## 0.1.19 — 2026-10-06

- Publish Python runtime 0.2.33 with authoritative connection health, explicit ConnectionError on EOF (request outcome may be unknown), awaited reader cleanup and reconnect-ready resync state. Pair with Agent 0.83.16, Protocol 0.9.1 and Memory 0.9.2.

## 0.1.18 / runtime 0.2.32 — 2026-10-02

- Make Python daemon readiness depend on successful startup of the required Agent runtime; authenticated ping now exposes `starting|ready|degraded|stopping|stopped|failed` lifecycle semantics and a fail-closed `ready` flag.
- Preserve existing request/conversation/turn correlation identifiers in relevant daemon telemetry and native request-error logs.
- Make `kittctl service start|stop|restart|status` propagate native service-manager failures instead of reporting false success; add a narrow Linux/macOS/Windows lifecycle portability matrix for service compilation plus launcher/path/cleanup tests.
- Bump the native Assistant workspace to 0.1.18 and Python runtime to 0.2.32. HUD package version metadata remains unchanged; final ecosystem reconciliation aligns native Memory to 0.9.1, native HUD Protocol to 0.9.0 and the Python validation baseline to Agent CLI 0.83.9.
- Keep Agent CLI as execution/policy/approval authority and `kitt-memoryd` as durable semantic-memory authority; no Protocol or root-`kitt` contract change is included.

## 0.1.17 / runtime 0.2.31 — 2026-10-02

- Refresh the native Rust graph to KITT Memory 0.9.0 and KITT Protocol 0.9.0 without changing protocol wire version 1.
- Validate the Python companion against promoted Agent CLI 0.83.5 revision `47a8e75573f199cea9b465745a540606ea71fbee` and resolve Protocol 0.9.0 in the uv lock.
- Bump the native Assistant workspace to 0.1.17 and Python runtime to 0.2.31; HUD metadata remains 0.1.16 because its behavior and dependency surface are unchanged.
- Preserve ownership boundaries: Agent owns policy/cancellation/wallet/runtime authority, while standalone kitt-memoryd owns durable Agent semantic memory.

## runtime 0.2.30 — 2026-10-02

- Close accepted connections before awaiting the asyncio listener; bound graceful shutdown and abort clients that do not drain. Apply the same rule to startup rollback and await writer task cancellation.
- Validate the entire Python runtime test tree, including the new idle-client shutdown regression and semantic surface actions, against Agent CLI 0.83.3.
- Refresh the main-resolved Python lock. Native workspace/HUD versions remain 0.1.16.
- Review: High severity / High confidence, `kitt/daemon/server.py`: Python 3.14 listener shutdown waits for active clients; the old order could hang indefinitely and retain the instance lock/runtime. Fix: close clients before waiting, five-second grace then abort, bounded writer cleanup. Validation: live daemon/client shutdown, surface round trip and all runtime tests.


## runtime 0.2.29 — 2026-10-02

- Validate the companion runtime against Agent CLI 0.83.0 and widen its compatible range to `<0.84`.
- Discover the entire runtime test tree with pytest, including daemon integration cases in namespace subdirectories missed by unittest discovery.
- Record the main-resolved Python graph for uv consumers. Native workspace/HUD versions are unchanged because their behavior did not change.
- Preserve Agent ownership of policy, wallet, cancellation and prompt construction; Assistant reuses these corrections through its shared control plane.

## runtime 0.2.27 - 2026-10-01

- Widen the reviewed Python companion range to `kitt-agent-cli>=0.81.0,<0.83` and validate the runtime against Agent CLI 0.81.0 revision `5529cbf5c3ddf9e1400408089a36bfaf73b9051d`.
- Keep approval, policy, budget, run coordination, ContextEpoch and workspace snapshot authority in Agent CLI; this is a compatibility-only runtime release.

## runtime 0.2.26 - 2026-09-30

- Require and validate Agent CLI 0.80.x as the companion control-plane contract.
- Forward executable principal identity through daemon `approval.remember` so workspace saved permissions preserve the same USER/GOAL/CHILD scoping as local Agent execution.
- Keep policy, approval, run coordination, budgets, ContextEpochs and workspace snapshots owned by Agent CLI rather than duplicating those authorities in the Assistant runtime.

## 0.1.14 / runtime 0.2.24 / HUD 0.1.14 - 2026-09-29

- Validate the companion runtime against Agent CLI 0.78.4 (`7d56faec43fa6f5c0e4b1f63c0c18e269a0eb9d8`).
- Align workspace and HUD version metadata at 0.1.14.
- Preserve existing daemon, approval and Memory 0.5.0 contracts.


## 0.1.13 / runtime 0.2.23 / HUD 0.1.13 - 2026-09-29

- Pin native memory dependencies to KITT Memory 0.5.0.
- Validate the Python runtime against Agent CLI 0.78.3 (`89a63da16368a59eac5eee185373bfbf89f516b1`).
- Align Rust workspace, HUD/Tauri metadata and lockfiles at 0.1.13.
- Preserve Protocol 0.4.0, standalone memory authority and daemon/remote APIs.


## 0.1.12 / runtime 0.2.22 / HUD 0.1.12 - 2026-09-28

- Widen Assistant runtime compatibility through Agent CLI 0.78.x (`f51dbba8a0506e90366ddb4e95026dc6c6614699`).
- Pin standalone Python-runtime CI to the promoted Agent 0.78.0 revision.
- Align Rust workspace, HUD/Tauri metadata and lockfiles at 0.1.12.
- Preserve daemon/remote APIs, approval durability, Protocol 0.4.0 and standalone Memory ownership.

## runtime 0.2.21 - 2026-09-28

- Widen the companion runtime to Agent CLI 0.76.x–0.77.x while preserving daemon/remote APIs and ownership boundaries.

## 0.1.9 / runtime 0.2.20 - 2026-09-28

- Align the Python runtime with Agent CLI 0.76.x.
- Document standalone kitt-memoryd as the durable Agent-memory authority.
- Keep Assistant optional for Agent + Reverse Proxy installations.


## 0.1.8 - 2026-09-28

- Make resident microphone/wake-word capture a `kittd` Cargo feature named `voice`.
- Keep `voice` enabled for normal source builds while allowing the ecosystem installer to compile a portable non-audio daemon when Linux ALSA development packages are unavailable.
- Preserve Control Center, memory, HUD, model routing, transcription APIs and TTS abstractions in non-audio builds.


## 0.1.7 / runtime 0.2.19 / HUD 0.1.7 - 2026-09-28

- Integrate KITT Protocol 0.3.0 and Memory 0.3.0 and extend Python-runtime compatibility through Agent CLI 0.75.x.
- Add capability-aware declarative Surface rendering to the remote Control Center with bounded component graphs and text-only DOM projection.
- Add a narrow daemon/HTTP semantic `surface.action` path; browser clients never receive a generic `kitt_runtime` execution endpoint.
- Pin standalone Python CI to Agent CLI 0.75.1 and refresh Rust/npm/Tauri locks.

## 0.1.6 / runtime 0.2.18 / HUD 0.1.6 - 2026-09-27

- Require Python 3.14+ for the Assistant Python runtime, matching the single current-interpreter support policy.
- Pin Rust daemon/CLI and HUD to kitt-protocol 0.2.1 while preserving wire protocol v1.
- Refresh Cargo/npm lock metadata and HUD/Tauri package versions for the 0.1.6 snapshot.


## 0.1.5 / runtime 0.2.17 - 2026-09-27

- Unify Python daemon handshake/event protocol version on the Agent's `DAEMON_PROTOCOL_VERSION` authority and add an end-to-end parity test.
- Move HUD socket writes outside the global subscriber mutex so slow clients cannot serialize all event delivery.
- Pin kitt-memory 0.2.1 and refresh the Cargo lock.
- Make standalone Assistant CI reproducible by pinning a known-compatible Agent 0.74.4 revision instead of mutable `main`.


## 0.1.4 / runtime 0.2.16 / HUD 0.1.4 - 2026-09-27

- Pin kitt-memory 0.2.0 and kitt-protocol 0.2.0 across daemon, domain and infrastructure crates.
- Preserve conversation scope keys and point-in-time recall through kittd protocol-v1 handlers.
- Preserve zero-result recall requests instead of coercing limit 0 to 1.
- Update Rust memory struct literals for schema-v4 contracts and expose scope_key in recalled DTOs.
- Extend the Python runtime compatibility bound through kitt-agent-cli 0.74.x.
- Refresh locked dependency graph, CI protocol pin, HUD protocol dependency and npm/Tauri locks.

## 0.2.15 - 2026-09-25

- Extend the Python runtime compatibility range through KITT Agent CLI 0.72.x.
- Keep the upper bound explicit so later Agent contract changes remain reviewable.


## 0.1.0 - Unreleased

- Initial KITT ecosystem foundation.
