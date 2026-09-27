# Changelog

## 0.1.4 / runtime 0.2.16 - 2026-09-27

- Integrate kitt-memory 0.2.0 schema-v4 contracts and kitt-protocol 0.2.0 while keeping protocol envelope v1.
- Forward conversation scope keys and point-in-time recall through kittd without coercing zero-result limits.
- Return scope keys on memory DTOs and update all internal Rust memory constructors.
- Raise the Rust MSRV to 1.88 to match the shared memory engine.
- Extend Python runtime compatibility through Agent CLI 0.74 and refresh frozen dependency pins/locks.

## 0.2.15 - 2026-09-25

- Extend the Python runtime compatibility range through KITT Agent CLI 0.72.x.
- Keep the upper bound explicit so later Agent contract changes remain reviewable.


## 0.1.0 - Unreleased

- Initial KITT ecosystem foundation.
