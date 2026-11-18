"""Optional advisory for NodexLM System One; never a source of authorization."""
from __future__ import annotations

import json
import os
from urllib.parse import urlparse
from urllib.request import Request, build_opener, ProxyHandler, HTTPRedirectHandler


class _NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def advise_route(event: dict, *, model: str | None = None,
                 endpoint: str | None = None, timeout: float = 2.0) -> dict:
    """Classify with the existing NodexLM API; fall back without escalating.

    Endpoint must be numeric loopback HTTP to prevent event-controlled SSRF.
    Advisory complexity/urgency is not applied to Agent permissions or wallets.
    """
    sensitivity = event.get("sensitivity", "private")
    if sensitivity in {"secret", "private", "personal"}:
        allowed = "local_only"
    else:
        allowed = "existing_agent_router"
    base = endpoint or os.getenv("KITT_CORTEX_DECISION_URL", "")
    model = model or os.getenv("KITT_CORTEX_DECISION_MODEL", "")
    result = {"route": allowed, "advisory": None, "status": "disabled"}
    if not base or not model:
        return result
    parsed = urlparse(base)
    if (parsed.scheme != "http" or parsed.hostname not in {"127.0.0.1", "::1"}
            or parsed.username or parsed.password or parsed.query or parsed.fragment
            or parsed.path not in {"", "/"}):
        result["status"] = "invalid_endpoint"
        return result
    if not isinstance(timeout, (int, float)) or not 0.1 <= timeout <= 5:
        result["status"] = "invalid_timeout"
        return result
    state = json.dumps({"topic": event.get("topic", ""), "severity": event.get("severity", 1),
                        "source": event.get("source", "")}, separators=(",", ":"))[:1024]
    body = json.dumps({
        "model": model, "state": state,
        "questions": {
            "complexity": {"type": "choice", "instructions": "Complexidade da triagem?",
                           "criteria": {"low": "Regra simples", "high": "Raciocínio profundo"}},
            "urgent": {"type": "noul", "instructions": "Requer triagem prioritária?"},
        },
    }).encode()
    headers = {"Content-Type": "application/json"}
    key = os.getenv("NODEXLM_API_KEY", "")
    if key:
        headers["Authorization"] = f"Bearer {key}"
    try:
        req = Request(base.rstrip("/") + "/v1/systemone", data=body,
                      headers=headers, method="POST")
        opener = build_opener(ProxyHandler({}), _NoRedirect())
        with opener.open(req, timeout=timeout) as response:
            if response.status != 200:
                raise ValueError("unavailable")
            raw = response.read(8193)
        if len(raw) > 8192:
            raise ValueError("decision response too large")
        data = json.loads(raw)
        if not isinstance(data, dict) or not isinstance(data.get("answers"), dict):
            raise ValueError("invalid System One response")
        result["advisory"] = data["answers"]
        result["status"] = "ok"
    except (OSError, ValueError, KeyError, TypeError, json.JSONDecodeError):
        result["status"] = "unavailable"
    return result
