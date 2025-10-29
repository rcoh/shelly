import type { HandlerFactory, Handler, PrepareResult, SummaryResult, SettingsSchema } from "./api.ts";

class CargoHandler implements Handler {
  private command: string;
  private settings: Record<string, any>;
  private stdout = "";
  private stderr = "";

  constructor(command: string, settings: Record<string, any>) {
    this.command = command;
    this.settings = settings;
  }

  prepare(): PrepareResult {
    const quiet = this.settings.quiet ?? true;
    const rustLog = this.settings.RUST_LOG ?? null;

    let modifiedCommand = this.command;
    
    // Add --quiet flag if enabled and not already present
    if (quiet && !this.command.includes("--quiet")) {
      modifiedCommand = this.command.replace(/^cargo\s+/, "cargo --quiet ");
    }

    const env: Record<string, string> = {};
    if (rustLog) {
      env.RUST_LOG = rustLog;
    }

    return { command: modifiedCommand, env };
  }

  summarize(stdoutChunk: string, stderrChunk: string, exitCode: number | null): SummaryResult {
    // Accumulate chunks
    this.stdout += stdoutChunk;
    this.stderr += stderrChunk;

    // Not complete yet, keep buffering
    if (exitCode === null) {
      return { summary: null };
    }

    const showWarnings = this.settings.show_warnings ?? false;
    
    // Filter stderr: keep errors, optionally keep warnings
    const lines = this.stderr.split("\n");
    const filtered: string[] = [];
    let inWarning = false;
    let hasErrors = this.stderr.includes("error");
    
    for (const line of lines) {
      // Track if we're in a warning block
      if (line.includes("warning:")) {
        inWarning = true;
        if (!showWarnings) continue;
      } else if (line.includes("error")) {
        inWarning = false;
      } else if (line.trim() === "") {
        inWarning = false;
      }
      
      // Skip warning details unless showWarnings is true
      if (inWarning && !showWarnings) {
        continue;
      }
      
      // Keep errors and aborting messages
      if (line.includes("error") || line.includes("aborting")) {
        filtered.push(line);
      }
      
      // Only keep compilation/finished lines if there are errors
      if (hasErrors && (line.includes("Compiling") || line.includes("Finished"))) {
        filtered.push(line);
      }
    }

    const summary = filtered.length > 0 
      ? filtered.join("\n")
      : exitCode === 0 
        ? "Build succeeded"
        : "Build failed";

    return { summary };
  }
}

export const cargoHandler: HandlerFactory = {
  matches(command: string): boolean {
    return command.trim().startsWith("cargo ");
  },

  create(command: string, settings: Record<string, any>): Handler {
    return new CargoHandler(command, settings);
  },

  settings(): SettingsSchema {
    return {
      quiet: {
        type: "boolean",
        default: true,
        description: "Add --quiet flag to reduce output noise",
      },
      show_warnings: {
        type: "boolean",
        default: false,
        description: "Include warnings in the summary (default: only errors)",
      },
      RUST_LOG: {
        type: "string",
        default: null,
        description: "Set RUST_LOG environment variable for logging",
      },
    };
  },
};
