# Assistant Runtime 0.4.34 — Agent CLI 0.88.45

Updates the Python assistant companion from 0.4.33 to 0.4.34. The minimal compatible Agent CLI version is 0.88.45, pinned to the immutable tested `main` SHA `eb11a32323ee397174e2bdb6cbfbd1e2cdd1c3e9` in `uv.lock` and both CI Agent checkouts.

This brings the 21 October 2026 Wine-MBS fix: a model-generated `*** Begin Patch` can include a context-only `@@` navigation section ahead of content-anchored modifications. No tool permissions, Chrome capture provenance, Rust runtime or Cargo.lock behavior changes.

Validation required before merge: Assistant CI, Python dependency/lock consistency, Linux and Windows runtime checks, with immutable release pin consistency in the KITT root release.
