"""Dataflow manifest queryable for the Python SDK.

Mirrors :mod:`bubbaloop_node::manifest` in the Rust SDK. Every node served
by ``run_node`` exposes a CBOR-encoded manifest at
``bubbaloop/global/{machine_id}/{instance_name}/manifest`` so the
``dataflow`` MCP tool (and ``bubbaloop dataflow`` CLI) can reconstruct the
runtime DAG without parsing config YAML.

Each input / output entry carries per-topic liveness information:
``{topic, ever_fired, still_live, declared_at_ns}``. Consumers decide
whether "declared but never fired" counts as an edge — by default it
does not.
"""

from __future__ import annotations

import logging
import threading
from typing import TYPE_CHECKING

import cbor2
import zenoh

if TYPE_CHECKING:
    from .context import NodeContext

log = logging.getLogger(__name__)

MANIFEST_SCHEMA_VERSION = 2
"""Wire-format version. Bump on breaking schema changes.

v1: inputs/outputs are plain string lists.
v2: inputs/outputs are ``{topic, ever_fired, still_live, declared_at_ns}``
    objects.
"""

VALID_ROLES = {"source", "processor", "sink", "unknown"}


def normalize_role(role: str | None) -> str:
    """Map a free-form ``role`` config string to one of the four canonical values."""
    if not role:
        return "unknown"
    lowered = role.strip().lower()
    return lowered if lowered in VALID_ROLES else "unknown"


def manifest_topic(machine_id: str, instance_name: str) -> str:
    """Queryable key for a node's dataflow manifest."""
    return f"bubbaloop/global/{machine_id}/{instance_name}/manifest"


def build_manifest(
    ctx: "NodeContext",
    role: str,
    started_at_ns: int,
    node_kind: str = "python",
) -> dict:
    """Build a manifest snapshot suitable for CBOR encoding."""
    return {
        "instance_name": ctx.instance_name or "",
        "machine_id": ctx.machine_id,
        "role": normalize_role(role),
        "inputs": ctx.inputs_snapshot(),
        "outputs": ctx.outputs_snapshot(),
        "schema_version": MANIFEST_SCHEMA_VERSION,
        "started_at_ns": int(started_at_ns),
        "node_kind": node_kind,
    }


def start_manifest_queryable(
    ctx: "NodeContext",
    role: str,
    started_at_ns: int,
    node_kind: str = "python",
):
    """Declare the manifest queryable on ``ctx.session``.

    Returns the underlying :class:`zenoh.Queryable` so callers may keep a
    reference and call ``.undeclare()`` on shutdown. Replies are rebuilt
    on every query so publishers / subscribers declared after startup are
    still reflected.

    NEVER use ``complete=True`` on this queryable — wildcard discovery
    (``bubbaloop/**/manifest``) requires non-complete replies.
    """
    if not ctx.instance_name:
        log.warning("manifest queryable skipped: ctx.instance_name is unset")
        return None

    key = manifest_topic(ctx.machine_id, ctx.instance_name)

    # Capture by closure; the snapshot is rebuilt each query so late
    # publisher/subscriber declarations are visible.
    _lock = threading.Lock()

    def _on_query(query: zenoh.Query):
        try:
            with _lock:
                manifest = build_manifest(ctx, role, started_at_ns, node_kind)
            payload = cbor2.dumps(manifest)
            # query.key_expr is a PROPERTY (not a method) — see CLAUDE.md.
            query.reply(query.key_expr, payload)
        except Exception:  # pragma: no cover — defensive
            log.exception("manifest reply failed for %s", key)

    queryable = ctx.session.declare_queryable(key, _on_query)
    log.info("Dataflow manifest queryable declared on %s", key)
    return queryable
