import { AlertTriangle, X } from "lucide-react";
import { hasProblem, useHealthStore } from "../store/useHealthStore";
import { useAppUpdateStore } from "../store/useAppUpdateStore";
import { useShowStore } from "../store/useShowStore";
import { Button } from "./ui/button";
import { MessagesDialog } from "./MessagesDialog";
import { useMessagesStore } from "../store/useMessagesStore";

function Banner({
  tone,
  children,
  onDismiss,
}: {
  tone: "warn" | "error";
  children: React.ReactNode;
  onDismiss?: () => void;
}) {
  return (
    <div
      className={
        tone === "error"
          ? "flex items-center gap-2 border-b border-destructive/40 bg-destructive/15 px-4 py-1.5 text-sm"
          : "flex items-center gap-2 border-b border-border bg-secondary px-4 py-1.5 text-sm"
      }
    >
      <AlertTriangle className="size-4 shrink-0" />
      <span className="flex-1">{children}</span>
      {onDismiss && (
        <Button variant="ghost" size="iconSm" title="Dismiss" onClick={onDismiss}>
          <X />
        </Button>
      )}
    </div>
  );
}

/** Notices that need the operator's eye: crash recovery, watchdog alarms, a
 * deferred or just-applied update. Also hosts the Messages window. */
export function StatusBanners() {
  const notice = useShowStore((s) => s.recoveryNotice);
  const dismissNotice = useShowStore((s) => s.dismissRecoveryNotice);
  const report = useHealthStore((s) => s.report);
  const { deferred, cancelDeferred, previousVersion, currentVersion, dismissVersionNotice } = useAppUpdateStore();
  const setMessagesOpen = useMessagesStore((s) => s.setOpen);

  return (
    <>
      {notice === "crash" && (
        <Banner tone="warn" onDismiss={dismissNotice}>
          LumenDMX did not shut down cleanly last time. Your latest autosave was restored; save the show once you have
          checked it.
        </Banner>
      )}
      {notice === "unsaved" && (
        <Banner tone="warn" onDismiss={dismissNotice}>
          Restored unsaved changes from the last session.
        </Banner>
      )}
      {hasProblem(report) && report && (
        <Banner tone="error">
          {report.engine === "stalled" ? "The engine tick has stalled. " : ""}
          {report.stalledOutputs.length > 0
            ? `Output has stopped on universe ${report.stalledOutputs.join(", ")}. `
            : ""}
          <button className="underline" onClick={() => setMessagesOpen(true)}>
            Open Messages
          </button>
        </Banner>
      )}
      {deferred && (
        <Banner tone="warn" onDismiss={cancelDeferred}>
          An update is waiting. It will install when show mode is turned off.
        </Banner>
      )}
      {previousVersion && (
        <Banner tone="warn" onDismiss={dismissVersionNotice}>
          Updated from v{previousVersion} to v{currentVersion}. A backup of your preferences and autosaves was taken
          before the update.
        </Banner>
      )}
      <MessagesDialog />
    </>
  );
}
