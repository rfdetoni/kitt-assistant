"""KITT Cortex: a bounded, opt-in event and context plane.

This package never executes tools. All side effects remain owned by Agent policy.
"""
from .core import CortexStore, preflight_action
from .router import advise_route

__all__ = ["CortexStore", "preflight_action", "advise_route"]
