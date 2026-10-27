import { useEffect, useState } from "react";
import { Check, Copy } from "lucide-react";
import * as mcp from "../lib/mcp";
import type { McpStatus } from "../lib/showfile/generated";
import { cn } from "../lib/utils";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Section } from "./ui/section";

function CopyLine({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="flex items-center gap-2 rounded-md bg-muted px-3 py-2 font-mono text-xs">
      <span className="flex-1 select-all break-all">{text}</span>
      <Button
        size="iconSm"
        variant="ghost"
        title="Copy"
        onClick={() => {
          void navigator.clipboard.writeText(text).then(() => {
            setCopied(true);
            setTimeout(() => setCopied(false), 1500);
          });
        }}
      >
        {copied ? <Check /> : <Copy />}
      </Button>
    </div>
  );
}

/** The built-in MCP server an AI agent connects to, to patch fixtures and
 * build the node graph. Local only (127.0.0.1). */
export function McpSettingsPanel() {
  const [status, setStatus] = useState<McpStatus | null>(null);
  const [port, setPort] = useState("");

  useEffect(() => {
    void mcp
      .mcpStatus()
      .then((s) => {
        setStatus(s);
        setPort(String(s.settings.port));
      })
      .catch(() => undefined);
  }, []);

  if (!status) return null;
  const apply = (enabled: boolean, nextPort = status.settings.port) =>
    void mcp.mcpConfigure({ enabled, port: nextPort }).then(setStatus).catch(() => undefined);

  return (
    <Section title="AI agents (MCP)">
      <div className="flex flex-wrap items-center gap-3 text-sm">
        <label className="flex items-center gap-2">
          <input type="checkbox" checked={status.settings.enabled} onChange={(e) => apply(e.target.checked)} />
          Let MCP clients control this show
        </label>
        <label className="flex items-center gap-2 text-muted-foreground">
          Port
          <Input
            className="h-8 w-24"
            type="number"
            min={1024}
            max={65535}
            value={port}
            onChange={(e) => setPort(e.target.value)}
            onBlur={() => {
              const p = Number(port);
              if (p >= 1024 && p <= 65535 && p !== status.settings.port) apply(status.settings.enabled, p);
              else setPort(String(status.settings.port));
            }}
          />
        </label>
        <span className={cn("text-xs", status.url ? "text-success" : status.error ? "text-destructive" : "text-muted-foreground")}>
          {status.url ? `Listening on ${status.url}` : (status.error ?? "Off")}
        </span>
      </div>
      {status.url && (
        <>
          <p className="text-xs text-muted-foreground">
            Add it to Claude Code (or any MCP client that speaks streamable HTTP), then ask for a look: the agent patches,
            groups and wires nodes, and you see it here live. Every change is undoable.
          </p>
          <CopyLine text={`claude mcp add --transport http lumendmx ${status.url}`} />
        </>
      )}
    </Section>
  );
}
