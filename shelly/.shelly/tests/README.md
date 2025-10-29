# Handler Test Framework

Test cases for handlers are stored in `.shelly/tests/<handler-name>/` directories.

## Test Format

Each test consists of two JSON files:

**`input-<name>.json`** - The test input:
```json
{
  "command": "cargo build",
  "settings": {},
  "stdout": "",
  "stderr": "...",
  "exit_code": 1
}
```

**`output-<name>.json`** - The expected output:
```json
{
  "summary": "expected summary text"
}
```

## Running Tests

Tests are automatically discovered and run:

```rust
let tests = testing::find_tests("cargo")?;
for test in &tests {
    let result = testing::run_test(&handler_path, test).await?;
    assert!(result.passed);
}
```

## Creating Tests

1. Create input file: `.shelly/tests/cargo/input-my-test.json`
2. Run handler to generate output
3. Create expected output: `.shelly/tests/cargo/output-my-test.json`

Or use snapshot testing (future):
```bash
shelly test cargo --update
```

## Example Tests

See `cargo/` directory for examples:
- `input-build-error.json` - Build with compilation error
- `input-build-success.json` - Successful build
