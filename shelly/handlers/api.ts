/**
 * Shelly Handler API
 * 
 * Handlers process commands before execution and summarize output after.
 * Handlers are stateful - create a new instance for each command execution.
 */

export interface HandlerFactory {
  /**
   * Check if this handler should process the given command.
   * 
   * @param command - The full command string (e.g., "cargo build --release")
   * @returns true if this handler should process the command
   */
  matches(command: string): boolean;

  /**
   * Create a new handler instance for a command execution.
   * 
   * @param command - The original command
   * @param settings - User-provided settings for this handler
   * @returns A new handler instance
   */
  create(command: string, settings: Record<string, any>): Handler;

  /**
   * Describe the settings this handler accepts.
   * Used for documentation and validation.
   */
  settings(): SettingsSchema;
}

export interface Handler {
  /**
   * Prepare the command for execution.
   * Can modify the command and set environment variables.
   * 
   * Note: This is skipped when exact: true is set.
   * 
   * @returns Modified command and environment variables
   */
  prepare(): PrepareResult;

  /**
   * Process incremental output chunks.
   * Called repeatedly as output arrives, and once more when complete.
   * 
   * @param stdoutChunk - New stdout data (may be empty)
   * @param stderrChunk - New stderr data (may be empty)
   * @param exitCode - Exit code if complete, null if still running
   * @returns Summary to emit, or null to keep buffering
   */
  summarize(stdoutChunk: string, stderrChunk: string, exitCode: number | null): SummaryResult;
}

export interface PrepareResult {
  /** The command to execute (may be modified from original) */
  command: string;
  /** Environment variables to set */
  env: Record<string, string>;
}

export interface SummaryResult {
  /** 
   * Summary text to emit to the agent.
   * Return null to keep buffering (waiting for more output).
   */
  summary: string | null;
  
  /**
   * Optional truncation metadata to help agents understand what was filtered.
   */
  truncation?: TruncationInfo;
}

export interface TruncationInfo {
  /** Whether content was truncated/filtered */
  truncated: boolean;
  /** Reason for truncation */
  reason?: "filtered_noise" | "content_too_large" | "filtered_duplicates";
  /** Human-readable description of what was removed */
  description?: string;
}

export interface SettingsSchema {
  [key: string]: SettingDefinition;
}

export interface SettingDefinition {
  type: "boolean" | "string" | "number";
  default: any;
  description: string;
}
