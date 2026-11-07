"""HTTP endpoints used by the image-phase controller frontend."""

from __future__ import annotations

import functools
import ipaddress
import logging
import os
from typing import Any

from ..services.image_phases import (
    discard_candidate,
    invalidate_phase,
    normalize_detailer,
    parse_phase,
    phase_status,
    promote_candidate,
)

LOGGER = logging.getLogger(__name__)


def is_local_host(host: str | None, allowed: str = "") -> bool:
    """True for a Host header a local ComfyUI can really receive: an IP address or localhost.

    A domain name means DNS rebinding: a web page whose name now points at this machine, for
    which ComfyUI's origin check passes because Origin and Host match. A machine reached by its
    own name on a LAN lists it in ``SAYA_ALLOWED_HOSTS`` (comma-separated).
    """
    text = (host or "").strip().lower()
    if text.startswith("["):
        name = text[1:].split("]", 1)[0]
    else:
        name = text.rsplit(":", 1)[0] if text.count(":") == 1 else text
    if not name:
        return False
    if name == "localhost" or name.endswith(".localhost"):
        return True
    if name in {item.strip().lower() for item in allowed.split(",") if item.strip()}:
        return True
    try:
        ipaddress.ip_address(name)
    except ValueError:
        return False
    return True


async def read_object(request: Any) -> dict[str, Any]:
    """The JSON body as an object. Anything else is a bad request, not a server error."""
    payload = await request.json()
    if not isinstance(payload, dict):
        raise ValueError("JSON object expected.")
    return payload


def register_image_phase_routes() -> None:
    """Register validation, redo, status, and emergency-unload endpoints."""
    from aiohttp import web
    from server import PromptServer

    routes = PromptServer.instance.routes

    def local_only(handler: Any) -> Any:
        @functools.wraps(handler)
        async def guarded(request: Any) -> Any:
            if not is_local_host(request.host, os.environ.get("SAYA_ALLOWED_HOSTS", "")):
                return web.json_response({"ok": False, "error": "Host not allowed."}, status=403)
            return await handler(request)

        return guarded

    @routes.post("/saya/image-phases/validate")
    @local_only
    async def validate_phase(request: Any) -> Any:
        try:
            payload = await read_object(request)
            phase = parse_phase(payload.get("phase"))
            detailer = normalize_detailer(payload.get("detailer", "none"))
            root = str(payload.get("checkpoint_root", "image/checkpoints"))
            transaction = str(payload.get("transaction_uuid") or "") or None
            manifest = promote_candidate(phase, detailer, root, transaction)
            return web.json_response({"ok": True, "manifest": manifest})
        except (ValueError, OSError) as error:
            LOGGER.exception("Image phase validation failed")
            return web.json_response({"ok": False, "error": str(error)}, status=400)

    @routes.post("/saya/image-phases/redo")
    @local_only
    async def redo_phase(request: Any) -> Any:
        try:
            payload = await read_object(request)
            phase = parse_phase(payload.get("phase"))
            root = str(payload.get("checkpoint_root", "image/checkpoints"))
            discarded = invalidate_phase(phase, root) if phase == 1 else discard_candidate(phase, root)
            return web.json_response({"ok": True, **discarded})
        except (ValueError, OSError) as error:
            return web.json_response({"ok": False, "error": str(error)}, status=400)

    @routes.post("/saya/image-phases/unload")
    @local_only
    async def unload_phase(request: Any) -> Any:
        del request
        return web.json_response({"ok": True, "unload": {"disabled": True, "reason": "automatic model unload removed"}})

    @routes.post("/saya/close-workflow")
    @local_only
    async def close_workflow(request: Any) -> Any:
        """Ask every open Comfy tab to drop the previous workflow. The PNG already holds it."""
        del request
        from server import PromptServer

        PromptServer.instance.send_sync("saya_close_workflow", {})
        return web.json_response({"ok": True})

    @routes.post("/saya/image-phases/status")
    @local_only
    async def status_phase(request: Any) -> Any:
        try:
            payload = await read_object(request)
            phase = parse_phase(payload.get("phase"))
            detailer = normalize_detailer(payload.get("detailer", "none"))
            root = str(payload.get("checkpoint_root", "image/checkpoints"))
            return web.json_response({"ok": True, **phase_status(phase, detailer, root)})
        except (ValueError, OSError) as error:
            return web.json_response({"ok": False, "error": str(error)}, status=400)
