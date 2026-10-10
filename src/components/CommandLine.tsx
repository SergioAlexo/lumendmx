import { useEffect, useRef, useState } from "react";
import * as engineApi from "../lib/engine";
import type { ConsoleStatus } from "../lib/engine";
import { cn } from "../lib/utils";
import { Button } from "./ui/button";
import { Input } from "./ui/input";

interface HistoryEntry {
  id: string;
  text: string;
  ok: boolean;
}

const STATUS_POLL_MS = 300;
const HISTORY_LIMIT = 20;

/** What a programmer key does: type some text into the command line, or run
 * a command straight away. `shortcut` is the key used with Alt. */
interface Key {
  label: string;
  /** Text appended to the command line (digits and "." are joined without
   * spaces, everything else is space-separated). */
  type?: string;
  /** A complete command to run immediately. */
  run?: string;
  shortcut?: string;
  title: string;
  /** Lit while the mode it toggles is on. */
  mode?: "blind" | "highlight";
}

const KEYPAD: Key[][] = [
  [
    { label: "7", type: "7", title: "7" },
    { label: "8", type: "8", title: "8" },
    { label: "9", type: "9", title: "9" },
    { label: "THRU", type: "THRU", shortcut: "T", title: "THRU: a range of fixtures" },
    { label: "FULL", type: "FULL", shortcut: "F", title: "FULL: 100% (after @)" },
  ],
  [
    { label: "4", type: "4", title: "4" },
    { label: "5", type: "5", title: "5" },
    { label: "6", type: "6", title: "6" },
    { label: "+", type: "+", title: "Add to the selection" },
    { label: "−", type: "-", title: "Remove from the selection" },
  ],
  [
    { label: "1", type: "1", title: "1" },
    { label: "2", type: "2", title: "2" },
    { label: "3", type: "3", title: "3" },
    { label: "@", type: "@", shortcut: "A", title: "@: set the intensity (or the cue number to copy/move to)" },
    { label: "ALL", run: "ALL", shortcut: "L", title: "ALL: select every fixture" },
  ],
  [
    { label: "0", type: "0", title: "0" },
    { label: ".", type: ".", title: "Decimal point" },
    { label: "TIME", type: "TIME", shortcut: "E", title: "TIME n: fade programmer changes in over n seconds" },
    { label: "GROUP", type: "GROUP", shortcut: "G", title: "GROUP n: select a group" },
    { label: "CLEAR", run: "CLEAR", shortcut: "C", title: "CLEAR: clears the selection, then the programmer values" },
  ],
  [
    { label: "REC", type: "RECORD CUE", shortcut: "R", title: "RECORD CUE n" },
    { label: "UPD", run: "UPDATE", shortcut: "U", title: "UPDATE: re-record the last recorded cue" },
    { label: "DEL", type: "DELETE CUE", shortcut: "D", title: "DELETE CUE n" },
    { label: "COPY", type: "COPY CUE", shortcut: "Y", title: "COPY CUE a @ b" },
    { label: "MOVE", type: "MOVE CUE", shortcut: "M", title: "MOVE CUE a @ b" },
  ],
  [
    { label: "HIGHLT", run: "HIGHLIGHT", shortcut: "H", title: "HIGHLIGHT: selection to full white, the rest dimmed", mode: "highlight" },
    { label: "BLIND", run: "BLIND", shortcut: "B", title: "BLIND: edit without changing the output", mode: "blind" },
    { label: "NEXT", run: "NEXT", shortcut: "N", title: "NEXT: GO the current cuelist's playback" },
    { label: "PREV", run: "PREV", shortcut: "P", title: "PREV: step the current cuelist's playback back" },
    { label: "REL", run: "RELEASE", shortcut: "Q", title: "RELEASE: clear the selection and programmer" },
  ],
];

const ALL_KEYS = KEYPAD.flat();

function isEditable(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName);
}

/** Joins `text` onto the command line: digits and "." continue the number
 * being typed, everything else is a separate word. */
function append(current: string, text: string): string {
  if (/^[0-9.]$/.test(text)) return current + text;
  return `${current}${current && !current.endsWith(" ") ? " " : ""}${text} `;
}

/** The programmer's command line and keypad (BUILD_PLAN Phase 3): `1 THRU 8
 * @ 50`, `GROUP 2 @ 0`, `RECORD CUE 1.5`, `COPY CUE 1 @ 5`, `TIME 3`,
 * `BLIND`, `HIGHLIGHT` ... Every keypad key has an Alt shortcut (hover for
 * it); outside a text field, digits, `+`, `-`, `.` and `/` type straight into
 * the command line. In the field, Escape clears it (a second Escape runs
 * CLEAR) and Up / Down recall earlier commands. There's no attribute encoder bar yet
 * (progressive UI work, see DECISIONS.md). */
