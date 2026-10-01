#!/usr/bin/env node

import { readFile, realpath } from "node:fs/promises";
import { basename, extname, isAbsolute, relative, resolve } from "node:path";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------
const BASE_URL = process.env.LOGMAKER_URL ?? "http://localhost:19999";
const API = `${BASE_URL}/api/v1`;
const FILE_ROOT = resolve(process.env.LOGMAKER_MCP_FILE_ROOT ?? process.cwd());

type ApiResult<T = unknown> = { ok: boolean; status: number; data: T };

// ---------------------------------------------------------------------------
// HTTP helper
// ---------------------------------------------------------------------------
async function api<T = unknown>(
  path: string,
  opts: { method?: string; body?: unknown } = {}
): Promise<ApiResult<T>> {
  const { method = "GET", body } = opts;
  const headers: Record<string, string> = {};
  if (body !== undefined) headers["Content-Type"] = "application/json";

  try {
    const res = await fetch(`${API}${path}`, {
      method,
      headers,
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });

    return parseResponse<T>(res);
  } catch (err) {
    return connectionError<T>(err);
  }
}

async function apiMultipart<T = unknown>(
  path: string,
  filePath: string,
  allowedExtensions: string[]
): Promise<ApiResult<T>> {
  const safePath = await validateUploadFile(filePath, allowedExtensions);
  const bytes = await readFile(safePath);
  const file = new Blob([bytes], {
    type: contentTypeFor(safePath),
  });
  const form = new FormData();
  form.append("file", file, basename(safePath));

  try {
    const res = await fetch(`${API}${path}`, {
      method: "POST",
      body: form,
    });

    return parseResponse<T>(res);
  } catch (err) {
    return connectionError<T>(err);
  }
}

async function validateUploadFile(
  filePath: string,
  allowedExtensions: string[]
): Promise<string> {
  const realRoot = await realpath(FILE_ROOT);
  const realFile = await realpath(resolve(filePath));

  if (!isInside(realFile, realRoot)) {
    throw new Error(`File must be inside LOGMAKER_MCP_FILE_ROOT: ${realRoot}`);
  }

  const extension = extname(realFile).toLowerCase();
  if (!allowedExtensions.includes(extension)) {
    throw new Error(
      `Unsupported file extension '${extension}'. Allowed: ${allowedExtensions.join(", ")}`
    );
  }

  return realFile;
}

function isInside(candidate: string, root: string): boolean {
  const fromRoot = relative(root, candidate);
  return fromRoot === "" || (!fromRoot.startsWith("..") && !isAbsolute(fromRoot));
}

async function parseResponse<T>(
  res: Response
): Promise<ApiResult<T>> {
  const text = await res.text();
  let data: T;
  try {
    data = JSON.parse(text) as T;
  } catch {
    data = text as unknown as T;
  }
  return { ok: res.ok, status: res.status, data };
}

function connectionError<T>(err: unknown): ApiResult<T> {
  const detail = err instanceof Error && err.message ? ` ${err.message}` : "";
  return {
    ok: false,
    status: 503,
    data: {
      type: "ERROR",
      message: `Failed to connect to LogMaker API at ${BASE_URL}. Is the server running?${detail}`,
    } as T,
  };
}

function contentTypeFor(filePath: string): string {
  switch (extname(filePath).toLowerCase()) {
    case ".json":
      return "application/json";
    default:
      return "application/octet-stream";
  }
}

function formatResult(r: { ok: boolean; status: number; data: unknown }): {
  content: { type: "text"; text: string }[];
  isError?: boolean;
} {
  const text =
    typeof r.data === "string" ? r.data : JSON.stringify(r.data, null, 2);
  return {
    content: [{ type: "text" as const, text }],
    ...(r.ok ? {} : { isError: true }),
  };
}

function formatError(err: unknown): {
  content: { type: "text"; text: string }[];
  isError: true;
} {
  return {
    content: [
      {
        type: "text" as const,
        text: err instanceof Error ? err.message : String(err),
      },
    ],
    isError: true,
  };
}

async function uploadFile(path: string, filePath: string, allowedExtensions: string[]) {
  try {
    return formatResult(await apiMultipart(path, filePath, allowedExtensions));
  } catch (err) {
    return formatError(err);
  }
}

