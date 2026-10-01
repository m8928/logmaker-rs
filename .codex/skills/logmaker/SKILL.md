---
name: logmaker
description: "Use when Codex needs to operate, inspect, or configure LogMaker through its MCP server: dashboard checks, plugin install/delete, maker/sender/log/scenario CRUD, import/export, log preview/start/stop, scenario start/stop, or building correlated log-generation workflows with shared variables, per-step senders, and overrides."
---

# LogMaker

## Quick Start

Use the LogMaker MCP tools first. If the MCP tools are not currently exposed, inspect `mcp-server/src/index.ts` for the available tool names and ensure the MCP server is registered or running with `LOGMAKER_URL` pointing at the LogMaker API.

For any creation workflow:

1. Read current state with `get_dashboard`, then list the relevant objects.
2. Discover schemas with `list_plugin_makers` and `list_plugin_senders` before choosing maker/sender arguments.
3. Create makers, then senders, then logs, then scenarios.
4. Use `preview_log` before creating or updating complex log formats.
5. Verify with `list_logs`, `list_scenarios`, and `get_dashboard`.

## Core Workflows

For a basic pipeline:

1. `list_plugin_makers`
2. `create_maker`
3. `list_plugin_senders`
4. `create_sender`
5. `preview_log`
6. `create_log`
7. `get_dashboard`

For import/export:

- Use `export_makers`, `export_senders`, `export_logs`, and `export_scenarios` to return JSON that matches the UI export payload.
- Use `import_makers`, `import_senders`, and `import_logs` when the JSON is already in the prompt or working context.
- Use `import_makers_file`, `import_senders_file`, `import_logs_file`, or `install_plugin` only with file paths local to the MCP server process.

For plugin management:

- Use `list_plugins` before install/delete.
- Use `install_plugin` with a local native plugin library path (`.so`, `.dylib` or `.dll`).
- Use `delete_plugin` only after checking references; plugin deletion can affect available maker/sender types.

## Scenario Rules

Create scenarios with `sharedVariables` as `variableName -> makerName`.

Use `steps[].senders` for delivery. Scenario-level sender lists are legacy and ignored by current execution.

Use `steps[].overrides` to replace fields in that step's log format:

```json
{
  "sharedVariables": { "src_ip": "ip_pool" },
  "steps": [
    {
      "logName": "login-log",
      "senders": ["debug-out"],
      "overrides": { "client_ip": "${src_ip}" }
    }
  ]
}
```

Override keys should normally be maker field names used in the target log format, such as `<client_ip>`. Override values may be literal text or a shared-variable token like `${src_ip}`.

## Safety Checks

Before destructive operations, list the target object and check reference counts or related objects. Prefer stop-before-delete for running logs and scenarios.

When tests or local commands are needed in this repository, run:

- `npm run build` in `mcp-server`
- `npm run check` and `npm run build` in `ui`
- `./mvnw test` at the repository root when backend behavior changes
