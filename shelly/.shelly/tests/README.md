# Handler Test Framework

The test framework validates that handlers correctly filter and summarize command output.

## Why Test Handlers?

Handlers transform raw command output into concise summaries for AI agents. Getting this right is critical:
- **Too verbose**: Wastes context window
- **Too filtered**: Loses important information (like line numbers in errors)
- **Inconsistent**: Makes debugging harder

The test framework lets you iterate on handlers with confidence.

## Test Structure

Tests live in `.shelly/tests/<handler-name>/` with paired input/output files:

```
.shelly/tests/
└── cargo/
    ├── input-build-error.json      # What the handler receives
    ├── output-build-error.json     # What it should produce
    ├── input-build-success.json
    └── output-build-success.json
```

## Test Format

### Input File (`input-<name>.json`)

Simulates what the handler receives after command execution:

```json
{
  "command": "cargo build",
  "settings": {},
  "stdout": "",
  "stderr": "   Compiling myproject v0.1.0\nerror[E0425]: cannot find value `y` in this scope\n --> src/main.rs:3:13\n  |\n3 |     println!(\"{}\", y);\n  |                    ^ not found in this scope\n\nerror: aborting due to previous error\n",
  "exit_code": 1
}
```

**Fields:**
- `command`: The original command string
- `settings`: Handler settings (e.g., `{"show_warnings": true}`)
- `stdout`: Standard output from the command
- `stderr`: Standard error from the command
- `exit_code`: Exit code (0 = success)

### Output File (`output-<name>.json`)

The expected summary after handler processing:

```json
{
  "summary": "   Compiling myproject v0.1.0\nerror[E0425]: cannot find value `y` in this scope\nerror: aborting due to previous error"
}
```

## Running Tests

Tests run automatically in the test suite:

```rust
#[tokio::test]
async fn test_handler_test_framework() {
    let tests = testing::find_tests("cargo").unwrap();
    let handler_path = std::path::PathBuf::from("handlers/cargo.ts");
    
    for test in &tests {
        let result = testing::run_test(&handler_path, test).await.unwrap();
        assert!(result.passed, "Test {} failed:\nExpected: {}\nActual: {}", 
                result.name, result.expected, result.actual);
    }
}
```

Run with: `cargo test`

## Creating New Tests

### 1. Capture Real Output

Run the command and save its output:

```bash
cargo build 2> error.txt
```

### 2. Create Input File

```json
{
  "command": "cargo build",
  "settings": {},
  "stdout": "",
  "stderr": "<paste error.txt contents>",
  "exit_code": 1
}
```

Save as `.shelly/tests/cargo/input-my-test.json`

### 3. Create Expected Output

Manually write what the handler *should* produce:

```json
{
  "summary": "error[E0425]: cannot find value `y` in this scope\n --> src/main.rs:3:13"
}
```

Save as `.shelly/tests/cargo/output-my-test.json`

### 4. Run Tests

```bash
cargo test
```

If the test fails, you'll see the diff between expected and actual output.

## Snapshot Testing (Future)

Eventually, you'll be able to generate expected outputs automatically:

```bash
shelly test cargo --update
```

This will run the handler and save its output as the expected result.

## Example: Improving the Cargo Handler

**Problem**: Cargo errors include line numbers, but the handler might filter them out.

**Solution**:
1. Create test with real cargo error (including line numbers)
2. Update expected output to include line numbers
3. Run test - it will fail
4. Fix handler to preserve line numbers
5. Run test - it passes!

See `cargo/input-build-error.json` for a complete example.

## Tips

- **Test edge cases**: Empty output, warnings only, multiple errors
- **Test settings**: Create tests with different handler settings
- **Keep it real**: Use actual command output, not made-up examples
- **Be specific**: Test one behavior per test case
- **Name clearly**: `input-build-with-warnings.json` is better than `input-test3.json`
