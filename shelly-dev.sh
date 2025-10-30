#!/bin/bash
cd "$(dirname "$0")"
cargo build --quiet --bin shelly-mcp
./target/debug/shelly-mcp
