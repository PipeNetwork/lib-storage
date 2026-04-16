from __future__ import annotations

import base64
import json
from dataclasses import dataclass
from typing import Any

from .client import PipeStorage


def openai_pipe_tools(enable_delete: bool = True) -> list[dict[str, Any]]:
    tools: list[dict[str, Any]] = [
        {
            "type": "function",
            "function": {
                "name": "pipe_store",
                "description": "Store bytes/JSON in Pipe and return operation + deterministic URL",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "file_name": {"type": "string"},
                        "data": {"description": "JSON-serializable payload or UTF-8 text"},
                        "tier": {
                            "type": "string",
                            "enum": ["normal", "priority", "premium", "ultra", "enterprise"],
                        },
                    },
                    "required": ["file_name", "data"],
                },
            },
        },
        {
            "type": "function",
            "function": {
                "name": "pipe_pin",
                "description": "Resolve deterministic URL from operation_id, file_name, or content_hash",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "operation_id": {"type": "string"},
                        "file_name": {"type": "string"},
                        "content_hash": {"type": "string"},
                        "account": {"type": "string"},
                    },
                },
            },
        },
        {
            "type": "function",
            "function": {
                "name": "pipe_fetch",
                "description": "Fetch object bytes/text via deterministic URL/hash or file_name",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "key": {
                            "type": "string",
                            "description": "deterministic URL, 64-char hash, or file name",
                        },
                        "as_text": {"type": "boolean"},
                        "as_json": {"type": "boolean"},
                    },
                    "required": ["key"],
                },
            },
        },
    ]

    if enable_delete:
        tools.append(
            {
                "type": "function",
                "function": {
                    "name": "pipe_delete",
                    "description": "Delete object by file_name or operation_id",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "file_name": {"type": "string"},
                            "operation_id": {"type": "string"},
                        },
                    },
                },
            }
        )

    return tools


def anthropic_pipe_tools(enable_delete: bool = True) -> list[dict[str, Any]]:
    tools: list[dict[str, Any]] = [
        {
            "name": "pipe_store",
            "description": "Store bytes/JSON in Pipe and return operation + deterministic URL",
            "input_schema": {
                "type": "object",
                "properties": {
                    "file_name": {"type": "string"},
                    "data": {"description": "JSON-serializable payload or UTF-8 text"},
                    "tier": {
                        "type": "string",
                        "enum": ["normal", "priority", "premium", "ultra", "enterprise"],
                    },
                },
                "required": ["file_name", "data"],
            },
        },
        {
            "name": "pipe_pin",
            "description": "Resolve deterministic URL from operation_id, file_name, or content_hash",
            "input_schema": {
                "type": "object",
                "properties": {
                    "operation_id": {"type": "string"},
                    "file_name": {"type": "string"},
                    "content_hash": {"type": "string"},
                    "account": {"type": "string"},
                },
            },
        },
        {
            "name": "pipe_fetch",
            "description": "Fetch object bytes/text via deterministic URL/hash or file_name",
            "input_schema": {
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "deterministic URL, 64-char hash, or file name",
                    },
                    "as_text": {"type": "boolean"},
                    "as_json": {"type": "boolean"},
                },
                "required": ["key"],
            },
        },
    ]

    if enable_delete:
        tools.append(
            {
                "name": "pipe_delete",
                "description": "Delete object by file_name or operation_id",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "file_name": {"type": "string"},
                        "operation_id": {"type": "string"},
                    },
                },
            }
        )

    return tools


