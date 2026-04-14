import { useState, useEffect } from "react";
import {
  getKnowledgeGaps,
  getCuriosityQueue,
  getResearchMissions,
  createResearchMission,
  researchTopic,
  KnowledgeGap,
  CuriosityItem,
  ResearchMission,
} from "@/lib/tauri";
import { useGraphStore } from "@/stores/graphStore";

export function LearningPanel() {
  const [gaps, setGaps] = useState<KnowledgeGap[]>([]);
  const [curiosity, setCuriosity] = useState<CuriosityItem[]>([]);
  const [missions, setMissions] = useState<ResearchMission[]>([]);
  const [status, setStatus] = useState("");
  const [isLearning, setIsLearning] = useState(false);
  const [activeTab, setActiveTab] = useState<"gaps" | "curiosity" | "missions">("gaps");
  const { addNodes } = useGraphStore();

  useEffect(() => {
    loadData();
  }, []);

  const loadData = async () => {
    try {
      const [g, c, m] = await Promise.all([
        getKnowledgeGaps().catch(() => []),
        getCuriosityQueue().catch(() => []),
        getResearchMissions().catch(() => []),
      ]);
      setGaps(g);
      setCuriosity(c);
      setMissions(m);
    } catch {}
  };

  const handleLearn = async (topic: string) => {
    setIsLearning(true);
    setStatus(`Researching: ${topic}...`);
    try {
      await createResearchMission(topic);
      const nodes = await researchTopic(topic);
      addNodes(nodes);
      setStatus(`Learned ${nodes.length} things about ${topic}`);
      loadData();
    } catch (err) {
      setStatus(`Error: ${err}`);
    } finally {
      setIsLearning(false);
    }
  };

  const tabs = [
    { id: "gaps" as const, label: "Knowledge Gaps", count: gaps.length },
    { id: "curiosity" as const, label: "Curiosity Queue", count: curiosity.length },
    { id: "missions" as const, label: "Missions", count: missions.length },
  ];

  return (
    <div className="p-4 flex flex-col h-full">
      <h2 className="text-lg font-semibold mb-4 flex items-center gap-2 text-brain-text">
        <svg className="w-5 h-5 text-brain-research" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253" />
        </svg>
        Autonomous Learning
      </h2>

      {status && (
        <div className="mb-3 text-xs font-mono px-3 py-2 rounded-lg bg-brain-research/10 text-brain-research border border-brain-research/20">
          {status}
        </div>
      )}

      {/* Tabs */}
      <div className="flex gap-1 mb-3">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            onClick={() => setActiveTab(tab.id)}
            className={`flex-1 text-xs font-mono py-1.5 rounded-lg transition-colors ${
              activeTab === tab.id
                ? "bg-brain-research/20 text-brain-research border border-brain-research/30"
                : "text-brain-muted hover:text-brain-text border border-brain-border/30"
            }`}
          >
            {tab.label} ({tab.count})
          </button>
        ))}
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto space-y-1.5 min-h-0">
        {activeTab === "gaps" && gaps.map((gap, i) => (
          <div key={i} className="flex items-center gap-2 text-xs font-mono px-3 py-2 rounded-lg bg-brain-bg/50 border border-brain-border/30 group">
            <div className="flex-1">
              <div className="text-brain-text">{gap.topic}</div>
              <div className="text-brain-muted/60 text-[10px]">{gap.reason}</div>
            </div>
            <div className="text-brain-research/50 text-[10px]">{Math.round(gap.priority * 100)}%</div>
            <button
              onClick={() => handleLearn(gap.topic)}
              disabled={isLearning}
              className="opacity-0 group-hover:opacity-100 text-[10px] px-2 py-0.5 rounded bg-brain-research/20 text-brain-research hover:bg-brain-research/30 transition-all disabled:opacity-30"
            >
              Learn
            </button>
          </div>
        ))}

        {activeTab === "curiosity" && curiosity.map((item, i) => (
          <div key={i} className="flex items-center gap-2 text-xs font-mono px-3 py-2 rounded-lg bg-brain-bg/50 border border-brain-border/30 group">
            <div className={`w-1.5 h-1.5 rounded-full ${
              item.source === "gap" ? "bg-red-400" :
              item.source === "popular" ? "bg-green-400" :
              item.source === "edge_topic" ? "bg-amber-400" : "bg-blue-400"
            }`} />
            <div className="flex-1">
              <div className="text-brain-text">{item.topic}</div>
              <div className="text-brain-muted/60 text-[10px]">{item.reason}</div>
            </div>
            <button
              onClick={() => handleLearn(item.topic)}
              disabled={isLearning}
              className="opacity-0 group-hover:opacity-100 text-[10px] px-2 py-0.5 rounded bg-brain-research/20 text-brain-research hover:bg-brain-research/30 transition-all disabled:opacity-30"
            >
              Learn
            </button>
          </div>
        ))}

        {activeTab === "missions" && missions.map((m, i) => (
          <div key={i} className="text-xs font-mono px-3 py-2 rounded-lg bg-brain-bg/50 border border-brain-border/30">
            <div className="flex items-center justify-between">
              <span className="text-brain-text">{m.topic}</span>
              <span className={`text-[10px] px-1.5 py-0.5 rounded ${
                m.status === "completed" ? "bg-green-500/20 text-green-400" :
                m.status === "active" ? "bg-blue-500/20 text-blue-400" :
                "bg-brain-border/30 text-brain-muted"
              }`}>{m.status}</span>
            </div>
            {m.nodes_created > 0 && (
              <div className="text-brain-muted/50 text-[10px] mt-0.5">+{m.nodes_created} neurons</div>
            )}
          </div>
        ))}

        {((activeTab === "gaps" && gaps.length === 0) ||
          (activeTab === "curiosity" && curiosity.length === 0) ||
          (activeTab === "missions" && missions.length === 0)) && (
          <div className="text-center text-brain-muted/50 text-xs font-mono py-4">
            No items yet. The brain needs more knowledge first.
          </div>
        )}
      </div>

      {/* Refresh button */}
      <button
        onClick={loadData}
        className="mt-3 w-full py-2 rounded-lg bg-brain-research/10 text-brain-research text-xs font-mono hover:bg-brain-research/20 transition-colors border border-brain-research/20"
      >
        Refresh Analysis
      </button>
    </div>
  );
}
