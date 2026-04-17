#!/usr/bin/env node
/**
 * NeuroVault MCP Server
 * ------------------------
 * Stdio MCP server that bridges your AI assistant <-> NeuroVault HTTP API.
 *
 * Architecture:
 *   AI assistant  --(MCP/stdio)-->  this script  --(HTTP)-->  NeuroVault
 *
 * Holds no state. Pure protocol translator.
 */

import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";

const BRAIN_URL = process.env.NEUROVAULT_HTTP_URL || "http://127.0.0.1:17777";

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

async function brainPost(path, body) {
  const url = `${BRAIN_URL}${path}`;
  const res = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(`Brain HTTP ${res.status} on ${path}: ${text || res.statusText}`);
  }
  return res.json();
}

async function brainGet(path) {
  const url = `${BRAIN_URL}${path}`;
  const res = await fetch(url);
  if (!res.ok) throw new Error(`Brain HTTP ${res.status} on ${path}`);
  return res.json();
}

async function checkHealth() {
  try {
    const data = await brainGet("/health");
    return data?.status === "ok";
  } catch {
    return false;
  }
}

// ---------------------------------------------------------------------------
// Tool definitions — matches what the brain HTTP API exposes.
// ---------------------------------------------------------------------------

const TOOLS = [
  {
    name: "brain_recall",
    description:
      "Semantic search across the entire NeuroVault knowledge graph. Use this to find prior context, related work, or knowledge on a topic.",
    inputSchema: {
      type: "object",
      properties: {
        query: {
          type: "string",
          description: "Natural-language query — what topic or question to search for.",
        },
        limit: {
          type: "integer",
          description: "Maximum number of matches to return. Default 10.",
          default: 10,
        },
      },
      required: ["query"],
    },
  },
  {
    name: "brain_context",
    description:
      "Returns everything the brain knows about a specific file or project path. Use this when you're about to edit a file and want to know prior decisions, related modules, or past discussions.",
    inputSchema: {
      type: "object",
      properties: {
        file_path: {
          type: "string",
          description: "Absolute or project-relative path to a file or directory.",
        },
      },
      required: ["file_path"],
    },
  },
  {
    name: "brain_preferences",
    description:
      "Returns the user's established behavioral and preference rules (the brain's user_cognition table). Use this BEFORE making subjective choices about code style, naming, structure, etc. Sorted by confidence x confirmations so the most reliable rules surface first.",
    inputSchema: {
      type: "object",
      properties: {
        pattern_type: {
          type: "string",
          description:
            "Optional filter: coding_style | debug_flow | planning | refactoring | naming | tooling | communication | decision_making | testing | error_handling. Omit for all.",
        },
      },
    },
  },
  {
    name: "brain_decisions",
    description:
      "Returns past architectural / technical decisions on a topic, with the recorded reasoning. Use this to avoid re-deciding things or contradicting prior conclusions.",
    inputSchema: {
      type: "object",
      properties: {
        topic: {
          type: "string",
          description: "Topic or area to look up decisions for.",
        },
      },
      required: ["topic"],
    },
  },
  {
    name: "brain_learn",
    description:
      "Force-record a new user_cognition rule. Use when the user explicitly says 'remember that I prefer X' or when you observe a clear, novel pattern that the auto-mining circuit might miss.",
    inputSchema: {
      type: "object",
      properties: {
        observation: {
          type: "string",
          description: "The rule itself, in natural language.",
        },
        pattern_type: {
          type: "string",
          description:
            "Category — coding_style | debug_flow | planning | refactoring | naming | tooling | communication | decision_making | testing | error_handling | general.",
        },
        trigger_node_id: {
          type: "string",
          description: "Optional id of a chat or decision node that triggered this rule.",
        },
      },
      required: ["observation"],
    },
  },
  {
    name: "brain_health",
    description: "Check whether the NeuroVault HTTP API is reachable. Returns status info.",
    inputSchema: { type: "object", properties: {} },
  },
  {
    name: "brain_stats",
    description:
      "Returns brain statistics: total nodes, total edges, domain breakdown, recent nodes. Use for situational awareness.",
    inputSchema: { type: "object", properties: {} },
  },
  // Phase 4.9 — additional brain tools
  {
    name: "brain_critique",
    description:
      "Critique a piece of code or a plan against the user's established user_cognition rules. Returns rules the text aligns with (good -- keep doing this) and rules it potentially conflicts with (warning -- needs review). Use BEFORE showing generated code or plans to sanity-check them against established patterns.",
    inputSchema: {
      type: "object",
      properties: {
        text: {
          type: "string",
          description: "The code, plan, or proposal text to critique.",
        },
      },
      required: ["text"],
    },
  },
  {
    name: "brain_history",
    description:
      "Returns a chronologically-ordered timeline of nodes for a topic — how the brain's understanding of this topic evolved over time. Use when you need to see the *progression* of work on a topic, not just current state.",
    inputSchema: {
      type: "object",
      properties: {
        topic: {
          type: "string",
          description: "Topic to get the history of.",
        },
      },
      required: ["topic"],
    },
  },
  {
    name: "brain_export_subgraph",
    description:
      "Export a focused subgraph: the requested nodes plus their immediate neighbours and connecting edges. Use when you need a self-contained context pack for a specific task — gives your AI assistant everything related to a small set of nodes in one call.",
    inputSchema: {
      type: "object",
      properties: {
        node_ids: {
          type: "array",
          items: { type: "string" },
          description: "Array of node ids (e.g., 'node:abc123') to expand into a subgraph.",
        },
      },
      required: ["node_ids"],
    },
  },
  {
    name: "brain_plan",
    description:
      "Generate a step-by-step plan for a task, guided by established preferences and past decisions. The brain pulls relevant past nodes via semantic search + top user_cognition rules, then has the FAST LLM produce a concrete numbered plan. Returns the plan plus the source nodes/preferences used so you can cite them.",
    inputSchema: {
      type: "object",
      properties: {
        task: {
          type: "string",
          description: "The task description to plan for.",
        },
      },
      required: ["task"],
    },
  },
  // Phase 2 — Dual-Brain Learning Tools
  {
    name: "brain_warnings",
    description:
      "Query past mistakes, bugs, gotchas, and contradictions the brain has recorded. Use BEFORE making changes in an area where past issues occurred — catches pitfalls before they bite.",
    inputSchema: {
      type: "object",
      properties: {
        query: {
          type: "string",
          description: "Topic to search for warnings about.",
        },
      },
      required: ["query"],
    },
  },
  {
    name: "brain_rules",
    description:
      "Query compiled deterministic rules (if/then, always, never, prefer). These are high-confidence machine-parseable rules extracted from behavioral patterns and decisions. Use to check established patterns before making style or architecture choices.",
    inputSchema: {
      type: "object",
      properties: {
        context: {
          type: "string",
          description: "Optional context to filter matching rules. Omit to get all active rules.",
        },
      },
    },
  },
  {
    name: "brain_learn_decision",
    description:
      "Record a decision with full reasoning so the brain remembers WHY a choice was made. Use when you and the user make a significant architectural, technical, or design decision.",
    inputSchema: {
      type: "object",
      properties: {
        topic: {
          type: "string",
          description: "What the decision is about (e.g. 'UBS payment gateway').",
        },
        choice: {
          type: "string",
          description: "What was chosen (e.g. 'Stripe').",
        },
        reasoning: {
          type: "string",
          description: "Why this choice was made.",
        },
        alternatives: {
          type: "array",
          items: { type: "string" },
          description: "Other options that were considered.",
        },
        confidence: {
          type: "number",
          description: "How confident in this decision (0.0-1.0). Default 0.8.",
        },
      },
      required: ["topic", "choice", "reasoning"],
    },
  },
  {
    name: "brain_learn_pattern",
    description:
      "Record a behavioral pattern or preference. Use when you observe how the user likes to work, or when the user explicitly says 'remember that I prefer X'. The brain will surface this pattern in future sessions.",
    inputSchema: {
      type: "object",
      properties: {
        observation: {
          type: "string",
          description: "The pattern itself (e.g. 'User prefers explicit error types over Box<dyn Error>').",
        },
        pattern_type: {
          type: "string",
          description: "Category: coding_style | debug_flow | planning | refactoring | naming | tooling | communication | decision_making | testing | error_handling | general.",
        },
        confidence: {
          type: "number",
          description: "How confident (0.0-1.0). Default 0.7.",
        },
      },
      required: ["observation"],
    },
  },
  {
    name: "brain_learn_mistake",
    description:
      "Record a mistake, bug, or gotcha so the brain warns about it in future sessions. Use when something breaks unexpectedly, when a workaround is needed, or when a past approach caused problems.",
    inputSchema: {
      type: "object",
      properties: {
        title: {
          type: "string",
          description: "Short title for the warning (e.g. 'Stripe webhook retry causes duplicates').",
        },
        description: {
          type: "string",
          description: "Full description of what went wrong and how to avoid it.",
        },
        severity: {
          type: "string",
          description: "high | medium | low. Default medium.",
        },
      },
      required: ["title", "description"],
    },
  },
];

