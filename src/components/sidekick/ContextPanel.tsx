import { useState, useEffect } from "react";
import {
  getUserProfile,
  analyzeTrends,
  getRecommendations,
  getBrainStats,
  type UserProfile,
  type TrendReport,
  type Recommendation,
  type BrainStats,
  getDomainColor,
} from "@/lib/tauri";

export function ContextPanel() {
  const [profile, setProfile] = useState<UserProfile | null>(null);
  const [trends, setTrends] = useState<TrendReport | null>(null);
  const [recommendations, setRecommendations] = useState<Recommendation[]>([]);
  const [stats, setStats] = useState<BrainStats | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    loadContext();
    const interval = setInterval(loadContext, 120_000); // Refresh every 2 min
    return () => clearInterval(interval);
  }, []);

  const loadContext = async () => {
    setLoading(true);
    try {
      const [p, t, r, s] = await Promise.all([
        getUserProfile().catch(() => null),
        analyzeTrends().catch(() => null),
        getRecommendations().catch(() => []),
        getBrainStats().catch(() => null),
      ]);
      if (p) setProfile(p);
      if (t) setTrends(t);
      if (r) setRecommendations(r as Recommendation[]);
      if (s) setStats(s);
    } catch {
      // Silent fail
    } finally {
      setLoading(false);
    }
  };

  if (loading && !profile) {
    return (
      <div className="p-4 text-center text-brain-muted text-sm">
        Loading brain context...
      </div>
    );
  }

  return (
    <div className="p-4 flex flex-col h-full overflow-y-auto">
      <h2 className="text-lg font-semibold mb-4 flex items-center gap-2 text-brain-text">
        <svg className="w-5 h-5 text-cyan-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
        </svg>
        Brain Sidekick
      </h2>

      {/* Brain IQ */}
      {trends && (
        <div className="glass-panel p-3 mb-4 bg-brain-bg/30 border border-brain-border/30 rounded-lg">
          <div className="flex items-center justify-between mb-2">
            <span className="text-xs font-mono text-brain-muted uppercase">Brain IQ</span>
            <span className="text-2xl font-bold text-brain-accent">{trends.brain_iq}</span>
          </div>
          <div className="h-1.5 bg-brain-bg/50 rounded-full overflow-hidden">
            <div
              className="h-full bg-brain-accent rounded-full transition-all duration-1000"
              style={{ width: `${Math.min(100, (trends.brain_iq / 200) * 100)}%` }}
            />
          </div>
        </div>
      )}

      {/* Knowledge Overview */}
      {stats && (
        <div className="grid grid-cols-2 gap-2 mb-4">
          <MiniStat label="Neurons" value={stats.total_nodes.toLocaleString()} color="#38BDF8" />
          <MiniStat label="Synapses" value={stats.total_edges.toLocaleString()} color="#8B5CF6" />
          <MiniStat label="Domains" value={stats.domains.length.toString()} color="#00CC88" />
          <MiniStat label="Sources" value={stats.total_sources.toLocaleString()} color="#F59E0B" />
        </div>
      )}

      {/* User Preferences */}
      {profile && (
        <Section title="Your Profile">
          {profile.primary_languages.length > 0 && (
            <InfoRow label="Languages" value={profile.primary_languages.slice(0, 5).join(", ")} />
          )}
          {profile.frameworks.length > 0 && (
            <InfoRow label="Frameworks" value={profile.frameworks.slice(0, 5).join(", ")} />
          )}
          {profile.coding_patterns.length > 0 && (
            <InfoRow label="Patterns" value={profile.coding_patterns.slice(0, 3).join(", ")} />
          )}
          <InfoRow label="Velocity" value={`${profile.learning_velocity.toFixed(1)} nodes/day`} />
        </Section>
      )}

      {/* Domain Breakdown */}
      {stats && stats.domains.length > 0 && (
        <Section title="Knowledge Domains">
          {stats.domains.map((d) => {
            const maxCount = Math.max(...stats.domains.map((x) => x.count), 1);
            const width = (d.count / maxCount) * 100;
            const color = getDomainColor(d.domain);
            return (
              <div key={d.domain} className="mb-2">
                <div className="flex justify-between text-xs font-mono mb-0.5">
                  <span style={{ color }}>{d.domain}</span>
                  <span className="text-brain-muted">{d.count.toLocaleString()}</span>
                </div>
                <div className="h-1 bg-brain-bg/50 rounded-full overflow-hidden">
                  <div
                    className="h-full rounded-full transition-all duration-500"
                    style={{ width: `${width}%`, backgroundColor: color }}
                  />
                </div>
              </div>
            );
          })}
        </Section>
      )}

      {/* Hot Topics */}
      {trends && trends.hot_topics.length > 0 && (
        <Section title="Recent Focus">
          <div className="flex flex-wrap gap-1.5">
            {trends.hot_topics.slice(0, 8).map((t) => (
              <span
                key={t.topic}
                className="text-[10px] font-mono px-2 py-0.5 rounded-full bg-brain-accent/10 text-brain-accent border border-brain-accent/20"
              >
                {t.topic}
              </span>
            ))}
          </div>
        </Section>
      )}

      {/* Recommendations */}
      {recommendations.length > 0 && (
        <Section title="Suggestions">
          {recommendations.slice(0, 3).map((rec, i) => (
            <div
              key={i}
              className="p-2 mb-2 rounded-lg bg-brain-bg/30 border border-brain-border/20"
            >
              <div className="flex items-center gap-2 mb-1">
                <span className="text-[9px] font-mono px-1 py-0.5 rounded bg-brain-accent/20 text-brain-accent uppercase">
                  {rec.rec_type}
                </span>
                <span className="text-xs font-mono text-brain-text truncate">{rec.title}</span>
              </div>
              <p className="text-[10px] text-brain-muted">{rec.description}</p>
            </div>
          ))}
        </Section>
      )}
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="mb-4">
      <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">{title}</h3>
      {children}
    </div>
  );
}

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between text-xs font-mono py-0.5">
      <span className="text-brain-muted">{label}</span>
      <span className="text-brain-text">{value}</span>
    </div>
  );
}

function MiniStat({ label, value, color }: { label: string; value: string; color: string }) {
  return (
    <div className="p-2 rounded-lg bg-brain-bg/30 border border-brain-border/30 text-center">
      <div className="text-lg font-bold font-mono" style={{ color }}>{value}</div>
      <div className="text-[10px] text-brain-muted uppercase">{label}</div>
    </div>
  );
}
