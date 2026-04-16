# Pipe Agent Integrations Matrix

Canonical deterministic endpoint format:

`https://us-west-01-firestarter.pipenetwork.com/{account}/{hash}`

## Native adapters shipped

- OpenAI tools
  - TypeScript: `createOpenAIPipeTools`, `runOpenAIPipeTool`
  - Python: `openai_pipe_tools`, `run_openai_pipe_tool`
  - Rust: `create_openai_pipe_tools`, `run_openai_pipe_tool`
- Anthropic tools
  - TypeScript: `createAnthropicPipeTools`, `runAnthropicPipeTool`
  - Python: `anthropic_pipe_tools`, `run_anthropic_pipe_tool`
  - Rust: `create_anthropic_pipe_tools`, `run_anthropic_pipe_tool`
- LangChain-style tools
  - TypeScript: `PipeStorageLangChainTool`
  - Python: `PipeStorageLangChainTool`
  - Rust: `PipeStorageLangChainTool`
- LlamaIndex-style tools
  - TypeScript: `createLlamaIndexPipeTools`
  - Python: `llamaindex_pipe_tools`
  - Rust: `create_llamaindex_pipe_tools`
- Vercel AI SDK tools
  - TypeScript: `createVercelPipeTools`
- Cloudflare AI Workflows tools
  - TypeScript: `createCloudflarePipeTools`, `runCloudflarePipeTool`
- AutoGen tools
  - Python: `autogen_pipe_tool_schemas`, `autogen_pipe_function_map`
- CrewAI tools
  - Python: `CrewAIPipeTool`, `crewai_pipe_tools`

## Mapped adapters (use existing native tools)

- Replit agents: use OpenAI/Anthropic adapters.
- Cursor background agents: use OpenAI/Anthropic adapters.
- On-chain agent frameworks: use deterministic URL output from `pin()` / `deterministic_url()`.

## Core tool contract

- `pipe_store`
- `pipe_pin`
- `pipe_fetch`
- `pipe_delete`

This contract is intentionally identical across adapters to keep agent behavior portable.