// ---------------------------------------------------------------------------
// MCP server setup
// ---------------------------------------------------------------------------

const server = new Server(
  {
    name: "neurovault-mcp",
    version: "0.1.0",
  },
  {
    capabilities: {
      tools: {},
    },
  }
);

server.setRequestHandler(ListToolsRequestSchema, async () => {
  return { tools: TOOLS };
});

server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const { name, arguments: args } = request.params;

  try {
    let result;
    switch (name) {
      case "brain_recall":
        result = await brainPost("/brain/recall", {
          query: args.query,
          limit: args.limit ?? 10,
        });
        break;

      case "brain_context":
        result = await brainPost("/brain/context", {
          file_path: args.file_path,
        });
        break;

      case "brain_preferences":
        result = await brainPost("/brain/preferences", {
          pattern_type: args.pattern_type ?? null,
        });
        break;

      case "brain_decisions":
        result = await brainPost("/brain/decisions", {
          topic: args.topic,
        });
        break;

      case "brain_learn":
        result = await brainPost("/brain/learn", {
          observation: args.observation,
          pattern_type: args.pattern_type ?? null,
          trigger_node_id: args.trigger_node_id ?? null,
        });
        break;

      case "brain_health":
        const ok = await checkHealth();
        result = { healthy: ok, url: BRAIN_URL };
        break;

      case "brain_stats":
        result = await brainGet("/stats");
        break;

      // Phase 4.9 — additional brain tools
      case "brain_critique":
        result = await brainPost("/brain/critique", {
          text: args.text,
        });
        break;

      case "brain_history":
        result = await brainPost("/brain/history", {
          topic: args.topic,
        });
        break;

      case "brain_export_subgraph":
        result = await brainPost("/brain/export_subgraph", {
          node_ids: args.node_ids,
        });
        break;

      case "brain_plan":
        result = await brainPost("/brain/plan", {
          task: args.task,
        });
        break;

      // Phase 2 — Dual-Brain Learning Tools
      case "brain_warnings":
        result = await brainPost("/brain/warnings", {
          query: args.query,
        });
        break;

      case "brain_rules":
        result = await brainPost("/brain/rules", {
          context: args.context ?? null,
        });
        break;

      case "brain_learn_decision":
        result = await brainPost("/brain/learn_decision", {
          topic: args.topic,
          choice: args.choice,
          reasoning: args.reasoning,
          alternatives: args.alternatives ?? [],
          confidence: args.confidence ?? 0.8,
        });
        break;

      case "brain_learn_pattern":
        result = await brainPost("/brain/learn_pattern", {
          observation: args.observation,
          pattern_type: args.pattern_type ?? "general",
          confidence: args.confidence ?? 0.7,
        });
        break;

      case "brain_learn_mistake":
        result = await brainPost("/brain/learn_mistake", {
          title: args.title,
          description: args.description,
          severity: args.severity ?? "medium",
        });
        break;

      default:
        throw new Error(`Unknown tool: ${name}`);
    }

    return {
      content: [
        {
          type: "text",
          text: JSON.stringify(result, null, 2),
        },
      ],
    };
  } catch (err) {
    return {
      isError: true,
      content: [
        {
          type: "text",
          text: `brain-mcp error in ${name}: ${err.message}`,
        },
      ],
    };
  }
});

// ---------------------------------------------------------------------------
// Boot
// ---------------------------------------------------------------------------

async function main() {
  // Probe health on startup so we fail fast if the brain isn't running.
  const healthy = await checkHealth();
  if (!healthy) {
    console.error(
      `[brain-mcp] WARNING: NeuroVault unreachable at ${BRAIN_URL}. ` +
        `Tools will fail until the brain is started.`
    );
  } else {
    console.error(`[brain-mcp] Connected to NeuroVault at ${BRAIN_URL}`);
  }

  const transport = new StdioServerTransport();
  await server.connect(transport);
}

main().catch((err) => {
  console.error("[brain-mcp] fatal:", err);
  process.exit(1);
});
