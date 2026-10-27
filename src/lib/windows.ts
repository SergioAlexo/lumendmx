import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

// Tabs popped out into their own windows. Each window runs the whole
// frontend against the same Rust show, so they stay in sync through the
// usual `show://changed` and engine events.

export type MainView = "setup" | "fixtures" | "nodes";

export const VIEW_LABELS: Record<MainView, string> = {
  setup: "Setup",
  fixtures: "Fixtures",
  nodes: "Nodes",
};

/** The view this window was opened for, or null in the main window. */
export function poppedOutView(): MainView | null {
  const view = new URLSearchParams(window.location.search).get("view");
  return view && view in VIEW_LABELS ? (view as MainView) : null;
}

/** Opens `view` in its own window, or focuses it if it's already open. */
export async function popOut(view: MainView): Promise<void> {
  const label = `view-${view}`;
  const existing = await WebviewWindow.getByLabel(label);
  if (existing) {
    await existing.setFocus();
    return;
  }
  const window = new WebviewWindow(label, {
    url: `/?view=${view}`,
    title: `LumenDMX — ${VIEW_LABELS[view]}`,
    width: 1000,
    height: 750,
    minWidth: 480,
    minHeight: 360,
    backgroundColor: "#0c0c0e",
  });
  await new Promise<void>((resolve, reject) => {
    void window.once("tauri://created", () => resolve());
    void window.once("tauri://error", (e) => reject(new Error(String(e.payload))));
  });
}