// ---------------------------------------------------------------------------
// MCP Server
// ---------------------------------------------------------------------------
const server = new McpServer({
  name: "logmaker",
  version: "1.0.0",
});

// ── Dashboard ───────────────────────────────────────────────────────────────
server.tool(
  "get_dashboard",
  "Get LogMaker dashboard metrics (maker/sender/log/plugin counts, EPS, CPU, memory, threads, scenarios)",
  {},
  async () => formatResult(await api("/dashboard"))
);

// ── Makers ──────────────────────────────────────────────────────────────────
server.tool(
  "list_makers",
  "List all registered makers",
  {},
  async () => formatResult(await api("/maker"))
);

server.tool(
  "export_makers",
  "Export all maker definitions as JSON (same data as the UI export)",
  {},
  async () => formatResult(await api("/maker"))
);

server.tool(
  "create_maker",
  "Create a new maker",
  {
    name: z.string().describe("Maker name"),
    type: z.string().describe("Maker type (e.g. IP, Date, Regex, Pick, UUID, NumberRange, IPRange)"),
    args: z
      .record(z.string(), z.unknown())
      .optional()
      .default({})
      .describe("Maker arguments (type-specific)"),
  },
  async ({ name, type, args }) =>
    formatResult(await api("/maker", { method: "POST", body: { name, type, args } }))
);

server.tool(
  "update_maker",
  "Update an existing maker",
  {
    name: z.string().describe("Maker name to update"),
    type: z.string().describe("Maker type"),
    args: z
      .record(z.string(), z.unknown())
      .optional()
      .default({})
      .describe("Updated arguments"),
  },
  async ({ name, type, args }) =>
    formatResult(
      await api(`/maker/${encodeURIComponent(name)}`, {
        method: "PUT",
        body: { name, type, args },
      })
    )
);

server.tool(
  "delete_maker",
  "Delete a maker by name",
  { name: z.string().describe("Maker name to delete") },
  async ({ name }) =>
    formatResult(await api(`/maker/${encodeURIComponent(name)}`, { method: "DELETE" }))
);

server.tool(
  "import_makers",
  "Import maker definitions from a JSON array",
  {
    makers: z
      .array(
        z
          .object({
            name: z.string().describe("Maker name"),
            type: z.string().describe("Maker type"),
            args: z.record(z.string(), z.unknown()).optional().default({}),
          })
          .passthrough()
      )
      .describe("Maker definitions to import"),
  },
  async ({ makers }) =>
    formatResult(await api("/maker:import", { method: "POST", body: makers }))
);

server.tool(
  "import_makers_file",
  "Import maker definitions from a local JSON file path under LOGMAKER_MCP_FILE_ROOT/current working directory",
  {
    filePath: z
      .string()
      .describe("Local .json file path under LOGMAKER_MCP_FILE_ROOT/current working directory"),
  },
  async ({ filePath }) => uploadFile("/maker:import-file", filePath, [".json"])
);

// ── Senders ─────────────────────────────────────────────────────────────────
server.tool(
  "list_senders",
  "List all registered senders",
  {},
  async () => formatResult(await api("/sender"))
);

server.tool(
  "export_senders",
  "Export all sender definitions as JSON (same data as the UI export)",
  {},
  async () => formatResult(await api("/sender"))
);

server.tool(
  "create_sender",
  "Create a new sender",
  {
    name: z.string().describe("Sender name"),
    type: z.string().describe("Sender type (e.g. Syslog, Kafka, Debug)"),
    args: z
      .record(z.string(), z.unknown())
      .optional()
      .default({})
      .describe("Sender arguments (type-specific, e.g. ip, port, facility, severity, messageFormat, hosts)"),
    limit: z
      .number()
      .optional()
      .default(0)
      .describe("Send limit (0 = unlimited)"),
  },
  async ({ name, type, args, limit }) =>
    formatResult(
      await api("/sender", { method: "POST", body: { name, type, args, limit } })
    )
);

