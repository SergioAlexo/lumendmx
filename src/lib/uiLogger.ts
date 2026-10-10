import { logWrite } from "./reliability";

let installed = false;

function describe(value: unknown): string {
  if (value instanceof Error) return value.stack ?? value.message;
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

/** Joins console arguments the way the console would, for the log. */
export function formatConsoleArgs(args: unknown[]): string {
  return args.map(describe).join(" ");
}

function forward(level: "error" | "warn", message: string) {
  // Never throw from the logger, and ignore "no Tauri runtime" in a browser preview.
  void logWrite(level, "ui", message).catch(() => undefined);
}

/** Sends uncaught errors and console.error/warn to the Rust log, so they
 * appear in the log files, the Messages window and diagnostics exports. */
export function installUiLogger(): void {
  if (installed || typeof window === "undefined") return;
  installed = true;

  window.addEventListener("error", (e) => {
    forward("error", `${e.message} (${e.filename}:${e.lineno})`);
  });
  window.addEventListener("unhandledrejection", (e) => {
    forward("error", `Unhandled rejection: ${describe(e.reason)}`);
  });

  for (const level of ["error", "warn"] as const) {
    const original = console[level].bind(console);
    console[level] = (...args: unknown[]) => {
      original(...args);
      forward(level, formatConsoleArgs(args));
    };
  }
}