def run_openai_pipe_tool(client: PipeStorage, name: str, arguments: str | dict[str, Any]) -> Any:
    args = json.loads(arguments) if isinstance(arguments, str) else dict(arguments)

    if name == "pipe_store":
        file_name = str(args.get("file_name") or "agent/object.json")
        tier = args.get("tier")
        stored = client.store(args.get("data"), file_name=file_name, tier=str(tier) if tier else "normal")
        pinned = client.pin({"operation_id": stored.get("operation_id")})
        return {
            "operation_id": stored.get("operation_id"),
            "file_name": stored.get("file_name"),
            "content_hash": pinned.get("content_hash"),
            "deterministic_url": pinned.get("url"),
        }

    if name == "pipe_pin":
        return client.pin(
            {
                "operation_id": args.get("operation_id"),
                "file_name": args.get("file_name"),
                "content_hash": args.get("content_hash"),
                "account": args.get("account"),
            }
        )

    if name == "pipe_fetch":
        key = str(args.get("key") or "")
        fetched = client.fetch(
            key,
            as_text=bool(args.get("as_text", False)),
            as_json=bool(args.get("as_json", False)),
        )
        if isinstance(fetched, (bytes, bytearray)):
            return {
                "bytes_base64": base64.b64encode(bytes(fetched)).decode("ascii"),
                "bytes_len": len(fetched),
            }
        return fetched

    if name == "pipe_delete":
        return client.delete(
            {
                "file_name": args.get("file_name"),
                "operation_id": args.get("operation_id"),
            }
        )

    raise ValueError(f"Unknown Pipe tool: {name}")


def run_anthropic_pipe_tool(client: PipeStorage, name: str, input_payload: str | dict[str, Any]) -> Any:
    return run_openai_pipe_tool(client, name, input_payload)


def autogen_pipe_tool_schemas(enable_delete: bool = True) -> list[dict[str, Any]]:
    return openai_pipe_tools(enable_delete=enable_delete)


def autogen_pipe_function_map(client: PipeStorage) -> dict[str, Any]:
    return {
        "pipe_store": lambda **kwargs: run_openai_pipe_tool(client, "pipe_store", kwargs),
        "pipe_pin": lambda **kwargs: run_openai_pipe_tool(client, "pipe_pin", kwargs),
        "pipe_fetch": lambda **kwargs: run_openai_pipe_tool(client, "pipe_fetch", kwargs),
        "pipe_delete": lambda **kwargs: run_openai_pipe_tool(client, "pipe_delete", kwargs),
    }


@dataclass
class CrewAIPipeTool:
    name: str
    description: str
    client: PipeStorage
    tool_name: str

    def run(self, **kwargs: Any) -> Any:
        return run_openai_pipe_tool(self.client, self.tool_name, kwargs)


def crewai_pipe_tools(client: PipeStorage, enable_delete: bool = True) -> list[CrewAIPipeTool]:
    tools = [
        CrewAIPipeTool(
            name="pipe_store",
            description="Store bytes/JSON in Pipe and return operation + deterministic URL",
            client=client,
            tool_name="pipe_store",
        ),
        CrewAIPipeTool(
            name="pipe_pin",
            description="Resolve deterministic URL from operation_id, file_name, or content_hash",
            client=client,
            tool_name="pipe_pin",
        ),
        CrewAIPipeTool(
            name="pipe_fetch",
            description="Fetch object bytes/text via deterministic URL/hash or file_name",
            client=client,
            tool_name="pipe_fetch",
        ),
    ]

    if enable_delete:
        tools.append(
            CrewAIPipeTool(
                name="pipe_delete",
                description="Delete object by file_name or operation_id",
                client=client,
                tool_name="pipe_delete",
            )
        )

    return tools


