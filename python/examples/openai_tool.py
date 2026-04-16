from pipe_storage import PipeStorage, openai_pipe_tools, run_openai_pipe_tool

pipe = PipeStorage()
tools = openai_pipe_tools()


def run_tool(name: str, arguments_json: str) -> dict:
    return run_openai_pipe_tool(pipe, name, arguments_json)
