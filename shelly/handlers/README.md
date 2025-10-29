# Shelly Handlers

Handlers process commands before execution and summarize output after.

## Handler API

Handlers are stateful - a new instance is created for each command execution.

See `api.ts` for the full TypeScript interface.

### HandlerFactory Methods

**`matches(command: string): boolean`**
- Determines if this handler should process the command
- Example: `command.startsWith("cargo ")`

**`create(command: string, settings: Record<string, any>): Handler`**
- Creates a new handler instance for this command execution
- Handler stores command and settings for use in prepare() and summarize()

**`settings(): SettingsSchema`**
- Describes available settings for this handler
- Used for documentation and validation

### Handler Instance Methods

**`prepare(): PrepareResult`**
- Modifies the command and sets environment variables before execution
- Skipped when `exact: true` is set
- Returns: `{ command: string, env: Record<string, string> }`

**`summarize(stdoutChunk: string, stderrChunk: string, exitCode: number | null): SummaryResult`**
- Called incrementally as output arrives (chunks may be any size)
- Handler accumulates chunks internally
- `exitCode` is `null` while running, set when complete
- Return `{ summary: null }` to keep buffering
- Return `{ summary: string }` to emit output
- Summary should be deterministic regardless of chunk boundaries

## Example: Cargo Handler

The cargo handler (`cargo.ts`):
- Adds `--quiet` flag by default
- Filters out warnings (shows only errors)
- Supports `RUST_LOG` environment variable

Settings:
- `quiet: boolean` (default: true) - Add --quiet flag
- `show_warnings: boolean` (default: false) - Include warnings in summary
- `RUST_LOG: string` (default: null) - Set RUST_LOG env var

## Testing

Run the test script:
```bash
deno run --allow-read handlers/test-cargo.ts
```

## Handler Discovery

Shelly searches for handlers in this order:
1. `.shelly/*.ts` in current directory (up to `$HOME`)
2. Built-in handlers (bundled with shelly)

User handlers override built-in ones.

## Creating Custom Handlers

1. Implement the `Handler` interface from `api.ts`
2. Export your handler as a named export
3. Place in `.shelly/<name>.ts`

Or fork a built-in handler:
```bash
shelly fork cargo  # Creates ./.shelly/cargo.ts
```