@dataclass
class PipeStorageLangChainTool:
    client: PipeStorage
    name: str = "pipe_storage"
    description: str = "Pipe storage tool: action=store|pin|fetch|delete for deterministic agent storage."

    def invoke(self, payload: str | dict[str, Any]) -> str:
        args = json.loads(payload) if isinstance(payload, str) else dict(payload)
        action = str(args.get("action") or "").strip().lower()

        if action == "store":
            file_name = str(args.get("file_name") or "")
            if not file_name:
                raise ValueError("store action requires file_name")
            stored = self.client.store(args.get("data"), file_name=file_name, tier=str(args.get("tier") or "normal"))
            pinned = self.client.pin({"operation_id": stored.get("operation_id")})
            return json.dumps(
                {
                    "action": "store",
                    "operation_id": stored.get("operation_id"),
                    "file_name": stored.get("file_name"),
                    "content_hash": pinned.get("content_hash"),
                    "deterministic_url": pinned.get("url"),
                }
            )

        if action == "pin":
            pinned = self.client.pin(
                {
                    "operation_id": args.get("operation_id"),
                    "file_name": args.get("file_name"),
                    "content_hash": args.get("content_hash"),
                }
            )
            return json.dumps({"action": "pin", **pinned})

        if action == "fetch":
            key = args.get("key") or args.get("file_name") or args.get("content_hash")
            if not key:
                raise ValueError("fetch action requires key/file_name/content_hash")
            fetched = self.client.fetch(
                str(key),
                as_text=bool(args.get("as_text", False)),
                as_json=bool(args.get("as_json", False)),
            )
            if isinstance(fetched, (bytes, bytearray)):
                return json.dumps(
                    {
                        "action": "fetch",
                        "bytes_base64": base64.b64encode(bytes(fetched)).decode("ascii"),
                        "bytes_len": len(fetched),
                    }
                )
            return json.dumps({"action": "fetch", "value": fetched})

        if action == "delete":
            deleted = self.client.delete(
                {
                    "file_name": args.get("file_name"),
                    "operation_id": args.get("operation_id"),
                }
            )
            return json.dumps({"action": "delete", **deleted})

        raise ValueError(f"Unsupported action: {action}")


def llamaindex_pipe_tools(client: PipeStorage) -> list[dict[str, Any]]:
    def _store(args: dict[str, Any]) -> str:
        file_name = str(args.get("file_name") or "agent/object.json")
        stored = client.store(args.get("data"), file_name=file_name, tier=str(args.get("tier") or "normal"))
        pinned = client.pin({"operation_id": stored.get("operation_id")})
        return json.dumps(
            {
                "operation_id": stored.get("operation_id"),
                "file_name": stored.get("file_name"),
                "content_hash": pinned.get("content_hash"),
                "deterministic_url": pinned.get("url"),
            }
        )

    def _fetch(args: dict[str, Any]) -> str:
        key = str(args.get("key") or args.get("file_name") or args.get("content_hash") or "")
        fetched = client.fetch(
            key,
            as_text=bool(args.get("as_text", False)),
            as_json=bool(args.get("as_json", False)),
        )
        if isinstance(fetched, (bytes, bytearray)):
            return json.dumps(
                {
                    "bytes_base64": base64.b64encode(bytes(fetched)).decode("ascii"),
                    "bytes_len": len(fetched),
                }
            )
        return json.dumps(fetched)

    def _delete(args: dict[str, Any]) -> str:
        return json.dumps(
            client.delete(
                {
                    "file_name": args.get("file_name"),
                    "operation_id": args.get("operation_id"),
                }
            )
        )

    def _pin(args: dict[str, Any]) -> str:
        key: dict[str, Any] = {}
        if args.get("operation_id"):
            key["operation_id"] = args["operation_id"]
        if args.get("file_name"):
            key["file_name"] = args["file_name"]
        if args.get("content_hash"):
            key["content_hash"] = args["content_hash"]
        if args.get("account"):
            key["account"] = args["account"]
        if not key:
            raise ValueError("pipe_pin requires operation_id, file_name, or content_hash")
        return json.dumps(client.pin(key))

    return [
        {
            "metadata": {
                "name": "pipe_store",
                "description": "Store JSON/text in Pipe and return deterministic URL",
            },
            "call": _store,
        },
        {
            "metadata": {
                "name": "pipe_pin",
                "description": "Resolve deterministic URL from operation_id, file_name, or content_hash",
            },
            "call": _pin,
        },
        {
            "metadata": {
                "name": "pipe_fetch",
                "description": "Fetch bytes/text/json from Pipe by key/hash/url",
            },
            "call": _fetch,
        },
        {
            "metadata": {
                "name": "pipe_delete",
                "description": "Delete object in Pipe by file_name or operation_id",
            },
            "call": _delete,
        },
    ]
