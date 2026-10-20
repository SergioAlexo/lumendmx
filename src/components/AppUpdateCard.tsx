import { Download, RefreshCw } from "lucide-react";
import { useAppUpdateStore } from "../store/useAppUpdateStore";
import { Card, CardContent, CardHeader, CardTitle } from "./ui/card";
import { Badge } from "./ui/badge";
import { Button } from "./ui/button";
import { Progress, formatBytes } from "./ui/progress";

/** App self-update (SRS 2.5), driven entirely by @tauri-apps/plugin-updater's
 * `check()` result -- no separate GitHub-polling command for the version badge.
 * The initial check runs once at app startup (see App.tsx), so the result is
 * ready by the time Setup is opened. */
export function AppUpdateCard() {
  const { currentVersion, update, checking, installing, progress, error, checked, checkNow, install } =
    useAppUpdateStore();

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
            <Button size="sm" disabled={installing} onClick={() => void install()}>
              <Download />
              {installing ? "Installing…" : "Install update"}
            </Button>
          )}
        </div>

        {installing && progress && (
          <div className="flex flex-col gap-1">
            <Progress value={progress.total ? (progress.downloaded / progress.total) * 100 : 0} />
            <p className="text-xs text-muted-foreground">
              {formatBytes(progress.downloaded)}
              {progress.total ? ` / ${formatBytes(progress.total)}` : ""}
            </p>
          </div>
        )}

        {error && <p className="text-xs text-destructive">{error}</p>}
      </CardContent>
    </Card>
  );
}