server.tool(
  "update_sender",
  "Update an existing sender",
  {
    name: z.string().describe("Sender name to update"),
    type: z.string().describe("Sender type"),
    args: z
      .record(z.string(), z.unknown())
      .optional()
      .default({})
      .describe("Updated arguments"),
    limit: z
      .number()
      .optional()
      .default(0)
      .describe("Send limit (0 = unlimited)"),
  },
  async ({ name, type, args, limit }) =>
    formatResult(
      await api(`/sender/${encodeURIComponent(name)}`, {
        method: "PUT",
        body: { name, type, args, limit },
      })
    )
);

server.tool(
  "delete_sender",
  "Delete a sender by name",
  { name: z.string().describe("Sender name to delete") },
  async ({ name }) =>
    formatResult(await api(`/sender/${encodeURIComponent(name)}`, { method: "DELETE" }))
);

server.tool(
  "import_senders",
  "Import sender definitions from a JSON array",
  {
    senders: z
      .array(
        z
          .object({
            name: z.string().describe("Sender name"),
            type: z.string().describe("Sender type"),
            args: z.record(z.string(), z.unknown()).optional().default({}),
            limit: z.number().optional().default(0),
          })
          .passthrough()
      )
      .describe("Sender definitions to import"),
  },
  async ({ senders }) =>
    formatResult(await api("/sender:import", { method: "POST", body: senders }))
);

server.tool(
  "import_senders_file",
  "Import sender definitions from a local JSON file path under LOGMAKER_MCP_FILE_ROOT/current working directory",
  {
    filePath: z
      .string()
      .describe("Local .json file path under LOGMAKER_MCP_FILE_ROOT/current working directory"),
  },
  async ({ filePath }) => uploadFile("/sender:import-file", filePath, [".json"])
);

// ── Logs ────────────────────────────────────────────────────────────────────
server.tool(
  "list_logs",
  "List all log definitions",
  {},
  async () => formatResult(await api("/log"))
);

server.tool(
  "export_logs",
  "Export all log definitions as JSON (same data as the UI export)",
  {},
  async () => formatResult(await api("/log"))
);

server.tool(
  "create_log",
  "Create a new log definition and start generating",
  {
    name: z.string().describe("Log name"),
    format: z
      .string()
      .describe("Log format string (use <maker_name> to reference makers, e.g. <ip_maker>)"),
    eps: z
      .number()
      .describe("Rate target — number interpreted with epsUnit+epsTimeUnit. For bytes mode, this is the raw byte count (e.g. 1048576 for 1 MB)."),
    epsUnit: z
      .enum(["events", "bytes"])
      .optional()
      .default("events")
      .describe("Rate unit: 'events' (count) or 'bytes' (volume)"),
    epsTimeUnit: z
      .enum(["sec", "min", "hour", "day"])
      .optional()
      .default("sec")
      .describe("Time unit for the rate: sec/min/hour/day"),
    sender: z
      .array(z.string())
      .optional()
      .default([])
      .describe("List of sender names. Empty = no delivery (log paused)"),
    paused: z
      .boolean()
      .optional()
      .default(false)
      .describe("If true, log is created in paused state (no generation)"),
  },
  async ({ name, format, eps, epsUnit, epsTimeUnit, sender, paused }) =>
    formatResult(
      await api("/log", {
        method: "POST",
        body: { name, format, eps, epsUnit, epsTimeUnit, sender, paused },
      })
    )
);

server.tool(
  "update_log",
  "Update an existing log definition (paused state is preserved)",
  {
    name: z.string().describe("Log name to update"),
    format: z.string().describe("Updated log format string"),
    eps: z.number().describe("Updated rate target"),
    epsUnit: z
      .enum(["events", "bytes"])
      .optional()
      .default("events")
      .describe("Rate unit: events or bytes"),
    epsTimeUnit: z
      .enum(["sec", "min", "hour", "day"])
      .optional()
      .default("sec")
      .describe("Time unit: sec/min/hour/day"),
    sender: z
      .array(z.string())
      .describe("Updated list of sender names"),
  },
  async ({ name, format, eps, epsUnit, epsTimeUnit, sender }) =>
    formatResult(
      await api(`/log/${encodeURIComponent(name)}`, {
        method: "PUT",
        body: { name, format, eps, epsUnit, epsTimeUnit, sender },
      })
    )
);

