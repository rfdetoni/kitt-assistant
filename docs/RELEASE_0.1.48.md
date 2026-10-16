# Assistant 0.1.48 / Runtime 0.4.3 — pin complete Agent approval flow

Requires Agent CLI 0.88.3. Its autonomous process policy now honors inline interpreter commands, pending actions retain the concrete host tool, and contract Goal approvals resume exactly once after evidence persistence. This closes the follow-up modal/lease errors that were missed by tests using a versioned interpreter path and manually constructed pending actions.

Runtime dependency lock and standalone CI use the same Agent revision. The daemon acknowledgment behavior from Runtime 0.4.2 is retained. No Rust source or Protocol contract changes.
