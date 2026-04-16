from __future__ import annotations

import time

from pipe_storage import PipeStorage, openai_pipe_tools


def main() -> None:
    pipe = PipeStorage()

    file_name = f"agent/session-{int(time.time() * 1000)}.json"

    stored = pipe.store(
        {
            "task": "agent-memory",
            "created_at": time.time(),
            "notes": ["first checkpoint", "second checkpoint"],
        },
        file_name=file_name,
    )

    pinned = pipe.pin({"operation_id": stored["operation_id"]})
    print("deterministic_url", pinned["url"])

    data = pipe.fetch(pinned["url"], as_json=True)
    print("fetched", data)

    tools = openai_pipe_tools()
    print("openai_tools", [t["function"]["name"] for t in tools])

    pipe.delete(file_name)


if __name__ == "__main__":
    main()
