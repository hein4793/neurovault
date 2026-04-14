import { useState, useEffect } from "react";
import {
  getCognitiveFingerprint,
  simulateDecision,
  runDialogue,
  synthesizeFingerprint,
  CognitiveFingerprint,
  DecisionSimulation,
  InternalDialogue,
} from "@/lib/tauri";

const FINGERPRINT_DIMS: { key: keyof CognitiveFingerprint; label: string; color: string }[] = [
  { key: "risk_tolerance", label: "Risk Tolerance", color: "bg-red-500" },
  { key: "decision_speed", label: "Decision Speed", color: "bg-amber-500" },
  { key: "analytical_depth", label: "Analytical Depth", color: "bg-blue-500" },
  { key: "creativity", label: "Creativity", color: "bg-purple-500" },
  { key: "pattern_recognition", label: "Pattern Recognition", color: "bg-emerald-500" },
  { key: "abstraction_level", label: "Abstraction Level", color: "bg-indigo-500" },
  { key: "detail_orientation", label: "Detail Orientation", color: "bg-cyan-500" },
  { key: "learning_agility", label: "Learning Agility", color: "bg-pink-500" },
];

const ROLE_COLORS: Record<string, string> = {
  advocate: "text-green-400 border-green-500/30 bg-green-500/10",
  critic: "text-red-400 border-red-500/30 bg-red-500/10",
  synthesizer: "text-violet-400 border-violet-500/30 bg-violet-500/10",
};

