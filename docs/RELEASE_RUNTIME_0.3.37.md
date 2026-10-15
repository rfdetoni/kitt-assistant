# kitt-assistant 0.3.37 — offline lexicon integration

Assistant Runtime 0.3.37 locks Agent CLI 0.87.30 at immutable main SHA 383ffa5bb5c82f8c43f3db82127c41e2e8c6678f. Adds offline lexical compiler support through Agent without changing runtime protocol, Rust crates or execution permissions. Optional UniMorph/WordNet sources remain separately acquired; refer to the Agent SEMANTIC_LEXICON.md for license terms.

Tests: enforce existing package builds, Python and uv lock checks in GitHub Actions. No token-savings claim.
