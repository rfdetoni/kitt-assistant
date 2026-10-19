# Assistant 0.1.58 / Runtime 0.4.13 — Wine Staging RE guest compatibility

Pins Agent CLI 0.88.13 with the full WineHQ Staging RE image and `kitt re lab upgrade-linux --approve`. Cargo workspace and lock, Python runtime pyproject/uv lock, and CI checkout revision are aligned. No new assistant-to-guest privileges: RE Linux image replacement remains operator-approved; existing Windows Oracle and workspace are preserved. The Wine-Staging source checkout is separate from the prebuilt Wine runtime.