server.tool(
  "delete_log",
  "Delete a log definition and stop generation",
  { name: z.string().describe("Log name to delete") },
  async ({ name }) =>
    formatResult(await api(`/log/${encodeURIComponent(name)}`, { method: "DELETE" }))
);

server.tool(
  "start_log",
  "Resume a paused log (no effect if already running)",
  { name: z.string().describe("Log name to start") },
  async ({ name }) =>
    formatResult(
      await api(`/log/${encodeURIComponent(name)}:start`, { method: "POST" })
    )
);

server.tool(
  "stop_log",
  "Pause a running log without removing it (senders/makers kept intact)",
  { name: z.string().describe("Log name to stop") },
  async ({ name }) =>
    formatResult(
      await api(`/log/${encodeURIComponent(name)}:stop`, { method: "POST" })
    )
);

server.tool(
  "preview_log",
  "Preview log output without creating it",
  {
    format: z
      .string()
      .describe("Log format string to preview"),
    name: z.string().optional().default("preview").describe("Temporary name"),
    eps: z.number().optional().default(1),
    sender: z.array(z.string()).optional().default([]),
  },
  async ({ format, name, eps, sender }) =>
    formatResult(
      await api("/log:preview", {
        method: "POST",
        body: { name, format, eps, sender },
      })
    )
);

server.tool(
  "import_logs",
  "Import log definitions from a JSON array",
  {
    logs: z
      .array(
        z
          .object({
            name: z.string().describe("Log name"),
            format: z.string().describe("Log format string"),
            eps: z.number().optional().default(0),
            epsUnit: z.enum(["events", "bytes"]).optional().default("events"),
            epsTimeUnit: z.enum(["sec", "min", "hour", "day"]).optional().default("sec"),
            sender: z.array(z.string()).optional().default([]),
            paused: z.boolean().optional().default(false),
          })
          .passthrough()
      )
      .describe("Log definitions to import"),
  },
  async ({ logs }) =>
    formatResult(await api("/log:import", { method: "POST", body: logs }))
);

server.tool(
  "import_logs_file",
  "Import log definitions from a local JSON file path under LOGMAKER_MCP_FILE_ROOT/current working directory",
  {
    filePath: z
      .string()
      .describe("Local .json file path under LOGMAKER_MCP_FILE_ROOT/current working directory"),
  },
  async ({ filePath }) => uploadFile("/log:import-file", filePath, [".json"])
);

// ── Plugins ─────────────────────────────────────────────────────────────────
server.tool(
  "list_plugins",
  "List all installed plugins",
  {},
  async () => formatResult(await api("/plugin"))
);

server.tool(
  "list_plugin_makers",
  "List all available maker types from plugins (shows type names and argument schemas)",
  {},
  async () => formatResult(await api("/plugin/maker"))
);

server.tool(
  "list_plugin_senders",
  "List all available sender types from plugins (shows type names and argument schemas)",
  {},
  async () => formatResult(await api("/plugin/sender"))
);

server.tool(
  "install_plugin",
  "Install a plugin from a local native plugin library (.so/.dylib/.dll) under LOGMAKER_MCP_FILE_ROOT/current working directory",
  {
    filePath: z
      .string()
      .describe("Local .so/.dylib/.dll plugin library path under LOGMAKER_MCP_FILE_ROOT/current working directory"),
  },
  async ({ filePath }) => uploadFile("/plugin", filePath, [".so", ".dylib", ".dll"])
);

server.tool(
  "delete_plugin",
  "Delete an installed plugin by name",
  { name: z.string().describe("Plugin name to delete") },
  async ({ name }) =>
    formatResult(await api(`/plugin/${encodeURIComponent(name)}`, { method: "DELETE" }))
);

// ── Scenarios ───────────────────────────────────────────────────────────────
server.tool(
  "list_scenarios",
  "List all scenarios",
  {},
  async () => formatResult(await api("/scenario"))
);

server.tool(
  "export_scenarios",
  "Export all scenarios as JSON (same data as the UI export)",
  {},
  async () => formatResult(await api("/scenario"))
);

