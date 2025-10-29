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
    
    // If successful, return simple message
    if (exitCode === 0 && !this.stderr.includes("error")) {
      return { summary: "Build succeeded" };
    }
    
    // Filter stderr: keep error blocks, optionally keep warning blocks
    const lines = this.stderr.split("\n");
    const filtered: string[] = [];
    let inErrorBlock = false;
    let inWarningBlock = false;
    let filteredWarnings = 0;
    
    for (const line of lines) {
      // Detect start of error block
      if (line.includes("error[E") || line.includes("error:")) {
        inErrorBlock = true;
        inWarningBlock = false;
        filtered.push(line);
        continue;
      }
      
      // Detect start of warning block
      if (line.includes("warning:")) {
        inWarningBlock = true;
        inErrorBlock = false;
        if (showWarnings) {
          filtered.push(line);
        } else {
          filteredWarnings++;
        }
        continue;
      }
      
      // End of block detection (empty line or new section)
      if (line.trim() === "" || line.startsWith("For more information")) {
        inErrorBlock = false;
        inWarningBlock = false;
        // Keep "For more information" lines only if they're about errors
        if (line.startsWith("For more information") && !showWarnings) {
          continue;
        }
      }
      
      // Keep all lines in error blocks (includes -->, |, ^, help, etc.)
      if (inErrorBlock) {
        filtered.push(line);
        continue;
      }
      
      // Keep warning block lines if showWarnings is true
      if (inWarningBlock && showWarnings) {
        filtered.push(line);
        continue;
      }
      
      // Keep final error summary lines
      if (line.includes("could not compile") || line.includes("aborting due to")) {
        filtered.push(line);
      }
    }

    const summary = filtered.length > 0 
      ? filtered.join("\n")
      : "Build failed";

    // Add truncation info if we filtered warnings
    const truncation = filteredWarnings > 0 ? {
      truncated: true,
      reason: "filtered_noise" as const,
      description: `Filtered ${filteredWarnings} warning(s) - use show_warnings: true to include them`
    } : undefined;

    return { summary, truncation };
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
