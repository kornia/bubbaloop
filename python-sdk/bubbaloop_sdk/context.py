"""NodeContext — entry point for Bubbaloop Python nodes.

Synchronous API — no asyncio required. zenoh-python is blocking by design;
this SDK wraps it without adding async complexity.

Topic layout (asymmetric: publishers auto-scope, subscribers are absolute):

- ``ctx.topic(suffix)``  → ``bubbaloop/global/{machine_id}/{instance_name}/{suffix}``
- ``ctx.local_topic(suffix)`` → ``bubbaloop/local/{machine_id}/{instance_name}/{suffix}``

Publishers default to auto-scoping under the node's ``instance_name`` (from the
``name`` field of its YAML config). You own your outputs, so this is the
right default — two unrelated nodes that both publish ``embeddings`` land in
distinct keys without the author having to think about it.

Subscribers default to ABSOLUTE keys (relative to
``bubbaloop/{global|local}/{machine_id}/``). The 99% case is consuming an
upstream node's topic — you almost never read your own outputs back. Pass
the full ``other_instance_name/data`` suffix.

For genuinely cross-node / shared topics on the publisher side (daemon
control, well-known buses) use the ``*_absolute`` publisher variants which
also accept an absolute suffix and skip the ``instance_name`` prefix.

Usage::

    ctx = NodeContext.connect()
    pub = ctx.publisher_cbor("sensor/data")           # → .../{instance_name}/sensor/data
    sub = ctx.subscribe("upstream_node/frames")       # → .../{machine_id}/upstream_node/frames
    bus = ctx.publisher_json_absolute("bus/events")   # → .../{machine_id}/bus/events
    while not ctx.is_shutdown():
        pub.put({"temperature": 22.5})
"""

import logging
import os
import signal
import socket
import threading
import time

import zenoh

log = logging.getLogger(__name__)


def _hostname() -> str:
    return socket.gethostname().replace("-", "_")


class _Liveness:
    """Per-topic manifest bookkeeping entry.

    Tracks whether a publisher/subscriber has actually fired at least once
    and whether it is still declared. Never removed — kept for history so
    the dataflow tool can distinguish "never fired" from "no longer live".
    """

    __slots__ = ("declared_at_ns", "ever_fired", "still_live")

    def __init__(self, declared_at_ns: int):
        self.declared_at_ns = declared_at_ns
        self.ever_fired = False
        self.still_live = True

    def to_dict(self, topic: str) -> dict:
        return {
            "topic": topic,
            "ever_fired": self.ever_fired,
            "still_live": self.still_live,
            "declared_at_ns": self.declared_at_ns,
        }