const scenarioStepSchema = z.object({
  logName: z.string().describe("Log name to execute in this step"),
  repeat: z.number().optional().default(1).describe("Repeat count for this step"),
  delayMinMs: z.number().optional().default(0).describe("Minimum delay before step (ms)"),
  delayMaxMs: z.number().optional().default(0).describe("Maximum delay before step (ms)"),
  senders: z
    .array(z.string())
    .optional()
    .default([])
    .describe("Sender names for this step"),
  overrides: z
    .record(z.string(), z.string())
    .optional()
    .default({})
    .describe("Per-step field overrides. Keys are log maker field names; values can be literal text or a shared-variable token like ${src_ip}"),
});

server.tool(
  "create_scenario",
  "Create a new scenario for correlated multi-step log generation",
  {
    name: z.string().describe("Scenario name"),
    description: z.string().optional().describe("Scenario description"),
    sharedVariables: z
      .record(z.string(), z.string())
      .optional()
      .default({})
      .describe("Shared variables across all steps: variable name -> maker name (e.g. {\"src_ip\": \"ip_maker\"})"),
    steps: z.array(scenarioStepSchema).describe("Ordered list of scenario steps"),
    intervalMinMs: z
      .number()
      .optional()
      .default(1000)
      .describe("Minimum interval between loops (ms)"),
    intervalMaxMs: z
      .number()
      .optional()
      .default(5000)
      .describe("Maximum interval between loops (ms)"),
    loopCount: z
      .number()
      .optional()
      .default(0)
      .describe("Number of loops (0 = infinite)"),
  },
  async (args) =>
    formatResult(await api("/scenario", { method: "POST", body: args }))
);

server.tool(
  "update_scenario",
  "Update an existing scenario",
  {
    name: z.string().describe("Scenario name to update"),
    description: z.string().optional(),
    sharedVariables: z
      .record(z.string(), z.string())
      .optional()
      .default({})
      .describe("Variable name -> maker name"),
    steps: z.array(scenarioStepSchema),
    intervalMinMs: z.number().optional().default(1000),
    intervalMaxMs: z.number().optional().default(5000),
    loopCount: z.number().optional().default(0),
  },
  async ({ name, ...rest }) =>
    formatResult(
      await api(`/scenario/${encodeURIComponent(name)}`, {
        method: "PUT",
        body: { name, ...rest },
      })
    )
);

server.tool(
  "delete_scenario",
  "Delete a scenario",
  { name: z.string().describe("Scenario name to delete") },
  async ({ name }) =>
    formatResult(
      await api(`/scenario/${encodeURIComponent(name)}`, { method: "DELETE" })
    )
);

server.tool(
  "start_scenario",
  "Start executing a scenario",
  { name: z.string().describe("Scenario name to start") },
  async ({ name }) =>
    formatResult(
      await api(`/scenario/${encodeURIComponent(name)}:start`, { method: "POST" })
    )
);

server.tool(
  "stop_scenario",
  "Stop a running scenario",
  { name: z.string().describe("Scenario name to stop") },
  async ({ name }) =>
    formatResult(
      await api(`/scenario/${encodeURIComponent(name)}:stop`, { method: "POST" })
    )
);

// ---------------------------------------------------------------------------
// Resources — expose live data as readable resources
// ---------------------------------------------------------------------------
server.resource(
  "dashboard",
  "logmaker://dashboard",
  { description: "Live dashboard metrics", mimeType: "application/json" },
  async () => {
    const r = await api("/dashboard");
    return {
      contents: [
        {
          uri: "logmaker://dashboard",
          mimeType: "application/json",
          text: JSON.stringify(r.data, null, 2),
        },
      ],
    };
  }
);

server.resource(
  "logs",
  "logmaker://logs",
  { description: "All log definitions and their status", mimeType: "application/json" },
  async () => {
    const r = await api("/log");
    return {
      contents: [
        {
          uri: "logmaker://logs",
          mimeType: "application/json",
          text: JSON.stringify(r.data, null, 2),
        },
      ],
    };
  }
);

server.resource(
  "makers",
  "logmaker://makers",
  { description: "All registered makers", mimeType: "application/json" },
  async () => {
    const r = await api("/maker");
    return {
      contents: [
        {
          uri: "logmaker://makers",
          mimeType: "application/json",
          text: JSON.stringify(r.data, null, 2),
        },
      ],
    };
  }
);

