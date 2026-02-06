"""HTTP API - Chat endpoint, WebSocket streaming, watcher/world state APIs."""

import asyncio
import json
import logging

from aiohttp import web, WSMsgType

logger = logging.getLogger(__name__)


class HttpApi:
    """HTTP API server for the bubbaloop agent."""

    def __init__(self, agent, watcher_engine, world_model, data_router, config: dict):
        self.agent = agent
        self.watcher_engine = watcher_engine
        self.world_model = world_model
        self.data_router = data_router

        http_config = config.get("http", {})
        self.host = http_config.get("host", "127.0.0.1")
        self.port = http_config.get("port", 8080)

        self.app = web.Application()
        self._setup_routes()

    def _setup_routes(self):
        """Register HTTP routes."""
        self.app.router.add_post("/api/chat", self._handle_chat)
        self.app.router.add_get("/api/chat/stream", self._handle_chat_ws)
        self.app.router.add_get("/api/watchers", self._handle_watchers)
        self.app.router.add_get("/api/world", self._handle_world)
        self.app.router.add_get("/api/captures", self._handle_captures)
        self.app.router.add_get("/api/health", self._handle_health)

    async def _handle_chat(self, request: web.Request) -> web.Response:
        """POST /api/chat - synchronous chat endpoint."""
        try:
            body = await request.json()
        except json.JSONDecodeError:
            return web.json_response({"error": "Invalid JSON"}, status=400)

        message = body.get("message", "")
        conversation_id = body.get("conversation_id")

        if not message:
            return web.json_response({"error": "Message required"}, status=400)

        # Collect all parts from the async iterator
        parts = []
        async for part in self.agent.handle_message(message, conversation_id):
            parts.append(part)

        # Last part is the final response
        response_text = parts[-1] if parts else "(No response)"
        intermediate = parts[:-1] if len(parts) > 1 else []

        return web.json_response({
            "response": response_text,
            "conversation_id": conversation_id,
            "intermediate": intermediate,
        })

    async def _handle_chat_ws(self, request: web.Request) -> web.WebSocketResponse:
        """GET /api/chat/stream - WebSocket streaming chat."""
        ws = web.WebSocketResponse()
        await ws.prepare(request)

        logger.info("WebSocket client connected")

        async for msg in ws:
            if msg.type == WSMsgType.TEXT:
                try:
                    body = json.loads(msg.data)
                except json.JSONDecodeError:
                    await ws.send_json({"error": "Invalid JSON"})
                    continue

                message = body.get("message", "")
                conversation_id = body.get("conversation_id")

                if not message:
                    await ws.send_json({"error": "Message required"})
                    continue

                # Stream responses
                async for part in self.agent.handle_message(message, conversation_id):
                    if part.startswith("[") and part.endswith("]"):
                        await ws.send_json({"type": "status", "content": part})
                    else:
                        await ws.send_json({"type": "response", "content": part})

            elif msg.type == WSMsgType.ERROR:
                logger.error(f"WebSocket error: {ws.exception()}")
                break

        logger.info("WebSocket client disconnected")
        return ws

    async def _handle_watchers(self, request: web.Request) -> web.Response:
        """GET /api/watchers - list active watchers."""
        watchers = []
        for w in self.watcher_engine.watchers.values():
            watchers.append({
                "name": w.name,
                "topics": w.topics,
                "instruction": w.instruction,
                "sample_interval_sec": w.sample_interval_sec,
                "paused": w.paused,
                "actions_this_hour": w.actions_this_hour,
                "max_actions_per_hour": w.max_actions_per_hour,
                "evaluations": len(w.history),
                "last_assessment": w.history[-1].assessment if w.history else None,
            })
        return web.json_response({"watchers": watchers})

    async def _handle_world(self, request: web.Request) -> web.Response:
        """GET /api/world - get world state."""
        await self.world_model.refresh()
        nodes = []
        for n in self.world_model.nodes.values():
            nodes.append({
                "name": n.name,
                "status": n.status,
                "health": n.health,
                "version": n.version,
                "description": n.description,
                "node_type": n.node_type,
            })
        return web.json_response({
            "daemon_healthy": self.world_model._daemon_healthy,
            "machine_id": self.world_model.zenoh.machine_id,
            "scope": self.world_model.zenoh.scope,
            "nodes": nodes,
        })

    async def _handle_captures(self, request: web.Request) -> web.Response:
        """GET /api/captures - list active captures."""
        captures = []
        for c in self.data_router.captures.values():
            if c.active:
                captures.append({
                    "id": c.id,
                    "topic": c.topic,
                    "output_path": c.output_path,
                    "format": c.format,
                    "samples_received": c.samples_received,
                    "files_written": c.files_written,
                    "bytes_written": c.bytes_written,
                })
        return web.json_response({"captures": captures})

    async def _handle_health(self, request: web.Request) -> web.Response:
        """GET /api/health - agent health check."""
        return web.json_response({"status": "ok", "agent": "bubbaloop-agent"})

    async def start(self):
        """Start the HTTP server."""
        runner = web.AppRunner(self.app)
        await runner.setup()
        site = web.TCPSite(runner, self.host, self.port)
        await site.start()
        logger.info(f"HTTP API listening on {self.host}:{self.port}")
        return runner
