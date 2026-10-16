# Assistant 0.1.49 / Runtime 0.4.4 — Agent CLI 0.88.4

The Python Assistant runtime requires Agent CLI >=0.88.4,<0.89 and resolves its Git source to the immutable Agent `f3c7889ab356441ef87f0726149fd1f337c2f1ee` commit. Both standalone Python CI consumers test exactly the same revision.

This update retains the exact-host-action grant/Goal fencing behavior from Agent CLI 0.88.3 and picks up approval persistence, one-time decision replay safety, cancellation cleanup and collision-safe parallel Goal work from 0.88.4.

Assistant Rust workspace metadata advances to 0.1.49; the five local workspace Cargo.lock package entries agree. No Rust source, shared Protocol schema, Memory, Toolbox, Reverse Proxy or HUD dependency graph changes.
