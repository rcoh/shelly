# Writing Shelly Handlers

Handlers are TypeScript modules that customize how Shelly executes and processes commands. They can modify commands before execution and filter/summarize output afterward.

## Quick Start

Create `.shelly/my-tool.ts`:

```typescript
import type { HandlerFactory, Handler, PrepareResult, SummaryResult } from "./api.ts";

export const myTool: HandlerFactory = {
  matches(command: string): boolean {
    return command.startsWith("my-tool ");
  },

  create(command: string, settings: Record<string, any>): Handler {
    return new MyToolHandler(command, settings);
  },

  settings() {
    return {
      type: "object",
      properties: {
        verbose: { type: "boolean", default: false }
      }
    };
  }
};

class MyToolHandler implements Handler {
  constructor(
    private command: string,
    private settings: Record<string, any>
  ) {}

  prepare(): PrepareResult {
    let cmd = this.command;
    if (!this.settings.verbose) {
      cmd += " --quiet";
    }
    return { command: cmd, env: {} };
  }

  summarize(stdout: string, stderr: string, exitCode: number | null): SummaryResult {
    if (exitCode === null) return { summary: null }; // Still running
    
    if (exitCode === 0) {
      return { summary: "Success" };
    } else {
      return { summary: stderr || "Command failed" };
    }
  }
}
```

## Handler Lifecycle

1. **Match**: `matches()` determines if handler applies to command
2. **Create**: `create()` instantiates handler with command and settings
3. **Prepare**: `prepare()` modifies command/environment (skipped if `exact: true`)
4. **Execute**: Shelly runs the prepared command
5. **Summarize**: `summarize()` processes output chunks as they arrive

## Core Concepts

### Command Matching

```typescript
matches(command: string): boolean {
  return command.startsWith("git ");
  // or: return /^docker (run|exec)/.test(command);
}
```

### Settings Schema

Define configurable options:

```typescript
settings() {
  return {
    type: "object",
    properties: {
      quiet: { type: "boolean", default: true },
      timeout: { type: "number", default: 30 },
      format: { type: "string", enum: ["json", "yaml"], default: "json" }
    }
  };
}
```

### Command Preparation

Modify commands before execution:

```typescript
prepare(): PrepareResult {
  let cmd = this.command;
  
  // Add flags
  if (this.settings.quiet) {
    cmd += " --quiet";
  }
  
  // Set environment
  const env: Record<string, string> = {};
  if (this.settings.debug) {
    env.DEBUG = "1";
  }
  
  return { command: cmd, env };
}
```

### Output Summarization

Process output incrementally:

```typescript
class MyHandler implements Handler {
  private buffer = "";
  
  summarize(stdout: string, stderr: string, exitCode: number | null): SummaryResult {
    this.buffer += stdout + stderr;
    
    // Still running - keep buffering
    if (exitCode === null) {
      return { summary: null };
    }
    
    // Command finished - process full output
    if (exitCode === 0) {
      const lines = this.buffer.split('\n').filter(l => l.includes('ERROR'));
      return { summary: lines.length ? lines.join('\n') : "Success" };
    } else {
      return { summary: `Failed: ${stderr}` };
    }
  }
}
```

## Common Patterns

### Filter Warnings

```typescript
summarize(stdout: string, stderr: string, exitCode: number | null): SummaryResult {
  if (exitCode === null) return { summary: null };
  
  const errors = stderr.split('\n')
    .filter(line => line.includes('error:') || line.includes('ERROR'));
  
  if (exitCode === 0) {
    return { summary: errors.length ? errors.join('\n') : "Build succeeded" };
  } else {
    return { summary: errors.join('\n') || "Build failed" };
  }
}
```

### Add Default Flags

```typescript
prepare(): PrepareResult {
  let cmd = this.command;
  
  // Add --json if not present
  if (!cmd.includes('--json') && !cmd.includes('--format')) {
    cmd += ' --json';
  }
  
  return { command: cmd, env: {} };
}
```

### Environment Variables

```typescript
prepare(): PrepareResult {
  const env: Record<string, string> = {};
  
  if (this.settings.rust_log) {
    env.RUST_LOG = this.settings.rust_log;
  }
  
  if (this.settings.no_color) {
    env.NO_COLOR = "1";
  }
  
  return { command: this.command, env };
}
```

## Testing Handlers

Create test files in `.shelly/tests/<handler>/`:

```toml
# .shelly/tests/my-tool/success.toml
command = "my-tool build"
settings = { verbose = false }

[input]
stdout = "Building project...\nBuild complete!"
stderr = ""
exit_code = 0

[expected]
summary = "Success"
```

Run tests:
```bash
cargo test
```

## Handler Discovery

Shelly searches for handlers in order:
1. `.shelly/<name>.ts` in current directory (walking up to `$HOME`)
2. Built-in handlers

User handlers override built-in ones with the same name.

## Built-in Handlers

- **cargo**: Rust build tool with warning filtering
- More coming soon...

## Tips

- Keep handlers simple and focused
- Use incremental summarization for large outputs
- Test with various command variations
- Consider edge cases (empty output, non-zero exits)
- Use settings for customization rather than hardcoding behavior

## API Reference

See `shelly/handlers/api.ts` for complete TypeScript interfaces.
