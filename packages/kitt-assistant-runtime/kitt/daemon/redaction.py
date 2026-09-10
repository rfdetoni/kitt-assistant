"""Compatibility surface for resident transport event sanitization.

The canonical public-event redaction policy belongs to the Agent control plane
because telemetry and every transport must apply the same policy.  The
Assistant runtime re-exports it under the historical daemon path for existing
resident-runtime consumers.
"""

from kitt.security.public_events import sanitize_public_event_payload

__all__ = ["sanitize_public_event_payload"]
