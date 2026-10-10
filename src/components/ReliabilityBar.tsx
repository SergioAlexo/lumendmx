import { Lock, LockOpen, MessageSquare } from "lucide-react";
import { cn } from "../lib/utils";
import { hasProblem, useHealthStore } from "../store/useHealthStore";
import { countProblems, useMessagesStore } from "../store/useMessagesStore";
import { useShowModeStore } from "../store/useShowModeStore";
import { Button } from "./ui/button";

/** Top-bar controls for show safety: the show-mode lock, the watchdog light
 * and the Messages button with its unread problem count. */
export function ReliabilityBar() {
  const locked = useShowModeStore((s) => s.locked);
  const setLocked = useShowModeStore((s) => s.setLocked);
  const report = useHealthStore((s) => s.report);
  const entries = useMessagesStore((s) => s.entries);
  const seenUpTo = useMessagesStore((s) => s.seenUpTo);
  const setOpen = useMessagesStore((s) => s.setOpen);

  const unread = countProblems(entries, seenUpTo);
  const problem = hasProblem(report);

  return (
    <div className="flex items-center gap-2">
      <Button
        variant={locked ? "default" : "outline"}
        size="sm"
        title={
          locked
            ? "Show mode is on: editing and destructive commands are disabled. Click to turn off."
            : "Turn on show mode to lock out editing and destructive commands"
        }
        onClick={() => void setLocked(!locked)}
      >
        {locked ? <Lock /> : <LockOpen />}
        Show mode
      </Button>

      <span
        className={cn("h-2.5 w-2.5 rounded-full", problem ? "bg-destructive animate-pulse" : "bg-success")}
        title={
          problem
            ? "The watchdog sees a problem: open Messages for details"
            : report
              ? `Engine healthy (last tick ${(report.lastTickUs / 1000).toFixed(1)} ms)`
              : "Engine status unknown"
        }
      />

      <Button variant="ghost" size="sm" title="Messages" onClick={() => setOpen(true)}>
        <MessageSquare />
        {unread > 0 && (
          <span className="rounded-full bg-destructive px-1.5 text-xs text-destructive-foreground">{unread}</span>
        )}
      </Button>
    </div>
  );
}
