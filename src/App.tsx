import { useEffect, useState } from "react";
import { SquareArrowOutUpRight } from "lucide-react";
import { TopBar } from "./components/TopBar";
import { useShowShortcuts } from "./components/ShowMenu";
import { FixturesPanel } from "./components/fixtures/FixturesPanel";
import { SetupPanel } from "./components/SetupPanel";
import { NodeEditor } from "./components/nodes/NodeEditor";
import { PlaybackBar } from "./components/PlaybackBar";
import { Button } from "./components/ui/button";
import { cn } from "./lib/utils";
import { type MainView, VIEW_LABELS, popOut, poppedOutView } from "./lib/windows";
import { useDmxStore } from "./store/useDmxStore";
import { useAppUpdateStore } from "./store/useAppUpdateStore";
import { useGraphStore } from "./store/useGraphStore";
import { useShowStore } from "./store/useShowStore";
import { useUniverseStatusesStore } from "./store/useUniverseStatusesStore";
import { useUniverseChannelsStore } from "./store/useUniverseChannelsStore";

/** Set once per window: a popped-out tab shows just that view. The top bar
 * and playback bar stay in the main window. */
const POPPED_OUT = poppedOutView();

function ViewPanel({ view }: { view: MainView }) {
  return view === "setup" ? <SetupPanel /> : view === "fixtures" ? <FixturesPanel /> : <NodeEditor />;
}

function ErrorBanner() {
  const error = useShowStore((s) => s.error);
  if (!error) return null;
  return (
    <div className="flex items-center justify-between gap-3 border-b border-destructive/40 bg-destructive/10 px-4 py-1.5 text-xs text-destructive">
      <span>{error}</span>
      <button className="underline-offset-2 hover:underline" onClick={() => useShowStore.setState({ error: null })}>
        Dismiss
      </button>
    </div>
  );
}

function App() {
  const init = useDmxStore((s) => s.init);
  const checkForAppUpdate = useAppUpdateStore((s) => s.checkNow);
  const initShow = useShowStore((s) => s.init);
  const initGraph = useGraphStore((s) => s.init);
  const showLoadCount = useShowStore((s) => s.loadCount);
  const initUniverseStatuses = useUniverseStatusesStore((s) => s.init);
  const initUniverseChannels = useUniverseChannelsStore((s) => s.init);
  const [mainView, setMainView] = useState<MainView>("nodes");
  const focus = useGraphStore((s) => s.focus);
  useShowShortcuts();

  // Showing a node (right-click on the playback bar) needs the Nodes tab.
  useEffect(() => {
    if (focus) setMainView("nodes");
  }, [focus]);

  useEffect(() => {
    if (!POPPED_OUT) {
      void init();
      void checkForAppUpdate();
    }
    void initShow();
    void initGraph();
    void initUniverseStatuses();
    void initUniverseChannels();
  }, [init, checkForAppUpdate, initShow, initGraph, initUniverseStatuses, initUniverseChannels]);

  // Keyed on show loads, so panels drop local state (selection, drafts)
  // belonging to the previous show.
  if (POPPED_OUT) {
    return (
      <div className="flex h-screen flex-col bg-background">
        <ErrorBanner />
        <div key={showLoadCount} className="flex-1 overflow-hidden">
          <ViewPanel view={POPPED_OUT} />
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-screen flex-col bg-background">
      <TopBar />
      <main className="flex flex-1 flex-col overflow-hidden">
        <div className="flex gap-1 border-b border-border px-4 py-2">
          {(Object.keys(VIEW_LABELS) as MainView[]).map((view) => (
            <div key={view} className="group flex items-center">
              <Button
                size="sm"
                variant={mainView === view ? "default" : "ghost"}
                className={cn("uppercase", mainView !== view && "text-muted-foreground")}
                onClick={() => setMainView(view)}
              >
                {VIEW_LABELS[view]}
              </Button>
              <Button
                size="iconSm"
                variant="ghost"
                title={`Open ${VIEW_LABELS[view]} in a new window`}
                className="text-muted-foreground opacity-0 group-hover:opacity-100 focus-visible:opacity-100 [&_svg]:size-3.5"
                onClick={() => void popOut(view).catch((err) => useShowStore.setState({ error: `Couldn't open window: ${err}` }))}
              >
                <SquareArrowOutUpRight />
              </Button>
            </div>
          ))}
        </div>
        <ErrorBanner />
        <div key={showLoadCount} className="flex-1 overflow-hidden">
          <ViewPanel view={mainView} />
        </div>
        <PlaybackBar />
      </main>
    </div>
  );
}

export default App;