server.resource(
  "senders",
  "logmaker://senders",
  { description: "All registered senders", mimeType: "application/json" },
  async () => {
    const r = await api("/sender");
    return {
      contents: [
        {
          uri: "logmaker://senders",
          mimeType: "application/json",
          text: JSON.stringify(r.data, null, 2),
        },
      ],
    };
  }
);

server.resource(
  "plugins",
  "logmaker://plugins",
  { description: "All installed plugins", mimeType: "application/json" },
  async () => {
    const r = await api("/plugin");
    return {
      contents: [
        {
          uri: "logmaker://plugins",
          mimeType: "application/json",
          text: JSON.stringify(r.data, null, 2),
        },
      ],
    };
  }
);

server.resource(
  "plugin_maker_types",
  "logmaker://plugin-maker-types",
  { description: "Available maker types and argument schemas", mimeType: "application/json" },
  async () => {
    const r = await api("/plugin/maker");
    return {
      contents: [
        {
          uri: "logmaker://plugin-maker-types",
          mimeType: "application/json",
          text: JSON.stringify(r.data, null, 2),
        },
      ],
    };
  }
);

server.resource(
  "plugin_sender_types",
  "logmaker://plugin-sender-types",
  { description: "Available sender types and argument schemas", mimeType: "application/json" },
  async () => {
    const r = await api("/plugin/sender");
    return {
      contents: [
        {
          uri: "logmaker://plugin-sender-types",
          mimeType: "application/json",
          text: JSON.stringify(r.data, null, 2),
        },
      ],
    };
  }
);

server.resource(
  "scenarios",
  "logmaker://scenarios",
  { description: "All scenarios and their execution status", mimeType: "application/json" },
  async () => {
    const r = await api("/scenario");
    return {
      contents: [
        {
          uri: "logmaker://scenarios",
          mimeType: "application/json",
          text: JSON.stringify(r.data, null, 2),
        },
      ],
    };
  }
);

// ---------------------------------------------------------------------------
// Prompts — common workflows
// ---------------------------------------------------------------------------
server.prompt(
  "setup_syslog_pipeline",
  "Guide: Set up a complete syslog pipeline (maker → sender → log)",
  {
    target_ip: z.string().default("127.0.0.1").describe("Syslog target IP"),
    target_port: z.string().default("514").describe("Syslog target port"),
    eps: z.string().default("10").describe("Events per second"),
  },
  ({ target_ip, target_port, eps }) => ({
    messages: [
      {
        role: "user" as const,
        content: {
          type: "text" as const,
          text: `Set up a complete syslog log generation pipeline on LogMaker:

1. First, check available maker and sender types with list_plugin_makers and list_plugin_senders
2. Create an IP maker for source IPs
3. Create a Syslog sender targeting ${target_ip}:${target_port}
4. Create a log definition with a realistic syslog format at ${eps} EPS, referencing the maker and sender
5. Verify everything is running with get_dashboard

Use the LogMaker MCP tools to execute each step.`,
        },
      },
    ],
  })
);

server.prompt(
  "setup_scenario",
  "Guide: Create a correlated multi-step scenario",
  {
    description: z.string().default("Login flow").describe("What the scenario simulates"),
  },
  ({ description }) => ({
    messages: [
      {
        role: "user" as const,
        content: {
          type: "text" as const,
          text: `Create a correlated log generation scenario on LogMaker for: "${description}"

1. Check existing logs with list_logs and available types with list_plugin_makers / list_plugin_senders
2. Create necessary makers for IP addresses, usernames, etc.
3. Create appropriate senders
4. Create log definitions for each step in the scenario
5. Create the scenario with sharedVariables as variable-name → maker-name, steps with their own senders, overrides that map log fields to \${variableName} tokens or literal values, and appropriate delays
6. Start the scenario

Use the LogMaker MCP tools to execute each step.`,
        },
      },
    ],
  })
);

// ---------------------------------------------------------------------------
// Start
// ---------------------------------------------------------------------------
async function main() {
  const transport = new StdioServerTransport();
  await server.connect(transport);
  console.error(`LogMaker MCP server running (API: ${API})`);
}

main().catch((err) => {
  console.error("Fatal:", err);
  process.exit(1);
});
