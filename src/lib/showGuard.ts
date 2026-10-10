/** Pure decisions behind show mode and confirm-on-delete, kept free of
 * stores and Tauri so they are unit-testable. The stores in
 * `store/useShowModeStore.ts` wire them to live state; Rust enforces the
 * same rule independently in the command handlers. */

export interface GuardResult {
  allowed: boolean;
  /** Shown to the operator when `allowed` is false. */
  message?: string;
}

export function checkEditAllowed(locked: boolean, action: string): GuardResult {
  if (!locked) return { allowed: true };
  return { allowed: false, message: `Show mode is on: can't ${action}. Turn show mode off to edit.` };
}

/** Whether to ask "are you sure?" before a delete. Show mode blocks the
 * delete outright, so there is nothing to confirm. */
export function needsDeleteConfirmation(confirmOnDelete: boolean, locked: boolean): boolean {
  return confirmOnDelete && !locked;
}
