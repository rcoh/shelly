#!/usr/bin/env -S deno run --allow-read

import { cargoHandler } from "./cargo.ts";

console.log("Testing cargo handler...\n");

// Test 1: matches
console.log("Test: matches()");
console.log("  cargo build:", cargoHandler.matches("cargo build"));
console.log("  npm install:", cargoHandler.matches("npm install"));
console.log();

// Test 2: prepare with default settings
console.log("Test: prepare() with defaults");
const handler1 = cargoHandler.create("cargo build", {});
const prep1 = handler1.prepare();
console.log("  Command:", prep1.command);
console.log("  Env:", prep1.env);
console.log();

// Test 3: prepare with custom settings
console.log("Test: prepare() with RUST_LOG");
const handler2 = cargoHandler.create("cargo test", { RUST_LOG: "debug" });
const prep2 = handler2.prepare();
console.log("  Command:", prep2.command);
console.log("  Env:", prep2.env);
console.log();

// Test 4: incremental summarization
console.log("Test: incremental summarization");
const handler3 = cargoHandler.create("cargo build", {});
handler3.prepare();

// First chunk - compilation output
let result = handler3.summarize("", "   Compiling myproject v0.1.0\n", null);
console.log("  After chunk 1:", result.summary);

// Second chunk - warning
result = handler3.summarize("", `warning: unused variable: \`x\`
 --> src/main.rs:2:9
  |
2 |     let x = 5;
  |         ^ help: if this is intentional, prefix it with an underscore: \`_x\`
  |
  = note: \`#[warn(unused_variables)]\` on by default

`, null);
console.log("  After chunk 2:", result.summary);

// Third chunk - error and completion
result = handler3.summarize("", `error[E0425]: cannot find value \`y\` in this scope
 --> src/main.rs:3:13
  |
3 |     println!("{}", y);
  |                    ^ not found in this scope

error: aborting due to previous error; 1 warning emitted
`, 1);
console.log("  After completion:");
console.log(result.summary);
console.log();

// Test 5: settings schema
console.log("Test: settings()");
const schema = cargoHandler.settings();
console.log("  Available settings:", Object.keys(schema));
console.log("  quiet:", schema.quiet);
