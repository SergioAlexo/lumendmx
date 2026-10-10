/** When an app update may be installed. Installing restarts the app, which
 * would drop the operator out of a running show, so show mode defers it. */

export interface InstallContext {
  showMode: boolean;
  hasUpdate: boolean;
  installing: boolean;
}

export type InstallDecision = { action: "install" } | { action: "defer"; reason: string } | { action: "none" };

export function decideInstall(ctx: InstallContext): InstallDecision {
  if (!ctx.hasUpdate || ctx.installing) return { action: "none" };
  if (ctx.showMode) {
    return { action: "defer", reason: "Show mode is on. The update will install when show mode is turned off." };
  }
  return { action: "install" };
}

/** Whether a previously deferred install should run now. */
export function shouldRunDeferred(deferred: boolean, ctx: InstallContext): boolean {
  return deferred && decideInstall(ctx).action === "install";
}

/** Link to the release page of the version to go back to, for a manual
 * rollback. `repo` is "owner/name". */
export function releasePageUrl(repo: string, version: string): string {
  const tag = version.startsWith("v") ? version : `v${version}`;
  return `https://github.com/${repo}/releases/tag/${tag}`;
}
