# MCP Integration Guide

NeuroVault includes a standalone MCP (Model Context Protocol) server that connects your AI coding assistant to your brain's knowledge.

## Setup

### 1. Install the MCP server
```bash
cd mcp-server
npm install
```

### 2. Configure your AI assistant

Add to your AI assistant's MCP configuration:
```json
{
  "mcpServers": {
    "neurovault": {
      "command": "node",
      "args": ["/path/to/neurovault/mcp-server/src/index.js"],
      "env": {
        "NEUROVAULT_HTTP_URL": "http://127.0.0.1:17777"
      }
    }
  }
}
```

### 3. Available MCP Tools

| Tool | Description |
|------|-------------|
| `brain_recall` | Search brain knowledge by query |
| `brain_context` | Get context relevant to current file |
| `brain_learn` | Teach the brain something new |
| `brain_preferences` | Get user preferences and patterns |
| `brain_decisions` | Query past decisions |
| `brain_critique` | Get brain's critique of an approach |
| `brain_history` | Query brain activity history |
| `brain_plan` | Get strategic planning context |
| `brain_simulate` | Simulate a decision |
| `brain_dialogue` | Run internal debate on a topic |

### 4. Usage Examples

Ask your AI assistant:
- "Check my brain for any knowledge about React server components"
- "What decisions have I made about database architecture?"
- "Learn this: we chose SQLite over PostgreSQL for embedded use"