export function CommandLine() {
  const [input, setInput] = useState("");
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [status, setStatus] = useState<ConsoleStatus | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const recall = useRef(-1);

  const refreshStatus = () => void engineApi.consoleStatus().then(setStatus).catch(() => undefined);
  useEffect(() => {
    refreshStatus();
    const id = setInterval(refreshStatus, STATUS_POLL_MS);
    return () => clearInterval(id);
  }, []);

  const execute = async (text: string) => {
    const trimmed = text.trim();
    if (!trimmed) return;
    try {
      const result = await engineApi.executeCommand(trimmed);
      setHistory((prev) =>
        [...prev, { id: crypto.randomUUID(), text: `${trimmed} → ${result.message}`, ok: result.ok }].slice(-HISTORY_LIMIT),
      );
      refreshStatus();
    } catch (err) {
      // Expected outside the Tauri runtime (plain browser preview).
      console.warn("Engine backend unavailable:", err);
    }
  };

  const enter = async () => {
    const text = input;
    setInput("");
    recall.current = -1;
    await execute(text);
  };

  const press = (key: Key) => {
    if (key.run) {
      void execute(key.run);
    } else if (key.type) {
      setInput((current) => append(current, key.type!));
      inputRef.current?.focus();
    }
  };

  // Global shortcuts. Kept in a ref so the listener always sees the latest
  // `press` and input without re-subscribing on every keystroke.
  const handler = useRef<(e: KeyboardEvent) => void>(() => undefined);
  handler.current = (e) => {
    if (e.altKey && !e.ctrlKey && !e.metaKey) {
      const key = ALL_KEYS.find((k) => k.shortcut && e.code === `Key${k.shortcut}`);
      if (key) {
        e.preventDefault();
        press(key);
      }
      return;
    }
    if (e.ctrlKey || e.metaKey || e.altKey || isEditable(e.target)) return;
    if (/^[0-9.+/-]$/.test(e.key)) {
      e.preventDefault();
      inputRef.current?.focus();
      setInput((current) => (e.key === "/" ? current : append(current, e.key)));
    }
  };
  useEffect(() => {
    const listener = (e: KeyboardEvent) => handler.current(e);
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, []);

  const recallHistory = (direction: 1 | -1) => {
    const commands = history.map((h) => h.text.split(" → ")[0]);
    if (commands.length === 0) return;
    const next = Math.min(commands.length - 1, Math.max(-1, recall.current + direction));
    recall.current = next;
    setInput(next === -1 ? "" : commands[commands.length - 1 - next]);
  };

  const modeOn = (mode: Key["mode"]) => (mode === "blind" ? status?.blind : mode === "highlight" ? status?.highlight : false);

  return (
    <div className="flex flex-col gap-2 border-t border-border p-3 font-mono text-xs">
      <div className="flex items-center justify-between gap-2 text-muted-foreground">
        <h2 className="font-sans text-xs font-semibold uppercase tracking-wide">Command Line</h2>
        <div className="flex items-center gap-1.5">
          {status?.blind && <span className="rounded bg-primary px-1.5 py-0.5 font-sans text-[10px] font-semibold text-primary-foreground">BLIND</span>}
          {status?.highlight && <span className="rounded bg-primary/20 px-1.5 py-0.5 font-sans text-[10px] font-semibold text-primary">HIGHLT</span>}
          {(status?.timeS ?? 0) > 0 && <span title="Programmer TIME">TIME {status?.timeS}s</span>}
          <span>{status?.selection.length ?? 0} selected</span>
        </div>
      </div>
      {history.length > 0 && (
        <div className="max-h-24 space-y-0.5 overflow-y-auto">
          {history.map((entry) => (
            <div key={entry.id} className={cn(entry.ok ? "text-success" : "text-destructive")}>
              {entry.text}
            </div>
          ))}
        </div>
      )}
      <div className="flex items-center gap-2">
        <span className="text-primary">›</span>
        <Input
          ref={inputRef}
          className="h-9 flex-1 font-mono"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void enter();
            else if (e.key === "Escape") {
              if (input) setInput("");
              else void execute("CLEAR");
              e.stopPropagation();
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              recallHistory(1);
            } else if (e.key === "ArrowDown") {
              e.preventDefault();
              recallHistory(-1);
            }
          }}
          placeholder="1 THRU 8 @ 50"
          aria-label="Command line"
        />
        <Button size="touch" onClick={() => void enter()} title="Run the command (Enter)">
          ENTER
        </Button>
      </div>
      <div className="grid grid-cols-5 gap-1 font-sans">
        {ALL_KEYS.map((key) => (
          <Button
            key={key.label}
            size="touch"
            variant={modeOn(key.mode) ? "default" : "secondary"}
            className="min-w-0 px-1"
            title={key.shortcut ? `${key.title} (Alt+${key.shortcut})` : key.title}
            onClick={() => press(key)}
          >
            {key.label}
          </Button>
        ))}
      </div>
    </div>
  );
}
