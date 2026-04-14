import { useState } from "react";
import { askBrain, BrainAnswer, getDomainColor } from "@/lib/tauri";

interface Message {
  role: "user" | "brain";
  content: string;
  sources?: { id: string; title: string; domain: string }[];
  confidence?: number;
}

export function AskBrainPanel() {
  const [question, setQuestion] = useState("");
  const [messages, setMessages] = useState<Message[]>([]);
  const [isThinking, setIsThinking] = useState(false);

  const handleAsk = async () => {
    if (!question.trim() || isThinking) return;
    const q = question.trim();
    setQuestion("");
    setMessages((prev) => [...prev, { role: "user", content: q }]);
    setIsThinking(true);

    try {
      const answer: BrainAnswer = await askBrain(q);
      setMessages((prev) => [
        ...prev,
        {
          role: "brain",
          content: answer.answer,
          sources: answer.sources.map((s) => ({ id: s.id, title: s.title, domain: s.domain })),
          confidence: answer.confidence,
        },
      ]);
    } catch (err) {
      setMessages((prev) => [
        ...prev,
        { role: "brain", content: `Error: ${err}. Make sure Ollama is running or your LLM API key is configured.` },
      ]);
    } finally {
      setIsThinking(false);
    }
  };

  return (
    <div className="p-4 flex flex-col h-full">
      <h2 className="text-lg font-semibold mb-4 flex items-center gap-2 text-brain-text">
        <svg className="w-5 h-5 text-brain-accent" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 10h.01M12 10h.01M16 10h.01M9 16H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-5l-5 5v-5z" />
        </svg>
        Ask the Brain
      </h2>

      {/* Messages */}
      <div className="flex-1 overflow-y-auto space-y-3 mb-4 min-h-0">
        {messages.length === 0 && (
          <div className="text-center text-brain-muted/50 text-sm font-mono py-8">
            Ask anything about your knowledge...
          </div>
        )}
        {messages.map((msg, i) => (
          <div key={i} className={`flex ${msg.role === "user" ? "justify-end" : "justify-start"}`}>
            <div className={`max-w-[90%] rounded-lg px-3 py-2 text-sm font-mono ${
              msg.role === "user"
                ? "bg-brain-accent/20 text-brain-accent border border-brain-accent/20"
                : "bg-brain-panel text-brain-text/80 border border-brain-border/30"
            }`}>
              <div className="whitespace-pre-wrap">{msg.content}</div>
              {msg.sources && msg.sources.length > 0 && (
                <div className="mt-2 pt-2 border-t border-brain-border/20">
                  <div className="text-[10px] text-brain-muted uppercase mb-1">Sources:</div>
                  {msg.sources.map((s, j) => (
                    <div key={j} className="flex items-center gap-1 text-[10px] text-brain-muted">
                      <div className="w-1.5 h-1.5 rounded-full" style={{ backgroundColor: getDomainColor(s.domain) }} />
                      {s.title}
                    </div>
                  ))}
                </div>
              )}
              {msg.confidence !== undefined && (
                <div className="mt-1 text-[10px] text-brain-muted/50">
                  Confidence: {Math.round(msg.confidence * 100)}%
                </div>
              )}
            </div>
          </div>
        ))}
        {isThinking && (
          <div className="flex justify-start">
            <div className="bg-brain-panel text-brain-accent/60 rounded-lg px-3 py-2 text-sm font-mono border border-brain-border/30 animate-pulse">
              Thinking...
            </div>
          </div>
        )}
      </div>

      {/* Input */}
      <div className="flex gap-2">
        <input
          value={question}
          onChange={(e) => setQuestion(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleAsk()}
          placeholder="Ask a question..."
          className="flex-1 bg-brain-bg/50 border border-brain-border/50 rounded-lg px-3 py-2 text-sm font-mono text-brain-text placeholder-brain-muted outline-none focus:border-brain-accent/50"
          disabled={isThinking}
        />
        <button
          onClick={handleAsk}
          disabled={isThinking || !question.trim()}
          className="px-4 py-2 rounded-lg bg-brain-accent/20 text-brain-accent text-sm font-mono hover:bg-brain-accent/30 transition-colors disabled:opacity-50 border border-brain-accent/20"
        >
          Ask
        </button>
      </div>
    </div>
  );
}
