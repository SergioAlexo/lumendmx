import { useAppUpdateStore } from "../store/useAppUpdateStore";
import { useHealthStore } from "../store/useHealthStore";
import { useMessagesStore } from "../store/useMessagesStore";
import { usePrefsStore } from "../store/usePrefsStore";
import { useShowModeStore } from "../store/useShowModeStore";
import { installUiLogger } from "./uiLogger";

/** Starts the app-wide reliability plumbing once, before React renders (so
 * StrictMode's double effects can't register listeners twice): UI error
 * forwarding, saved preferences applied to the document, show mode, the
 * Messages feed and the watchdog feed. */
export function initReliability(): void {
  installUiLogger();
  void usePrefsStore.getState().init();
  void useShowModeStore.getState().init();
  void useMessagesStore.getState().init();
  void useHealthStore.getState().init();
  void useAppUpdateStore.getState().noteVersionChange();
}
