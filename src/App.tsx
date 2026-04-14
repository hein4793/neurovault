import { useState, useEffect } from "react";
import { BrainScene } from "@/components/brain/BrainScene";
import { Sidebar } from "@/components/layout/Sidebar";
import { SearchBar } from "@/components/hud/SearchBar";
import { StatusBar } from "@/components/hud/StatusBar";
import { ViewModeSelector } from "@/components/hud/ViewModeSelector";
import { ActivityFeed } from "@/components/sidekick/ActivityFeed";
import { SuggestionToast } from "@/components/sidekick/SuggestionToast";
import { SearchPanel } from "@/components/panels/SearchPanel";
import { NodeDetail } from "@/components/panels/NodeDetail";
import { IngestionPanel } from "@/components/panels/IngestionPanel";
import { StatsPanel } from "@/components/panels/StatsPanel";
import { ResearchPanel } from "@/components/panels/ResearchPanel";
import { SettingsPanel } from "@/components/panels/SettingsPanel";
import { AskBrainPanel } from "@/components/panels/AskBrainPanel";
import { QualityPanel } from "@/components/panels/QualityPanel";
import { LearningPanel } from "@/components/panels/LearningPanel";
import { InsightsPanel } from "@/components/panels/InsightsPanel";
import { BackupPanel } from "@/components/panels/BackupPanel";
import { AutonomyPanel } from "@/components/panels/AutonomyPanel";
import { BrainActivityPanel } from "@/components/panels/BrainActivityPanel";
import { BrainPicker } from "@/components/hud/BrainPicker";
import { ContextPanel } from "@/components/sidekick/ContextPanel";
import { KeyboardShortcutHelp } from "@/components/panels/KeyboardShortcutHelp";
import { DigitalTwinPanel } from "@/components/panels/DigitalTwinPanel";
import { SwarmPanel } from "@/components/panels/SwarmPanel";
import { WorldModelPanel } from "@/components/panels/WorldModelPanel";
import { SelfImprovePanel } from "@/components/panels/SelfImprovePanel";
import { ConsciousnessPanel } from "@/components/panels/ConsciousnessPanel";
import { useBrainData } from "@/hooks/useBrainData";
import { useBrainStats } from "@/hooks/useBrainStats";
import { useSidekickEvents } from "@/hooks/useSidekickEvents";
import { useKeyboardShortcuts } from "@/hooks/useKeyboardShortcuts";
import { useUiStore } from "@/stores/uiStore";
import { useGraphStore } from "@/stores/graphStore";

export default function App() {
  const { loadPhase } = useBrainData();
  const { activePanel } = useUiStore();
  const { selectedNode, isLoading } = useGraphStore();

  // Initialize systems
  useBrainStats();
  useSidekickEvents();
  useKeyboardShortcuts();

  const showPanel = activePanel || selectedNode;

  return (
    <div className="flex h-screen w-screen bg-brain-bg overflow-hidden max-h-screen">
      <KeyboardShortcutHelp />
      <Sidebar />

      {/* Main content - 3D Brain */}
      <div className="flex-1 min-w-0 relative">
        {/* Loading overlay */}
        {isLoading && loadPhase === "init" && (
          <div className="absolute inset-0 z-50 flex items-center justify-center bg-brain-bg/80 backdrop-blur-sm">
            <div className="flex flex-col items-center gap-4">
              <div className="w-16 h-16 border-2 border-brain-accent/30 border-t-brain-accent rounded-full animate-spin" />
              <span className="text-brain-accent text-sm font-mono animate-pulse">
                Initializing Brain...
              </span>
            </div>
          </div>
        )}

        {/* 3D Brain Scene */}
        <BrainScene />

        {/* Search bar - top center */}
        <div className="absolute top-4 left-1/2 -translate-x-1/2 z-10 w-[500px]">
          <SearchBar />
        </div>

        {/* Brain picker (Phase 4.1) — top left, next to the sidebar */}
        <div className="absolute top-4 left-20 z-20">
          <BrainPicker />
        </div>

        {/* View mode - top right */}
        <div className="absolute top-4 right-4 z-10">
          <ViewModeSelector />
        </div>

        {/* Activity feed - bottom left (above status bar) */}
        <div className="absolute bottom-16 left-4 z-10">
          <ActivityFeed />
        </div>

        {/* Status bar - bottom left */}
        <div className="absolute bottom-4 left-4 z-10">
          <StatusBar />
        </div>

        {/* Suggestion toast - bottom right */}
        <div className="absolute bottom-4 right-4 z-10">
          <SuggestionToast />
        </div>
      </div>

      {/* Right panel */}
      {showPanel && (
        <div className="w-[380px] min-w-[380px] max-h-screen h-full border-l border-brain-border/50 flex flex-col overflow-hidden">
          <div className="flex-1 glass-panel m-0 rounded-none border-0 overflow-y-auto overflow-x-hidden">
            {selectedNode && <NodeDetail />}
            {activePanel === "search" && !selectedNode && <SearchPanel />}
            {activePanel === "ingest" && !selectedNode && <IngestionPanel />}
            {activePanel === "stats" && !selectedNode && <StatsPanel />}
            {activePanel === "research" && !selectedNode && <ResearchPanel />}
            {activePanel === "settings" && !selectedNode && <SettingsPanel />}
            {activePanel === "ask" && !selectedNode && <AskBrainPanel />}
            {activePanel === "quality" && !selectedNode && <QualityPanel />}
            {activePanel === "learning" && !selectedNode && <LearningPanel />}
            {activePanel === "insights" && !selectedNode && <InsightsPanel />}
            {activePanel === "backup" && !selectedNode && <BackupPanel />}
            {activePanel === "autonomy" && !selectedNode && <AutonomyPanel />}
            {activePanel === "activity" && !selectedNode && <BrainActivityPanel />}
            {activePanel === "context" && !selectedNode && <ContextPanel />}
            {activePanel === "digital-twin" && !selectedNode && <DigitalTwinPanel />}
            {activePanel === "swarm" && !selectedNode && <SwarmPanel />}
            {activePanel === "world-model" && !selectedNode && <WorldModelPanel />}
            {activePanel === "self-improve" && !selectedNode && <SelfImprovePanel />}
            {activePanel === "consciousness" && !selectedNode && <ConsciousnessPanel />}
          </div>
        </div>
      )}
    </div>
  );
}
