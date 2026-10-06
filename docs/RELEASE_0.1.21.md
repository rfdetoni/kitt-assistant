# Assistant 0.1.21 / Python runtime 0.2.34

Refresh the runtime lock to Agent CLI 0.83.18 at b9e8949367a69eac7fa39e09fd6ec627aa228de8. The explicit managed Reverse Proxy role-selection path now trusts the exact selected endpoint before router persistence; frozen Assistant installations must not retain Agent 0.83.17. No daemon wire or runtime source changes are introduced.

Python runtime package and editable lock metadata agree at 0.2.34. Native workspace and its five Cargo package entries agree at 0.1.21; third-party, Protocol/Memory and HUD locks remain unchanged. Existing native, Python, platform and lock-validation CI gates remain required.
