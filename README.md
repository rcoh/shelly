# Shelly

A smart command execution wrapper that filters and summarizes CLI output for AI agents.

## What is Shelly?

Shelly intercepts command execution and uses TypeScript handlers to:
- **Filter verbose output** - Remove noise while preserving important information
- **Summarize results** - Provide concise, actionable summaries
- **Customize behavior** - Configure how different commands are processed

Perfect for AI agents that need to run CLI tools without being overwhelmed by verbose output.

## Key Features

- **Smart Filtering**: Built-in handlers for common tools (cargo, etc.)
- **Extensible**: Write custom handlers in TypeScript
- **MCP Integration**: Works as a Model Context Protocol server
- **Configurable**: Per-command settings and environment variables
- **Test Framework**: Validate handler behavior with TOML test files

## Quick Start

### As MCP Server

```bash
# Run the MCP server
./shelly-dev.sh

# Or build and run manually
cargo run --bin shelly-mcp
```

### As Library

```rust
use shelly::{execute_command, ExecuteRequest};

let result = execute_command(ExecuteRequest {
    command: "cargo build".to_string(),
    working_dir: "/path/to/project".into(),
    exact: false,  // Use handlers
    settings: HashMap::new(),
    env: HashMap::new(),
}).await?;

println!("{}", result.summary);  // "Build succeeded" instead of verbose output
```

## How It Works

1. **Command Interception**: Shelly receives a command to execute
2. **Handler Matching**: Finds appropriate handler (e.g., cargo handler for `cargo build`)
3. **Command Preparation**: Handler can modify command and set environment variables
4. **Execution**: Runs the actual command
5. **Output Processing**: Handler filters and summarizes the output
6. **Result**: Returns concise summary instead of raw output

## Built-in Handlers

### Cargo Handler
- Adds `--quiet` flag by default
- Filters out warnings (shows only errors)
- Returns "Build succeeded" for successful builds
- Supports `RUST_LOG` environment variable

**Settings:**
- `quiet: boolean` (default: true) - Add --quiet flag
- `show_warnings: boolean` (default: false) - Include warnings
- `RUST_LOG: string` - Set RUST_LOG environment variable

## Custom Handlers

Create handlers in `.shelly/<name>.ts` to customize command processing.

See [WRITING_HANDLERS.md](WRITING_HANDLERS.md) for a complete guide with examples.

## Testing Handlers

Create test files in `.shelly/tests/<handler>/`:

```toml
# .shelly/tests/cargo/build-error.toml
command = "cargo build"
settings = { quiet = true }

[input]
stdout = ""
stderr = """
error[E0425]: cannot find value `undefined_var` in this scope
 --> src/main.rs:2:13
  |
2 |     println!("{}", undefined_var);
  |             ^^^^^^^^^^^^^ not found in this scope
"""
exit_code = 101

[expected]
summary = """
error[E0425]: cannot find value `undefined_var` in this scope
 --> src/main.rs:2:13
  |
2 |     println!("{}", undefined_var);
  |             ^^^^^^^^^^^^^ not found in this scope
"""
```

Run tests:
```bash
cargo test
```

## Project Structure

```
shelly/
├── shelly/           # Core library
│   ├── src/          # Rust source code
│   └── handlers/     # Built-in TypeScript handlers
├── shelly-mcp/       # MCP server implementation
└── .shelly/          # User handlers and tests
    ├── tests/        # Handler test files
    └── *.ts          # Custom handlers
```

## Development

```bash
# Build the project
cargo build

# Run tests
cargo test

# Run MCP server in development
./shelly-dev.sh
```

**Important**: When developing with Q CLI, code changes to Shelly require restarting the entire Q CLI session. The MCP server is loaded once at startup and doesn't reload automatically.

## Use Cases

- **AI Code Assistants**: Filter compiler output to show only relevant errors
- **CI/CD Integration**: Summarize build results without log spam  
- **Development Tools**: Provide clean command output for better UX
- **Automation Scripts**: Process CLI tools with consistent, parseable output

## License

[Add your license here]
