# Assistant Runtime 0.3.14 — synchronized CDP stream fixes

Pin Agent CLI 0.86.17 at `ade5b6d4d6f2b82ee5fd58bd0e688fddfdb14132`
in the Python uv.lock and both CI checkouts. Bump Python runtime to 0.3.14
and its minimum Agent version to 0.86.17.

The Agent no longer passes a synthetic 240,000-ms contract deadline when
the durable request budget permits more time. Reverse Proxy 5.1.7 adds
correlated CDP stream liveness and KAP/1 end-marker recovery. No native
daemon or Docker/Podman changes in this release.
