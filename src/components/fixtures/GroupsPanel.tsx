import { useState } from "react";
import { ChevronDown, ChevronRight, GripVertical, Plus, Trash2, X } from "lucide-react";
import * as showApi from "../../lib/show";
import type { Group } from "../../lib/showfile/generated";
import { cn } from "../../lib/utils";
import { EMPTY_GROUPS, EMPTY_PATCH, useShowStore } from "../../store/useShowStore";
import { Button } from "../ui/button";
import { Input } from "../ui/input";

/** Groups: named, ordered fixture sets the node graph targets. The order is
 * the fan order effects spread along; drag members to change it. */
export function GroupsPanel({ selected, onSelect }: { selected: number[]; onSelect: (numbers: number[]) => void }) {
  const groups = useShowStore((s) => s.show?.groups ?? EMPTY_GROUPS);
  const run = useShowStore((s) => s.run);
  const [open, setOpen] = useState<number | null>(null);

  const create = async () => {
    const group = await run(() => showApi.createGroup("", selected));
    if (group) setOpen(group.id);
  };

  return (
    <aside className="flex w-72 shrink-0 flex-col overflow-hidden border-l border-border bg-card">
      <div className="flex items-center justify-between gap-2 border-b border-border px-3 py-2">
        <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">Groups</h3>
        <Button size="sm" variant="secondary" disabled={selected.length === 0} onClick={() => void create()} title="New group from the selected fixtures, in the order you picked them">
          <Plus />
          From selection
        </Button>
      </div>
      <div className="flex-1 overflow-y-auto p-2">
        {groups.length === 0 && (
          <p className="p-2 text-xs text-muted-foreground">Select fixtures (⌘/shift-click), then make a group from them.</p>
        )}
        <div className="flex flex-col gap-1.5">
          {groups.map((group) => (
            <GroupCard
              key={group.id}
              group={group}
              open={open === group.id}
              selected={selected}
              onToggle={() => setOpen(open === group.id ? null : group.id)}
              onSelect={() => onSelect(group.fixtureNumbers)}
            />
          ))}
        </div>
      </div>
    </aside>
  );
}

function GroupCard({
  group,
  open,
  selected,
  onToggle,
  onSelect,
}: {
  group: Group;
  open: boolean;
  selected: number[];
  onToggle: () => void;
  onSelect: () => void;
}) {
  const patch = useShowStore((s) => s.show?.patch ?? EMPTY_PATCH);
  const run = useShowStore((s) => s.run);
  const [name, setName] = useState(group.name);
  const [dragging, setDragging] = useState<number | null>(null);

  const setMembers = (fixtureNumbers: number[]) => void run(() => showApi.updateGroup(group.id, { fixtureNumbers }));
  const isActive = selected.length > 0 && selected.length === group.fixtureNumbers.length && group.fixtureNumbers.every((n, i) => selected[i] === n);
  const nameOf = (n: number) => patch.find((f) => f.fixtureNumber === n)?.name ?? "?";

  const drop = (target: number) => {
    if (dragging === null || dragging === target) return;
    const members = [...group.fixtureNumbers];
    const [moved] = members.splice(dragging, 1);
    members.splice(target, 0, moved);
    setDragging(null);
    setMembers(members);
  };

  return (
    <div className={cn("rounded-md border", isActive ? "border-primary" : "border-border")}>
      <div className="flex items-center gap-1 px-1.5 py-1">
        <button className="text-muted-foreground hover:text-foreground" onClick={onToggle} title={open ? "Collapse" : "Show members"}>
          {open ? <ChevronDown className="h-4 w-4" /> : <ChevronRight className="h-4 w-4" />}
        </button>
        <button className="flex flex-1 items-center justify-between gap-2 truncate text-left text-sm" onClick={onSelect} title="Select this group's fixtures">
          <span className="truncate">{group.name}</span>
          <span className="shrink-0 text-xs text-muted-foreground">{group.fixtureNumbers.length}</span>
        </button>
      </div>

      {open && (
        <div className="flex flex-col gap-2 border-t border-border p-2">
          <Input
            className="h-7 text-xs"
            aria-label="Group name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onBlur={() => {
              if (name.trim() && name !== group.name) void run(() => showApi.updateGroup(group.id, { name: name.trim() }));
            }}
            onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
          />
          <ol className="flex flex-col">
            {group.fixtureNumbers.map((n, i) => (
              <li
                key={n}
                draggable
                onDragStart={() => setDragging(i)}
                onDragOver={(e) => e.preventDefault()}
                onDrop={() => drop(i)}
                onDragEnd={() => setDragging(null)}
                className={cn("flex items-center gap-1.5 rounded px-1 py-0.5 text-xs", dragging === i ? "opacity-40" : "hover:bg-accent/40")}
              >
                <GripVertical className="h-3.5 w-3.5 cursor-grab text-muted-foreground" />
                <span className="w-5 text-right text-muted-foreground">{i + 1}</span>
                <span className="truncate">
                  #{n} {nameOf(n)}
                </span>
                <div className="flex-1" />
                <button
                  className="text-muted-foreground hover:text-foreground"
                  title="Remove from group"
                  onClick={() => setMembers(group.fixtureNumbers.filter((m) => m !== n))}
                >
                  <X className="h-3.5 w-3.5" />
                </button>
              </li>
            ))}
          </ol>
          <div className="flex flex-wrap gap-1">
            <Button
              size="sm"
              variant="ghost"
              disabled={selected.every((n) => group.fixtureNumbers.includes(n))}
              onClick={() => setMembers([...group.fixtureNumbers, ...selected.filter((n) => !group.fixtureNumbers.includes(n))])}
            >
              Add selected
            </Button>
            <Button size="sm" variant="ghost" disabled={selected.length === 0} onClick={() => setMembers(selected)}>
              Replace with selected
            </Button>
            <div className="flex-1" />
            <Button size="iconSm" variant="ghost" title="Delete group" onClick={() => void run(() => showApi.deleteGroup(group.id))}>
              <Trash2 />
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
