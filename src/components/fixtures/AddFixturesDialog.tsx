import { useRef, useState } from "react";
import { X } from "lucide-react";
import * as showApi from "../../lib/show";
import type { FixtureDefinition } from "../../lib/fixtures/types";
import { validateFixtureDefinition } from "../../lib/fixtures/validate";
import { importOfl } from "../../lib/fixtures/importers/ofl";
import { importQlcPlus } from "../../lib/fixtures/importers/qlcplus";
import { useAllFixtures, useFixtureLibraryStore } from "../../store/useFixtureLibraryStore";
import { EMPTY_UNIVERSES, useShowStore } from "../../store/useShowStore";
import { FixtureEditor } from "../FixtureEditor";
import { Button } from "../ui/button";
import { Input, Select } from "../ui/input";

/** Patch fixtures of one type: pick type, mode, universe and how many; the
 * address and fixture numbers are found automatically unless given. Also
 * where fixture types are imported (JSON, OFL, QLC+) or authored. */
export function AddFixturesDialog({ initialUniverse, onClose }: { initialUniverse?: number; onClose: () => void }) {
  const library = useAllFixtures();
  const customFixtures = useFixtureLibraryStore((s) => s.customFixtures);
  const addCustomFixture = useFixtureLibraryStore((s) => s.addCustomFixture);
  const universes = useShowStore((s) => s.show?.universes ?? EMPTY_UNIVERSES);

  const [fixtureType, setFixtureType] = useState(library[0]?.id ?? "");
  const [modeIndex, setModeIndex] = useState(0);
  const [universe, setUniverse] = useState(initialUniverse ?? universes[0]?.id ?? 1);
  const [count, setCount] = useState(1);
  const [address, setAddress] = useState("");
  const [firstNumber, setFirstNumber] = useState("");
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);

  const jsonInput = useRef<HTMLInputElement>(null);
  const oflInput = useRef<HTMLInputElement>(null);
  const qlcInput = useRef<HTMLInputElement>(null);

  const def = library.find((f) => f.id === fixtureType);
  const mode = def?.modes[modeIndex];
  const isCustom = def !== undefined && customFixtures.some((f) => f.id === def.id);

  const acceptImported = (imported: FixtureDefinition) => {
    const errors = validateFixtureDefinition(imported);
    if (errors.length > 0) {
      setError(errors.join("; "));
      return;
    }
    addCustomFixture(imported);
    setFixtureType(imported.id);
    setModeIndex(0);
  };

  const importFile = async (file: File, parse: (text: string) => FixtureDefinition) => {
    setError(null);
    try {
      acceptImported(parse(await file.text()));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const add = async () => {
    setError(null);
    try {
      await showApi.addFixtures({
        fixtureType,
        modeIndex,
        universe,
        count,
        address: address.trim() ? Number(address) : undefined,
        firstNumber: firstNumber.trim() ? Number(firstNumber) : undefined,
        name: name.trim() || undefined,
      });
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const filePicker = (ref: React.RefObject<HTMLInputElement | null>, accept: string, parse: (text: string) => FixtureDefinition) => (
    <input
      ref={ref}
      type="file"
      accept={accept}
      className="hidden"
      onChange={(e) => {
        const file = e.target.files?.[0];
        if (file) void importFile(file, parse);
        e.target.value = "";
      }}
    />
  );

  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/60 p-4" onMouseDown={onClose}>
      <div
        className="flex w-full max-w-xl flex-col gap-4 rounded-lg border border-border bg-card p-5 shadow-xl"
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "Escape") onClose();
          if (e.key === "Enter" && (e.target as HTMLElement).tagName !== "SELECT") void add();
        }}
      >
        <div className="flex items-center justify-between">
          <h2 className="text-sm font-semibold">Add fixtures</h2>
          <Button size="iconSm" variant="ghost" onClick={onClose} title="Close">
            <X />
          </Button>
        </div>

        <div className="grid grid-cols-2 gap-3 text-xs">
          <label className="col-span-2 flex flex-col gap-1">
            Fixture type
            <Select
              autoFocus
              value={fixtureType}
              onChange={(e) => {
                setFixtureType(e.target.value);
                setModeIndex(0);
              }}
            >
              {library.map((f) => (
                <option key={f.id} value={f.id}>
                  {f.manufacturer} {f.model}
                </option>
              ))}
            </Select>
          </label>
          <label className="flex flex-col gap-1">
            Mode
            <Select value={modeIndex} onChange={(e) => setModeIndex(Number(e.target.value))}>
              {def?.modes.map((m, i) => (
                <option key={m.name} value={i}>
                  {m.name} ({m.channelCount}ch)
                </option>
              ))}
            </Select>
          </label>
          <label className="flex flex-col gap-1">
            Universe
            <Select value={universe} onChange={(e) => setUniverse(Number(e.target.value))}>
              {universes.map((u) => (
                <option key={u.id} value={u.id}>
                  U{u.id} · {u.name}
                </option>
              ))}
            </Select>
          </label>
          <label className="flex flex-col gap-1">
            How many
            <Input type="number" min={1} max={512} value={count} onChange={(e) => setCount(Math.max(1, Number(e.target.value) || 1))} />
          </label>
          <label className="flex flex-col gap-1">
            Start address
            <Input type="number" min={1} max={512} placeholder="first free" value={address} onChange={(e) => setAddress(e.target.value)} />
          </label>
          <label className="flex flex-col gap-1">
            First fixture #
            <Input type="number" min={1} placeholder="next free" value={firstNumber} onChange={(e) => setFirstNumber(e.target.value)} />
          </label>
          <label className="flex flex-col gap-1">
            Name
            <Input placeholder={def?.model ?? ""} value={name} onChange={(e) => setName(e.target.value)} />
          </label>
        </div>

        {mode && (
          <p className="text-xs text-muted-foreground">
            {count} × {mode.channelCount} channels = {count * mode.channelCount} channels
            {count > 1 ? ", patched back to back" : ""}.
          </p>
        )}
        {universes.length === 0 && <p className="text-xs text-destructive">Add a universe in Setup first.</p>}
        {error && <p className="text-xs text-destructive">{error}</p>}

        <div className="flex flex-wrap items-center gap-2 border-t border-border pt-3">
          <span className="mr-1 text-xs text-muted-foreground">Fixture types:</span>
          <Button size="sm" variant="ghost" onClick={() => jsonInput.current?.click()}>
            Import JSON
          </Button>
          <Button size="sm" variant="ghost" onClick={() => oflInput.current?.click()}>
            Import OFL
          </Button>
          <Button size="sm" variant="ghost" onClick={() => qlcInput.current?.click()}>
            Import QLC+
          </Button>
          <Button size="sm" variant="ghost" onClick={() => setEditorOpen(true)}>
            {isCustom ? "Edit type" : "New type"}
          </Button>
          {filePicker(jsonInput, "application/json", (text) => JSON.parse(text) as FixtureDefinition)}
          {filePicker(oflInput, "application/json", importOfl)}
          {filePicker(qlcInput, ".qxf,application/xml,text/xml", importQlcPlus)}
          <div className="flex-1" />
          <Button onClick={() => void add()} disabled={!def || universes.length === 0}>
            Add {count > 1 ? `${count} fixtures` : "fixture"}
          </Button>
        </div>
      </div>

      {editorOpen && (
        <div onMouseDown={(e) => e.stopPropagation()} onKeyDown={(e) => e.stopPropagation()}>
          <FixtureEditor initial={isCustom ? def : undefined} onClose={() => setEditorOpen(false)} />
        </div>
      )}
    </div>
  );
}
