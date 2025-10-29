# Handler Test Framework

The test framework validates that handlers correctly filter and summarize command output.

## Why Test Handlers?

Handlers transform raw command output into concise summaries for AI agents. Getting this right is critical:
- **Too verbose**: Wastes context window
- **Too filtered**: Loses important information (like line numbers in errors)
- **Inconsistent**: Makes debugging harder

The test framework lets you iterate on handlers with confidence.

## Test Structure

Tests live in `.shelly/tests/<handler-name>/` as individual TOML files:

```
.shelly/tests/
└── cargo/
    ├── build-error.toml
    ├── build-success.toml
    └── build-with-warnings.toml
```

Each file contains both the input and expected output.

## Test Format

Each test is a single TOML file with the command input and expected summary:

```toml
command = "cargo build"
exit_code = 1
stdout = ""
stderr = """
   Compiling myproject v0.1.0
error[E0425]: cannot find value `y` in this scope
 --> src/main.rs:3:13
  |
3 |     println!("{}", y);
  |                    ^ not found in this scope

error: aborting due to previous error
"""
expected_summary = """
   Compiling myproject v0.1.0
error[E0425]: cannot find value `y` in this scope
error: aborting due to previous error"""

[settings]
```

**Fields:**
- `command`: The original command string
- `exit_code`: Exit code (0 = success)
- `stdout`: Standard output (use `"""` for multiline)
- `stderr`: Standard error (use `"""` for multiline)
- `expected_summary`: What the handler should produce
- `settings`: Handler settings table (e.g., `settings.show_warnings = true`)

**Note**: TOML multiline strings preserve formatting. Don't add trailing newlines unless the handler output includes them.

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

### 2. Create Test File

```toml
command = "cargo build"
exit_code = 1
stdout = ""
stderr = """
<paste error.txt contents>
"""
expected_summary = """
<what the handler should produce>
"""

[settings]
```

Save as `.shelly/tests/cargo/my-test.toml`

### 3. Run Tests

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

See `cargo/build-error.toml` for a complete example.

## Tips

- **Test edge cases**: Empty output, warnings only, multiple errors
- **Test settings**: Create tests with different handler settings
- **Keep it real**: Use actual command output, not made-up examples
- **Be specific**: Test one behavior per test case
- **Name clearly**: `input-build-with-warnings.json` is better than `input-test3.json`
