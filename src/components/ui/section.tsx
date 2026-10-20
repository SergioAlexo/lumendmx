import type * as React from "react";

/** A titled card for a settings page, with optional buttons in its header. */
export function Section({ title, actions, children }: { title: string; actions?: React.ReactNode; children: React.ReactNode }) {
  return (
    <section className="rounded-md border border-border bg-card p-4">
      <div className="mb-3 flex min-h-8 items-center justify-between gap-2">
        <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{title}</h3>
        {actions}
      </div>
      <div className="flex flex-col gap-3">{children}</div>
    </section>
  );
}
