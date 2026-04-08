"""Schema queryable helper for protobuf nodes.

Nodes that publish protobuf data should call ``declare_schema_queryable()``
so the dashboard can fetch the ``FileDescriptorSet`` on demand and decode
messages using the correct schema.

This mirrors the Rust SDK's ``SchemaQueryable`` — serving
``msg_class.DESCRIPTOR.file.serialized_pb`` at the ``{node}/schema`` topic.

Example::

    schema_queryable = declare_schema_queryable(
        ctx.session, ctx.machine_id, "my-node", MyMessage
    )
    # keep schema_queryable alive for the duration of the node

Notes:
- Do NOT pass ``complete=True`` to ``declare_queryable`` — it blocks
  wildcard queries like ``bubbaloop/**/schema`` used by the dashboard.
- ``query.key_expr`` is a **property**, not a method.
"""

import zenoh


def declare_schema_queryable(
    session: zenoh.Session,
    machine_id: str,
    node_name: str,
    msg_class,
) -> zenoh.Queryable:
    """Declare a queryable that serves the node's ``FileDescriptorSet``.

    The queryable listens at::

        bubbaloop/global/{machine_id}/{node_name}/schema

    and replies with the serialized ``FileDescriptorSet`` bytes derived from
    ``msg_class.DESCRIPTOR.file.serialized_pb``.

    Returns the ``zenoh.Queryable`` — keep the reference alive for the
    lifetime of the node (garbage-collecting it will undeclare the queryable).
    """
    topic = f"bubbaloop/global/{machine_id}/{node_name}/schema"
    descriptor_bytes: bytes = msg_class.DESCRIPTOR.file.serialized_pb

    def _handler(query: zenoh.Query) -> None:
        # query.key_expr is a property, NOT a method call.
        query.reply(query.key_expr, descriptor_bytes)

    return session.declare_queryable(topic, _handler)
