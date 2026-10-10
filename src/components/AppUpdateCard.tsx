import { Download, History, RefreshCw } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { RELEASES_REPO } from "../lib/appUpdate";
import { releasePageUrl } from "../lib/updatePolicy";
import { useShowModeStore } from "../store/useShowModeStore";
import { useAppUpdateStore } from "../store/useAppUpdateStore";
import { Card, CardContent, CardHeader, CardTitle } from "./ui/card";
import { Badge } from "./ui/badge";
import { Button } from "./ui/button";
import { Progress, formatBytes } from "./ui/progress";

/** App self-update (SRS 2.5), driven entirely by @tauri-apps/plugin-updater's
 * `check()` result -- no separate GitHub-polling command for the version badge.
 * The initial check runs once at app startup (see App.tsx) so the SystemPanel's
 * update dot can light up before this card is ever expanded/mounted. */
export function AppUpdateCard() {
  const {
    currentVersion,
    update,
    checking,
    downloading,
    installing,
    downloaded,
    deferred,
    progress,
    error,
    checked,
    rollbackCandidate,
    previousVersion,
    checkNow,
    checkForRollback,
    download,
    install,
    cancelDeferred,
  } = useAppUpdateStore();
  const showMode = useShowModeStore((s) => s.locked);

  const statusBadge = !checked ? (
    <Badge variant="secondary">{checking ? "Checking…" : "Not checked"}</Badge>
  ) : update ? (
    <Badge>Update available</Badge>
  ) : (
    <Badge variant="success">Up to date</Badge>
  );

  return (
    <Card>
      <CardHeader className="flex-row items-center justify-between space-y-0 p-4">
        <div>
          <CardTitle className="text-sm">LumenDMX</CardTitle>
          <p className="mt-0.5 text-xs text-muted-foreground">
            v{currentVersion || "…"}
            {update ? ` → v${update.version}` : ""}
          </p>
        </div>
        <Button variant="ghost" size="iconSm" onClick={() => void checkNow()} disabled={checking}>
          <RefreshCw className={checking ? "animate-spin" : undefined} />
        </Button>
      </CardHeader>
      <CardContent className="flex flex-col gap-3 p-4 pt-0">
        <div className="flex items-center justify-between">
          {statusBadge}
          {update && (
            <div className="flex gap-2">
              {!downloaded && !deferred && (
                <Button size="sm" variant="outline" disabled={downloading || installing} onClick={() => void download()}>
                  <Download />
                  {downloading ? "Downloading…" : "Download only"}
                </Button>
              )}
              {deferred ? (
                <Button size="sm" variant="outline" onClick={cancelDeferred}>
                  Cancel scheduled install
                </Button>
              ) : (
                <Button size="sm" disabled={installing} onClick={() => void install()}>
                  <Download />
                  {installing
                    ? "Installing…"
                    : showMode
                      ? "Install after show"
                      : rollbackCandidate
                        ? "Install this version"
                        : downloaded
                          ? "Restart and install"
                          : "Install update"}
                </Button>
              )}
            </div>
          )}
        </div>

        {showMode && update && (
          <p className="text-xs text-muted-foreground">
            Show mode is on, so the update will wait. Turn show mode off to install it.
          </p>
        )}

        {(installing || downloading) && progress && (
          <div className="flex flex-col gap-1">
            <Progress value={progress.total ? (progress.downloaded / progress.total) * 100 : 0} />
            <p className="text-xs text-muted-foreground">
              {formatBytes(progress.downloaded)}
              {progress.total ? ` / ${formatBytes(progress.total)}` : ""}
            </p>
          </div>
        )}

        <div className="flex items-center justify-between border-t border-border pt-3">
          <p className="text-xs text-muted-foreground">
            Updates copy your preferences and autosaves aside first. To go back to an earlier version, install its
            release below, or check whether the release feed points at an older build.
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button size="sm" variant="outline" disabled={checking} onClick={() => void checkForRollback()}>
            <History />
            Check for older build
          </Button>
          {previousVersion && (
            <Button
              size="sm"
              variant="ghost"
              onClick={() => void openUrl(releasePageUrl(RELEASES_REPO, previousVersion))}
            >
              Open v{previousVersion} release page
            </Button>
          )}
        </div>

        {error && <p className="text-xs text-destructive">{error}</p>}
      </CardContent>
    </Card>
  );
}
