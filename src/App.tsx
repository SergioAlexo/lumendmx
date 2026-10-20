import { useEffect, useState } from "react";
import { TopBar } from "./components/TopBar";
import { FixturesPanel } from "./components/fixtures/FixturesPanel";
import { SetupPanel } from "./components/SetupPanel";
import { Button } from "./components/ui/button";
import { cn } from "./lib/utils";
import { useDmxStore } from "./store/useDmxStore";
import { useAppUpdateStore } from "./store/useAppUpdateStore";
import { useShowStore } from "./store/useShowStore";
import { useUniverseStatusesStore } from "./store/useUniverseStatusesStore";
import { useUniverseChannelsStore } from "./store/useUniverseChannelsStore";

type MainView = "setup" | "fixtures" | "nodes";

const VIEW_LABELS: Record<MainView, string> = {
  setup: "Setup",
  fixtures: "Fixtures",
  nodes: "Nodes",
};

function App() {
  const init = useDmxStore((s) => s.init);
  const checkForAppUpdate = useAppUpdateStore((s) => s.checkNow);
  const initShow = useShowStore((s) => s.init);
  const showLoadCount = useShowStore((s) => s.loadCount);
  const error = useShowStore((s) => s.error);
  const initUniverseStatuses = useUniverseStatusesStore((s) => s.init);
  const initUniverseChannels = useUniverseChannelsStore((s) => s.init);
  const [mainView, setMainView] = useState<MainView>("fixtures");

  useEffect(() => {
    void init();
    void checkForAppUpdate();
    void initShow();
    void initUniverseStatuses();
    void initUniverseChannels();
  }, [init, checkForAppUpdate, initShow, initUniverseStatuses, initUniverseChannels]);

  return (
    <div className="flex h-screen flex-col bg-background">
      <TopBar />
      <main className="flex flex-1 flex-col overflow-hidden">
        <div className="flex gap-1 border-b border-border px-4 py-2">
          {(Object.keys(VIEW_LABELS) as MainView[]).map((view) => (
            <Button
              key={view}
              size="sm"
              variant={mainView === view ? "default" : "ghost"}
              className={cn("uppercase", mainView !== view && "text-muted-foreground")}
              onClick={() => setMainView(view)}
            >
              {VIEW_LABELS[view]}
            </Button>
          ))}
        </div>
        {error && (
          <div className="flex items-center justify-between gap-3 border-b border-destructive/40 bg-destructive/10 px-4 py-1.5 text-xs text-destructive">
            <span>{error}</span>
            <button className="underline-offset-2 hover:underline" onClick={() => useShowStore.setState({ error: null })}>
              Dismiss
            </button>
          </div>
        )}
        {/* Keyed on show loads, so panels drop local state (selection,
            drafts) belonging to the previous show. */}
        <div key={showLoadCount} className="flex-1 overflow-hidden">
          {mainView === "setup" ? (
            <SetupPanel />
          ) : mainView === "fixtures" ? (
            <FixturesPanel />
          ) : (
            <div className="p-8 text-sm text-muted-foreground">The node editor is coming next.</div>
          )}
        </div>
      </main>
    </div>
  );
}

export default App;
