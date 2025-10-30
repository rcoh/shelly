#!/bin/bash
set -e  # Exit immediately if any command fails

# Save the current working directory
ORIGINAL_DIR="$(pwd)"

# Save the script's directory
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Change to the script's directory to build
cd "$SCRIPT_DIR"

# Build the project
rm -f ./target/debug/shelly-mcp
cargo build --quiet --bin shelly-mcp

# Return to the original directory and run the server
cd "$ORIGINAL_DIR"
"$SCRIPT_DIR"/target/debug/shelly-mcp
