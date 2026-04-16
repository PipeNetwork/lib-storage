from __future__ import annotations

from pipe_storage import PipeStorage, crewai_pipe_tools


def main() -> None:
    client = PipeStorage()
    tools = crewai_pipe_tools(client, enable_delete=False)

    print("crewai_tool_names", [tool.name for tool in tools])

    store_tool = next(tool for tool in tools if tool.name == "pipe_store")
    stored = store_tool.run(
        file_name="agent/crewai-demo.json",
        data={"from": "crewai", "note": "demo"},
    )
    print("pipe_store", stored)


if __name__ == "__main__":
    main()
