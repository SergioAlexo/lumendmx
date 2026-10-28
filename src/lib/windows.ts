import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

// Tabs popped out into their own windows. Each window runs the whole
// frontend against the same Rust show, so they stay in sync through the
// usual `show://changed` and engine events.

export type MainView = "setup" | "fixtures" | "nodes";
/** What a window can be opened for: a tab, or the playback controls strip. */
export type PopView = MainView | "controls";

export const VIEW_LABELS: Record<MainView, string> = {
  setup: "Setup",
  fixtures: "Fixtures",
  nodes: "Nodes",
};

/** The view this window was opened for, or null in the main window. */
export function poppedOutView(): PopView | null {
  const view = new URLSearchParams(window.location.search).get("view");
  return view === "controls" || (view && view in VIEW_LABELS) ? (view as PopView) : null;
}

const WINDOWS: Record<PopView, { title: string; width: number; height: number; minWidth: number; minHeight: number }> = {
  setup: { title: VIEW_LABELS.setup, width: 1000, height: 750, minWidth: 480, minHeight: 360 },
  fixtures: { title: VIEW_LABELS.fixtures, width: 1000, height: 750, minWidth: 480, minHeight: 360 },
  nodes: { title: VIEW_LABELS.nodes, width: 1000, height: 750, minWidth: 480, minHeight: 360 },
  controls: { title: "Controls", width: 640, height: 320, minWidth: 160, minHeight: 120 },
};

/** The open window for `view`, if any. */
export function poppedOutWindow(view: PopView): Promise<WebviewWindow | null> {
  return WebviewWindow.getByLabel(`view-${view}`);
}

/** Opens `view` in its own window, or focuses it if it's already open. Resolves to the window. */
export async function popOut(view: PopView): Promise<WebviewWindow> {
  const existing = await poppedOutWindow(view);
  if (existing) {
    await existing.setFocus();
    return existing;
  }
  const { title, ...size } = WINDOWS[view];
  const window = new WebviewWindow(`view-${view}`, {
    url: `/?view=${view}`,
    title: `LumenDMX — ${title}`,
    ...size,
    backgroundColor: "#0c0c0e",
  });
  await new Promise<void>((resolve, reject) => {
    void window.once("tauri://created", () => resolve());
    void window.once("tauri://error", (e) => reject(new Error(String(e.payload))));
  });
  return window;
}
