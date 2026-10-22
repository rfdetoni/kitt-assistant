# Assistant Runtime 0.4.42

Pins Agent CLI 0.88.53 (`2ce5c7bb4d4a6dfbfe7a4831dfcdc81c28db665f`) in the Python minimum dependency, uv lock and both CI checkouts.

FreeLLMAPI discovery now uses authenticated `/v1/models`, preserves model provider identity, appears in the model picker, and uses configured workspace endpoints in `kitt models freellmapi --root PATH`. Alternate origins retain explicit endpoint trust requirements.

Agent validation: 696 passed, 4 skipped, 129 subtests passed. Four existing RE/VM tests fail against the active host lab. The installed CLI listed 279 real gateway models. Listing does not certify upstream inference availability.
