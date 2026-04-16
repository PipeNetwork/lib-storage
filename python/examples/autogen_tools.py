from __future__ import annotations

from pipe_storage import PipeStorage, autogen_pipe_function_map, autogen_pipe_tool_schemas


def main() -> None:
    client = PipeStorage()
    schemas = autogen_pipe_tool_schemas(enable_delete=True)
    function_map = autogen_pipe_function_map(client)

    print("autogen_tool_names", [t["function"]["name"] for t in schemas])

    file_name = "agent/autogen-demo.json"
    stored = function_map["pipe_store"](
        file_name=file_name,
        data={"from": "autogen", "note": "demo"},
    )
    print("pipe_store", stored)


if __name__ == "__main__":
    main()