class NodeContext:
    """Zenoh session + machine_id + instance_name + shutdown signal for a bubbaloop node.

    Create with :meth:`connect`. Cleanup with :meth:`close` (or use as a
    context manager).

    SHM transport is always enabled on the session. All publishers and
    subscribers benefit from zero-copy delivery automatically when both
    sides are on the same machine.

    Topic auto-scoping (publishers only): when ``instance_name`` is set
    (always the case under :func:`run_node`), all topics returned by
    :meth:`topic` / :meth:`local_topic` are prefixed with the node's
    instance name. Subscribers always take absolute keys — use
    :meth:`absolute_topic` / :meth:`absolute_local_topic` to construct
    them when needed.
    """

    def __init__(self, session: zenoh.Session, machine_id: str, instance_name: str | None):
        self.session = session
        self.machine_id = machine_id
        self.instance_name = instance_name
        self._shutdown = threading.Event()
        # Dataflow manifest tracking — every publisher/subscriber records the
        # absolute key suffix it was declared on, along with liveness bits
        # (declared_at_ns / ever_fired / still_live). The manifest queryable
        # serves these so the dataflow tool can tell live edges from ghosts.
        self._io_lock = threading.Lock()
        self._outputs: dict[str, _Liveness] = {}
        self._inputs: dict[str, _Liveness] = {}
        for sig in (signal.SIGINT, signal.SIGTERM):
            signal.signal(sig, lambda s, f: self._shutdown.set())

        if os.environ.get("BUBBALOOP_TOPIC_LEGACY") == "1":
            log.warning(
                "BUBBALOOP_TOPIC_LEGACY=1 is set but legacy mode was removed. "
                "Publishers are auto-scoped under instance_name "
                "(bubbaloop/{global|local}/{machine_id}/{instance_name}/{suffix}); "
                "subscribers take absolute keys "
                "(bubbaloop/{global|local}/{machine_id}/{absolute_suffix}). "
                "Use ctx.publisher_*_absolute() for cross-node publishing."
            )

    @classmethod
    def connect(
        cls,
        endpoint: str | None = None,
        instance_name: str | None = None,
    ) -> "NodeContext":
        """Connect to a Zenoh router and return a ready NodeContext.

        Endpoint resolution: ``endpoint`` arg → ``BUBBALOOP_ZENOH_ENDPOINT`` env
        → ``tcp/127.0.0.1:7447``.

        ``instance_name`` is used as the per-node topic namespace for
        publishers AND for health / schema topics. Pass the ``name`` field
        from your config so multi-instance deployments don't collide. If
        ``None`` (rare — only when called outside :func:`run_node`),
        publisher auto-scoping falls back to using just the ``machine_id``
        (legacy layout).
        """
        machine_id = os.environ.get("BUBBALOOP_MACHINE_ID", _hostname())
        ep = endpoint or os.environ.get("BUBBALOOP_ZENOH_ENDPOINT", "tcp/127.0.0.1:7447")

        conf = zenoh.Config()
        conf.insert_json5("mode", '"client"')
        conf.insert_json5("connect/endpoints", f'["{ep}"]')
        conf.insert_json5("scouting/multicast/enabled", "false")
        conf.insert_json5("scouting/gossip/enabled", "false")
        conf.insert_json5("transport/shared_memory/enabled", "true")
        session = zenoh.open(conf)

        return cls(session, machine_id, instance_name)

    # ------------------------------------------------------------------
    # Topic helpers
    # ------------------------------------------------------------------

    def _scope_prefix(self) -> str:
        """Return ``{machine_id}/{instance_name}`` or just ``{machine_id}``."""
        if self.instance_name:
            return f"{self.machine_id}/{self.instance_name}"
        return self.machine_id

    def topic(self, suffix: str) -> str:
        """Return ``bubbaloop/global/{machine_id}/{instance_name}/{suffix}``.

        When ``instance_name`` is unset, falls back to
        ``bubbaloop/global/{machine_id}/{suffix}`` (legacy layout).
        """
        return f"bubbaloop/global/{self._scope_prefix()}/{suffix}"

    def local_topic(self, suffix: str) -> str:
        """Return ``bubbaloop/local/{machine_id}/{instance_name}/{suffix}``.

        SHM-only — never crosses the WebSocket bridge. Use for large binary
        payloads consumed only by processes on the same machine (e.g. raw RGBA
        camera frames).
        """
        return f"bubbaloop/local/{self._scope_prefix()}/{suffix}"

    def absolute_topic(self, suffix: str) -> str:
        """Return ``bubbaloop/global/{machine_id}/{suffix}`` (no instance scoping).

        ``suffix`` is interpreted as an absolute key relative to
        ``bubbaloop/global/{machine_id}/``. Use when you need to print or
        construct a fully qualified key (e.g. for logging, lint scripts).
        """
        return f"bubbaloop/global/{self.machine_id}/{suffix}"

    def absolute_local_topic(self, suffix: str) -> str:
        """Return ``bubbaloop/local/{machine_id}/{suffix}`` (no instance scoping)."""
        return f"bubbaloop/local/{self.machine_id}/{suffix}"

    def _resolve_topic(self, suffix: str, local: bool) -> str:
        return self.local_topic(suffix) if local else self.topic(suffix)

    def _resolve_absolute_topic(self, suffix: str, local: bool) -> str:
        return self.absolute_local_topic(suffix) if local else self.absolute_topic(suffix)

    # ------------------------------------------------------------------
    # Dataflow manifest bookkeeping
    # ------------------------------------------------------------------

    def _strip_prefix(self, key: str) -> str | None:
        """Strip ``bubbaloop/{global|local}/{machine_id}/`` from ``key``."""
        for scope in ("global", "local"):
            prefix = f"bubbaloop/{scope}/{self.machine_id}/"
            if key.startswith(prefix):
                return key[len(prefix):]
        return None

    def _ensure_io_state(self) -> None:
        """Lazy-init manifest tracking state.

        Defensive against callers (notably unit tests) that build a
        ``NodeContext`` via ``object.__new__`` and skip ``__init__``.
        """
        if not hasattr(self, "_io_lock"):
            self._io_lock = threading.Lock()
        if not hasattr(self, "_outputs") or isinstance(self._outputs, list):
            self._outputs = {}
        if not hasattr(self, "_inputs") or isinstance(self._inputs, list):
            self._inputs = {}

    def _declare_output(self, key: str) -> str | None:
        """Register ``key`` as an output. Returns the stripped suffix, or
        ``None`` if the key does not match this node's machine prefix."""
        sfx = self._strip_prefix(key)
        if sfx is None:
            return None
        self._ensure_io_state()
        with self._io_lock:
            if sfx not in self._outputs:
                self._outputs[sfx] = _Liveness(time.time_ns())
            else:
                # Re-declaration: resurrect (some callers re-create publishers).
                self._outputs[sfx].still_live = True
        return sfx

    def _declare_input(self, key: str) -> str | None:
        sfx = self._strip_prefix(key)
        if sfx is None:
            return None
        self._ensure_io_state()
        with self._io_lock:
            if sfx not in self._inputs:
                self._inputs[sfx] = _Liveness(time.time_ns())
            else:
                self._inputs[sfx].still_live = True
        return sfx

    def _mark_output_fired(self, sfx: str | None) -> None:
        if not sfx:
            return
        self._ensure_io_state()
        with self._io_lock:
            entry = self._outputs.get(sfx)
            if entry is not None:
                entry.ever_fired = True

    def _mark_input_fired(self, sfx: str | None) -> None:
        if not sfx:
            return
        self._ensure_io_state()
        with self._io_lock:
            entry = self._inputs.get(sfx)
            if entry is not None:
                entry.ever_fired = True

    def _mark_output_undeclared(self, sfx: str | None) -> None:
        if not sfx:
            return
        self._ensure_io_state()
        with self._io_lock:
            entry = self._outputs.get(sfx)
            if entry is not None:
                entry.still_live = False

    def _mark_input_undeclared(self, sfx: str | None) -> None:
        if not sfx:
            return
        self._ensure_io_state()
        with self._io_lock:
            entry = self._inputs.get(sfx)
            if entry is not None:
                entry.still_live = False

    # Legacy (append-only list) API retained for tests and callers that only
    # care about the suffix set. Deprecated but still used by older tests.
    def _record_output(self, key: str) -> None:
        self._declare_output(key)

    def _record_input(self, key: str) -> None:
        self._declare_input(key)

    def outputs_snapshot(self) -> list[dict]:
        """List of ``{topic, ever_fired, still_live, declared_at_ns}`` entries."""
        self._ensure_io_state()
        with self._io_lock:
            return [v.to_dict(k) for k, v in self._outputs.items()]

    def inputs_snapshot(self) -> list[dict]:
        self._ensure_io_state()
        with self._io_lock:
            return [v.to_dict(k) for k, v in self._inputs.items()]

    # ------------------------------------------------------------------
    # Shutdown
    # ------------------------------------------------------------------

    def is_shutdown(self) -> bool:
        """True if SIGINT/SIGTERM has been received."""
        return self._shutdown.is_set()

    def wait_shutdown(self) -> None:
        """Block until SIGINT/SIGTERM is received."""
        self._shutdown.wait()

    # ------------------------------------------------------------------
    # Schema URI default
    # ------------------------------------------------------------------

    def _default_schema_uri(self, topic_suffix: str, schema_version: int) -> str:
        """Compose ``bubbaloop://{instance}/{topic_suffix}@v{schema_version}``.

        Falls back to ``machine_id`` when ``instance_name`` is unset — the
        rare case where ``NodeContext.connect()`` is used outside
        :func:`run_node`.
        """
        instance = self.instance_name or self.machine_id
        return f"bubbaloop://{instance}/{topic_suffix}@v{schema_version}"

    def _resolve_schema_uri(
        self, topic_suffix: str, schema_uri: str | None, schema_version: int
    ) -> str:
        """Use caller-provided URI if given; otherwise synthesize default.

        ``schema_uri=""`` (explicit empty string) is respected — lets callers
        opt out of the default for backward compat with consumers that rely
        on blank URIs.
        """
        if schema_uri is not None:
            return schema_uri
        return self._default_schema_uri(topic_suffix, schema_version)

    # ------------------------------------------------------------------
    # Publishers (auto-scoped under instance_name)
    # ------------------------------------------------------------------

    def _wire_manifest_hooks(self, publisher, sfx: str | None, is_input: bool) -> None:
        """Wire a publisher's first-fire + undeclare callbacks into manifest state.

        Silently ignores targets that don't accept attribute assignment
        (e.g. `MagicMock(return_value="pub")` in unit tests returning a bare
        string) — the manifest wiring is best-effort and must not mask
        test-time object shapes.
        """
        if sfx is None:
            return
        try:
            if is_input:
                publisher._on_first_fire = lambda s=sfx: self._mark_input_fired(s)
                publisher._on_undeclare = lambda s=sfx: self._mark_input_undeclared(s)
            else:
                publisher._on_first_fire = lambda s=sfx: self._mark_output_fired(s)
                publisher._on_undeclare = lambda s=sfx: self._mark_output_undeclared(s)
        except (AttributeError, TypeError):
            return

    def publisher_json(
        self,
        suffix: str,
        schema_uri: str | None = None,
        schema_version: int = 1,
    ) -> "JsonPublisher":
        """Declare a JSON publisher at ``topic(suffix)`` (auto-scoped).

        Payloads are wrapped in the SDK ``{header, body}`` provenance envelope
        (same shape as CBOR). ``schema_uri`` defaults to
        ``bubbaloop://{instance}/{instance}/{suffix}@v{schema_version}``
        when not provided; pass ``schema_uri=""`` to opt out.
        """
        from .publisher import JsonPublisher
        key = self.topic(suffix)
        sfx = self._declare_output(key)
        uri = self._resolve_schema_uri(sfx or suffix, schema_uri, schema_version)
        pub = JsonPublisher._declare(
            self.session,
            key,
            source_instance=self.instance_name or "",
            schema_uri=uri,
        )
        self._wire_manifest_hooks(pub, sfx, is_input=False)
        return pub

    def publisher_cbor(
        self,
        suffix: str,
        schema_uri: str | None = None,
        schema_version: int = 1,
        local: bool = False,
    ) -> "CborPublisher":
        """Declare a CBOR publisher at ``topic(suffix)`` (auto-scoped).

        Every payload is wrapped in a ``{header, body}`` provenance envelope.
        ``schema_uri`` defaults to
        ``bubbaloop://{instance}/{instance}/{suffix}@v{schema_version}``; pass
        ``schema_uri=""`` to opt out (empty string is respected).

        When ``local=True``, publishes to
        ``bubbaloop/local/{machine_id}/{instance_name}/{suffix}`` with
        ``CongestionControl.BLOCK`` — waits for the subscriber to release the
        SHM buffer instead of silently dropping frames. Use for large binary
        payloads (e.g. RGBD frames) consumed only by processes on the same
        machine. Mirrors ``publisher_raw(local=True)`` behaviour for CBOR
        publishers.
        """
        from .publisher import CborPublisher
        key = self._resolve_topic(suffix, local)
        sfx = self._declare_output(key)
        uri = self._resolve_schema_uri(sfx or suffix, schema_uri, schema_version)
        pub = CborPublisher._declare(
            self.session,
            key,
            source_instance=self.instance_name or "",
            schema_uri=uri,
            local=local,
        )
        self._wire_manifest_hooks(pub, sfx, is_input=False)
        return pub

    def publisher_raw(self, suffix: str, local: bool = False) -> "RawPublisher":
        """Declare a raw publisher with no encoding (auto-scoped under instance_name).

        When ``local=True``, publishes to ``local/{machine_id}/{instance_name}/{suffix}``
        with ``congestion_control=Block`` — waits for the subscriber to release the
        SHM buffer instead of dropping frames. Never crosses the bridge.
        """
        from .publisher import RawPublisher
        key = self._resolve_topic(suffix, local)
        sfx = self._declare_output(key)
        pub = RawPublisher._declare(self.session, key, local=local)
        self._wire_manifest_hooks(pub, sfx, is_input=False)
        return pub

    # ------------------------------------------------------------------
    # Absolute publishers (skip instance_name scoping)
    # ------------------------------------------------------------------

    def publisher_json_absolute(
        self,
        absolute_suffix: str,
        schema_uri: str | None = None,
        schema_version: int = 1,
    ) -> "JsonPublisher":
        """Declare a JSON publisher at the absolute key
        ``bubbaloop/global/{machine_id}/{absolute_suffix}`` (no instance scoping).

        Wraps payloads in the ``{header, body}`` envelope like
        :meth:`publisher_json`.
        """
        from .publisher import JsonPublisher
        key = self.absolute_topic(absolute_suffix)
        sfx = self._declare_output(key)
        uri = self._resolve_schema_uri(sfx or absolute_suffix, schema_uri, schema_version)
        pub = JsonPublisher._declare(
            self.session,
            key,
            source_instance=self.instance_name or "",
            schema_uri=uri,
        )
        self._wire_manifest_hooks(pub, sfx, is_input=False)
        return pub

    def publisher_cbor_absolute(
        self,
        absolute_suffix: str,
        schema_uri: str | None = None,
        schema_version: int = 1,
        local: bool = False,
    ) -> "CborPublisher":
        """Declare a CBOR publisher at the absolute key (no instance scoping).

        Wraps payloads in the same ``{header, body}`` envelope as
        :meth:`publisher_cbor`.

        When ``local=True``, publishes to
        ``bubbaloop/local/{machine_id}/{absolute_suffix}`` with
        ``CongestionControl.BLOCK`` — same SHM + backpressure semantics as
        :meth:`publisher_cbor` with ``local=True``, but without instance-name
        scoping. Use for shared well-known bus topics consumed on the same
        machine only.
        """
        from .publisher import CborPublisher
        key = self._resolve_absolute_topic(absolute_suffix, local)
        sfx = self._declare_output(key)
        uri = self._resolve_schema_uri(sfx or absolute_suffix, schema_uri, schema_version)
        pub = CborPublisher._declare(
            self.session,
            key,
            source_instance=self.instance_name or "",
            schema_uri=uri,
            local=local,
        )
        self._wire_manifest_hooks(pub, sfx, is_input=False)
        return pub

    def publisher_raw_absolute(self, absolute_suffix: str, local: bool = False) -> "RawPublisher":
        """Declare a raw publisher at the absolute key (no instance scoping)."""
        from .publisher import RawPublisher
        key = self._resolve_absolute_topic(absolute_suffix, local)
        sfx = self._declare_output(key)
        pub = RawPublisher._declare(self.session, key, local=local)
        self._wire_manifest_hooks(pub, sfx, is_input=False)
        return pub

    # ------------------------------------------------------------------
    # Subscribers (absolute by default — you almost always read upstream)
    # ------------------------------------------------------------------

    def subscribe(self, absolute_suffix: str, local: bool = False) -> "CborSubscriber":
        """Declare an auto-decoding subscriber at the ABSOLUTE key
        ``bubbaloop/{global|local}/{machine_id}/{absolute_suffix}``.

        - ``application/cbor`` → :class:`Envelope` (auto-unwrapped) or SimpleNamespace
        - ``application/json`` → :class:`Envelope` (auto-unwrapped) or ``dict``
        - anything else        → raw ``bytes``

        ``absolute_suffix`` is interpreted relative to
        ``bubbaloop/{global|local}/{machine_id}/`` — pass the full
        ``other_instance_name/topic`` of the upstream node you want to read.
        Almost all subscribers in a node want this — the data they consume
        comes from elsewhere.
        """
        from .subscriber import CborSubscriber
        key = self._resolve_absolute_topic(absolute_suffix, local)
        sfx = self._declare_input(key)
        sub = CborSubscriber(self.session, key)
        if sfx is not None:
            # Wrap the subscriber's recv to flip ever_fired on first delivery.
            # Best-effort — test stubs may not expose recv; skip silently.
            try:
                original_recv = sub.recv

                def _tracked_recv(s=sfx):
                    value = original_recv()
                    self._mark_input_fired(s)
                    return value

                sub.recv = _tracked_recv  # type: ignore[assignment]
            except (AttributeError, TypeError):
                pass
        return sub

    def subscribe_raw(self, absolute_suffix: str, local: bool = False) -> "RawSubscriber":
        """Declare a raw subscriber at the ABSOLUTE key
        ``bubbaloop/{global|local}/{machine_id}/{absolute_suffix}``.

        Use this to receive raw frames published by another node, e.g.
        ``ctx.subscribe_raw("tapo_terrace/raw", local=True)``.
        """
        from .subscriber import RawSubscriber
        key = self._resolve_absolute_topic(absolute_suffix, local)
        sfx = self._declare_input(key)
        sub = RawSubscriber(self.session, key)
        if sfx is not None:
            try:
                original_recv = sub.recv

                def _tracked_recv(s=sfx):
                    value = original_recv()
                    self._mark_input_fired(s)
                    return value

                sub.recv = _tracked_recv  # type: ignore[assignment]
            except (AttributeError, TypeError):
                pass
        return sub

    # ------------------------------------------------------------------
    # Cleanup
    # ------------------------------------------------------------------

    def close(self) -> None:
        """Close the Zenoh session."""
        self.session.close()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()
