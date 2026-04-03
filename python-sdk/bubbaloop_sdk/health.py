"""Background health heartbeat thread."""

import threading
import time

import zenoh


def start_health_heartbeat(
    session: zenoh.Session,
    scope: str,
    machine_id: str,
    instance_name: str,
    shutdown: threading.Event,
    interval_secs: float = 5.0,
) -> threading.Thread:
    """Publish 'ok' to ``{instance_name}/health`` every ``interval_secs``.

    Returns the daemon thread (already started). Stops when ``shutdown`` is set.
    """
    topic = f"bubbaloop/{scope}/{machine_id}/{instance_name}/health"
    pub = session.declare_publisher(topic)

    def _loop():
        while not shutdown.wait(timeout=interval_secs):
            pub.put(b"ok")

    t = threading.Thread(target=_loop, daemon=True, name=f"health-{instance_name}")
    t.start()
    return t