export function DigitalTwinPanel() {
  const [fingerprint, setFingerprint] = useState<CognitiveFingerprint | null>(null);
  const [decision, setDecision] = useState<DecisionSimulation | null>(null);
  const [dialogue, setDialogue] = useState<InternalDialogue | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isSynthesizing, setIsSynthesizing] = useState(false);
  const [isSimulating, setIsSimulating] = useState(false);
  const [isDebating, setIsDebating] = useState(false);
  const [questionInput, setQuestionInput] = useState("");
  const [topicInput, setTopicInput] = useState("");
  const [activeTab, setActiveTab] = useState<"fingerprint" | "decision" | "dialogue">("fingerprint");

  useEffect(() => {
    loadFingerprint();
  }, []);

  const loadFingerprint = async () => {
    setIsLoading(true);
    try {
      const fp = await getCognitiveFingerprint();
      setFingerprint(fp);
    } catch (err) {
      setError(String(err));
    } finally {
      setIsLoading(false);
    }
  };

  const handleSynthesize = async () => {
    setIsSynthesizing(true);
    setError(null);
    try {
      const fp = await synthesizeFingerprint();
      setFingerprint(fp);
    } catch (err) {
      setError(String(err));
    } finally {
      setIsSynthesizing(false);
    }
  };

  const handleSimulate = async () => {
    if (!questionInput.trim()) return;
    setIsSimulating(true);
    setError(null);
    try {
      const result = await simulateDecision(questionInput.trim());
      setDecision(result);
    } catch (err) {
      setError(String(err));
    } finally {
      setIsSimulating(false);
    }
  };

  const handleDialogue = async () => {
    if (!topicInput.trim()) return;
    setIsDebating(true);
    setError(null);
    try {
      const result = await runDialogue(topicInput.trim());
      setDialogue(result);
    } catch (err) {
      setError(String(err));
    } finally {
      setIsDebating(false);
    }
  };

  const tabs = [
    { id: "fingerprint" as const, label: "Fingerprint" },
    { id: "decision" as const, label: "Decisions" },
    { id: "dialogue" as const, label: "Dialogue" },
  ];

  return (
    <div className="p-4 flex flex-col h-full">
      <h2 className="text-lg font-semibold mb-1 flex items-center gap-2 text-brain-text">
        <svg className="w-5 h-5 text-violet-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z" />
        </svg>
        Digital Twin
      </h2>
      <p className="text-[10px] text-brain-muted mb-3">Cognitive fingerprint, decision simulation, internal dialogue</p>

      {error && (
        <div className="mb-3 text-xs font-mono px-3 py-2 rounded-lg bg-red-500/10 text-red-400 border border-red-500/20">
          {error}
          <button onClick={() => setError(null)} className="ml-2 underline">dismiss</button>
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
                ? "bg-violet-500/20 text-violet-400 border border-violet-500/30"
                : "text-brain-muted hover:text-brain-text border border-brain-border/30"
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>

      <div className="flex-1 overflow-y-auto space-y-3 min-h-0">
        {isLoading && (
          <div className="text-center text-brain-muted text-sm font-mono py-4 animate-pulse">
            Loading twin...
          </div>
        )}

        {/* Fingerprint Tab */}
        {activeTab === "fingerprint" && (
          <>
            {fingerprint && (
              <div className="space-y-2">
                {FINGERPRINT_DIMS.map((dim) => {
                  const value = fingerprint[dim.key];
                  if (typeof value !== "number") return null;
                  return (
                    <div key={dim.key} className="flex items-center gap-2 text-[11px] font-mono">
                      <span className="text-brain-muted w-28 truncate">{dim.label}</span>
                      <div className="flex-1 h-2 bg-brain-border/20 rounded-full overflow-hidden">
                        <div
                          className={`h-full rounded-full ${dim.color} transition-all duration-500`}
                          style={{ width: `${Math.min(100, value * 100)}%` }}
                        />
                      </div>
                      <span className="text-brain-text w-10 text-right">
                        {(value * 100).toFixed(0)}%
                      </span>
                    </div>
                  );
                })}
                <div className="text-[10px] text-brain-muted/50 text-center mt-2 font-mono">
                  Synthesized: {new Date(fingerprint.synthesized_at).toLocaleString()}
                </div>
              </div>
            )}

            {!fingerprint && !isLoading && (
              <div className="text-center text-brain-muted text-xs py-4">
                No cognitive fingerprint yet. Click below to synthesize.
              </div>
            )}

            <button
              onClick={handleSynthesize}
              disabled={isSynthesizing}
              className="w-full py-2 rounded-lg bg-violet-500/10 text-violet-400 text-xs font-mono hover:bg-violet-500/20 transition-colors border border-violet-500/20 disabled:opacity-50"
            >
              {isSynthesizing ? "Synthesizing..." : "Synthesize Fingerprint"}
            </button>
          </>
        )}

        {/* Decision Simulator Tab */}
        {activeTab === "decision" && (
          <>
            <div className="flex gap-2">
              <input
                type="text"
                value={questionInput}
                onChange={(e) => setQuestionInput(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && handleSimulate()}
                placeholder="Ask a decision question..."
                className="flex-1 bg-brain-bg/50 border border-brain-border/30 rounded-lg px-3 py-2 text-xs text-brain-text placeholder-brain-muted/50 focus:outline-none focus:border-violet-500/50"
              />
              <button
                onClick={handleSimulate}
                disabled={isSimulating || !questionInput.trim()}
                className="px-3 py-2 rounded-lg bg-brain-accent/20 hover:bg-brain-accent/30 text-brain-accent border border-brain-accent/30 text-xs font-mono disabled:opacity-50 transition-colors"
              >
                {isSimulating ? "..." : "Simulate"}
              </button>
            </div>

            {isSimulating && (
              <div className="text-center text-brain-muted text-xs font-mono py-4 animate-pulse">
                Simulating decision...
              </div>
            )}

            {decision && !isSimulating && (
              <div className="space-y-3">
                <div className="bg-brain-bg/50 border border-brain-border/30 rounded-lg p-3">
                  <div className="text-[10px] text-brain-muted uppercase tracking-wider mb-1">Prediction</div>
                  <div className="text-xs text-brain-text">{decision.prediction}</div>
                </div>

                <div className="flex gap-2">
                  <div className="flex-1 bg-brain-bg/50 border border-brain-border/30 rounded-lg p-3 text-center">
                    <div className={`text-lg font-mono font-bold ${
                      decision.confidence > 0.7 ? "text-green-400" :
                      decision.confidence > 0.4 ? "text-amber-400" : "text-red-400"
                    }`}>
                      {(decision.confidence * 100).toFixed(0)}%
                    </div>
                    <div className="text-[10px] text-brain-muted uppercase tracking-wider">Confidence</div>
                  </div>
                </div>

                <div className="bg-brain-bg/50 border border-brain-border/30 rounded-lg p-3">
                  <div className="text-[10px] text-brain-muted uppercase tracking-wider mb-1">Reasoning</div>
                  <div className="text-[11px] text-brain-muted">{decision.reasoning}</div>
                </div>

                {decision.alternatives.length > 0 && (
                  <div className="bg-brain-bg/50 border border-brain-border/30 rounded-lg p-3">
                    <div className="text-[10px] text-brain-muted uppercase tracking-wider mb-1">Alternatives</div>
                    {decision.alternatives.map((alt, i) => (
                      <div key={i} className="text-[11px] text-brain-muted/80 flex items-start gap-1.5 mt-1">
                        <span className="text-violet-400/60">-</span>
                        <span>{alt}</span>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            )}
          </>
        )}

        {/* Internal Dialogue Tab */}
        {activeTab === "dialogue" && (
          <>
            <div className="flex gap-2">
              <input
                type="text"
                value={topicInput}
                onChange={(e) => setTopicInput(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && handleDialogue()}
                placeholder="Enter a topic to debate..."
                className="flex-1 bg-brain-bg/50 border border-brain-border/30 rounded-lg px-3 py-2 text-xs text-brain-text placeholder-brain-muted/50 focus:outline-none focus:border-violet-500/50"
              />
              <button
                onClick={handleDialogue}
                disabled={isDebating || !topicInput.trim()}
                className="px-3 py-2 rounded-lg bg-brain-accent/20 hover:bg-brain-accent/30 text-brain-accent border border-brain-accent/30 text-xs font-mono disabled:opacity-50 transition-colors"
              >
                {isDebating ? "..." : "Debate"}
              </button>
            </div>

            {isDebating && (
              <div className="text-center text-brain-muted text-xs font-mono py-4 animate-pulse">
                Running internal dialogue...
              </div>
            )}

            {dialogue && !isDebating && (
              <div className="space-y-2">
                {dialogue.turns.map((turn, i) => (
                  <div
                    key={i}
                    className={`text-[11px] font-mono px-3 py-2 rounded-lg border ${
                      ROLE_COLORS[turn.role] || "text-brain-muted border-brain-border/30 bg-brain-bg/30"
                    }`}
                  >
                    <div className="font-semibold text-[10px] uppercase tracking-wider mb-0.5">
                      {turn.role}
                    </div>
                    <div className="opacity-90">{turn.content}</div>
                  </div>
                ))}

                {dialogue.synthesis && (
                  <div className="bg-violet-500/10 border border-violet-500/30 rounded-lg p-3 mt-2">
                    <div className="text-[10px] text-violet-400 uppercase tracking-wider mb-1 font-semibold">
                      Synthesis
                    </div>
                    <div className="text-[11px] text-brain-text">{dialogue.synthesis}</div>
                  </div>
                )}
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
