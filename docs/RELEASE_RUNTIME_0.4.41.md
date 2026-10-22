# Assistant Runtime 0.4.41

Pins Agent CLI **0.88.52**, commit `bc4694586ee826f057d034701c2e2589960788f6`, in the Python minimum dependency, uv lock and both CI Agent checkouts.

The FreeLLMAPI Wine reproduction exposed read-only plan JSON being inferred as a file replacement, missing native OpenAI tool round trips, discarded stream errors and insufficient output/retry defaults. Agent 0.88.52 addresses these paths without broadening capabilities or sandbox permissions. Multiple native calls are rejected before execution and receive bounded correction within the existing turn; earlier host receipts are retained.

The Rust runtime remains 0.1.61. Free provider capacity, model quality, Wine compilation and actual Windows Oracle comparisons are independent runtime requirements. This pin does not certify Wine implementations.
