import { useEffect, useState } from "react";
import { TopBar } from "./components/TopBar";
import { TriggerGrid } from "./components/TriggerGrid";
import { CuePanel } from "./components/CuePanel";
import { PlaybackBar } from "./components/PlaybackBar";
import { AiSettings } from "./components/AiSettings";
import { SystemPanel } from "./components/SystemPanel";
import { FixturePatchPanel } from "./components/FixturePatchPanel";
import { ProgrammerPanel } from "./components/ProgrammerPanel";
import { CommandLine } from "./components/CommandLine";
import { LiveAiConsole } from "./components/LiveAiConsole";
import { SetupPanel } from "./components/SetupPanel";
import { PalettesPanel } from "./components/PalettesPanel";
import { Button } from "./components/ui/button";
import { cn } from "./lib/utils";
import { useDmxStore } from "./store/useDmxStore";
import { useMidiStore } from "./store/useMidiStore";
import { useAppUpdateStore } from "./store/useAppUpdateStore";
import { useShowStore } from "./store/useShowStore";
import { useUniverseStatusesStore } from "./store/useUniverseStatusesStore";
import { useUniverseChannelsStore } from "./store/useUniverseChannelsStore";

type MainView = "fixtures" | "cues" | "cuelists" | "palettes" | "setup";

function App() {
  const init = useDmxStore((s) => s.init);
  const initMidi = useMidiStore((s) => s.init);
  const checkForAppUpdate = useAppUpdateStore((s) => s.checkNow);
  const initShow = useShowStore((s) => s.init);
  const showLoadCount = useShowStore((s) => s.loadCount);
  const initUniverseStatuses = useUniverseStatusesStore((s) => s.init);
  const initUniverseChannels = useUniverseChannelsStore((s) => s.init);
  const [mainView, setMainView] = useState<MainView>("fixtures");

  useEffect(() => {
    void init();
    void initMidi();
    void checkForAppUpdate();
    void initShow();
    void initUniverseStatuses();
    void initUniverseChannels();
  }, [init, initMidi, checkForAppUpdate, initShow, initUniverseStatuses, initUniverseChannels]);

  return (
    <div className="flex h-screen flex-col bg-background">
      <TopBar />
      <div className="flex flex-1 overflow-hidden">
        <main className="flex flex-1 flex-col overflow-hidden">
          <div className="flex gap-1 border-b border-border px-4 py-2">
            {(["fixtures", "cues", "cuelists", "palettes", "setup"] as MainView[]).map((view) => (
              <Button
                key={view}
                size="sm"
                variant={mainView === view ? "default" : "ghost"}
                className={cn("uppercase", mainView !== view && "text-muted-foreground")}
                onClick={() => setMainView(view)}
              >
                {view === "fixtures"
                  ? "Fixtures"
                  : view === "cues"
                    ? "Triggers"
                    : view === "cuelists"
                      ? "Cuelists"
                      : view === "palettes"
                        ? "Palettes"
                        : "Setup"}
              </Button>
            ))}
          </div>
          {/* Keyed on show loads: panels fetch engine data (cuelists, presets,
              groups) on mount, so remount them when a different show loads. */}
          <div key={showLoadCount} className="flex-1 overflow-hidden">
            {mainView === "fixtures" ? (
              <FixturePatchPanel />
            ) : mainView === "cues" ? (
              <TriggerGrid />
            ) : mainView === "cuelists" ? (
              <CuePanel />
            ) : mainView === "palettes" ? (
              <PalettesPanel />
            ) : (
              <SetupPanel />
            )}
          </div>
          <PlaybackBar />
        </main>
        <aside className="flex w-80 flex-col overflow-y-auto border-l border-border bg-card">
          <AiSettings />
          <SystemPanel />
          <CommandLine />
          <ProgrammerPanel />
          <div className="flex-1" />
          <LiveAiConsole />
        </aside>
      </div>
    </div>
  );
}

export default App;
