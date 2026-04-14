import { useState } from "react";
import { useGraphStore } from "@/stores/graphStore";
import { researchTopic, researchBatch } from "@/lib/tauri";

const TOPIC_CATEGORIES: { label: string; color: string; topics: string[] }[] = [
  {
    label: "Core Stack",
    color: "text-blue-400",
    topics: [
      "React", "TypeScript", "Rust", "Python", "Tauri", "Three.js",
      "SurrealDB", "Docker", "Kubernetes", "GraphQL", "WebAssembly",
      "FastAPI", "Next.js", "PostgreSQL", "Redis", "Kafka",
      "TailwindCSS", "Vite", "Node.js", "Go", "Flutter",
    ],
  },
  {
    label: "AI & ML",
    color: "text-purple-400",
    topics: [
      "Machine Learning", "Vector Database", "RAG", "LangChain",
      "Anthropic Claude API", "OpenAI API", "Ollama", "LLM Fine-Tuning",
      "Hugging Face Transformers", "AI Agents", "Semantic Search",
    ],
  },
  {
    label: "Architecture & Patterns",
    color: "text-cyan-400",
    topics: [
      "Microservices", "Event Sourcing", "CQRS", "DDD",
      "Clean Architecture", "Hexagonal Architecture", "SAGA Pattern",
      "API Gateway Pattern", "Circuit Breaker Pattern",
    ],
  },
  {
    label: "Auth & Security",
    color: "text-red-400",
    topics: [
      "OAuth", "JWT", "WebSocket", "gRPC", "Elasticsearch",
      "Keycloak", "Auth0", "RBAC", "Multi-Tenancy", "Zero Trust Security",
      "OWASP Security", "Rate Limiting", "API Key Management",
    ],
  },
  {
    label: "Payments & E-Commerce",
    color: "text-green-400",
    topics: [
      "Stripe API", "PayFast", "Payment Gateway Integration",
      "Subscription Billing", "PCI DSS Compliance",
      "Shopping Cart Architecture", "Inventory Management Systems",
      "Product Catalog Design", "Order Management Systems",
      "Shopify Architecture", "E-Commerce Platform Design",
    ],
  },
  {
    label: "Data & Analytics",
    color: "text-amber-400",
    topics: [
      "ClickHouse", "Apache Spark", "dbt", "Data Warehousing",
      "Real-Time Analytics", "Time Series Database", "Apache Flink",
      "ETL Pipelines", "Data Lake Architecture",
    ],
  },
  {
    label: "DevOps & Infrastructure",
    color: "text-orange-400",
    topics: [
      "Terraform", "Ansible", "Helm Charts", "GitHub Actions",
      "Nginx", "Cloudflare", "AWS", "Azure", "GCP",
      "Prometheus", "Grafana", "Sentry", "OpenTelemetry",
      "CI/CD Pipelines", "Infrastructure as Code", "Service Mesh",
    ],
  },
  {
    label: "Messaging & Queues",
    color: "text-teal-400",
    topics: [
      "RabbitMQ", "NATS", "Apache Pulsar", "Event-Driven Architecture",
      "Message Queue Patterns", "Pub/Sub Systems",
    ],
  },
  {
    label: "Frontend & Mobile",
    color: "text-pink-400",
    topics: [
      "Zustand", "React Query", "Framer Motion", "React Native",
      "Capacitor", "Progressive Web Apps", "Web Workers",
      "Server-Side Rendering", "Static Site Generation",
    ],
  },
  {
    label: "Testing & Quality",
    color: "text-lime-400",
    topics: [
      "Playwright", "Vitest", "Cypress", "Load Testing",
      "Contract Testing", "Chaos Engineering",
    ],
  },
  {
    label: "APIs & Integration",
    color: "text-indigo-400",
    topics: [
      "tRPC", "REST API Design", "OpenAPI Specification",
      "Webhook Architecture", "API Versioning", "GraphQL Federation",
    ],
  },
  {
    label: "Database & Storage",
    color: "text-yellow-400",
    topics: [
      "MongoDB", "CockroachDB", "ScyllaDB", "MinIO",
      "Database Sharding", "Read Replicas", "Connection Pooling",
    ],
  },
  {
    label: "SaaS & Business",
    color: "text-emerald-400",
    topics: [
      "Multi-Tenant SaaS Architecture", "Feature Flags",
      "A/B Testing", "Usage-Based Pricing", "SaaS Metrics",
      "Customer Onboarding Flows", "Internationalization i18n",
      "White-Label Architecture", "Plugin System Design",
    ],
  },
];

const ALL_TOPICS = TOPIC_CATEGORIES.flatMap((c) => c.topics);

export function ResearchPanel() {
  const [topic, setTopic] = useState("");
  const [isResearching, setIsResearching] = useState(false);
  const [results, setResults] = useState<{ topic: string; count: number }[]>([]);
  const [status, setStatus] = useState("");
  const [expandedCategories, setExpandedCategories] = useState<Set<string>>(new Set());
  const { addNodes } = useGraphStore();

  const toggleCategory = (label: string) => {
    setExpandedCategories((prev) => {
      const next = new Set(prev);
      if (next.has(label)) next.delete(label);
      else next.add(label);
      return next;
    });
  };

  const expandAll = () => setExpandedCategories(new Set(TOPIC_CATEGORIES.map((c) => c.label)));
  const collapseAll = () => setExpandedCategories(new Set());

  const handleResearch = async (topicToResearch: string) => {
    if (!topicToResearch.trim() || isResearching) return;
    setIsResearching(true);
    setStatus(`Researching ${topicToResearch}...`);

    try {
      const nodes = await researchTopic(topicToResearch);
      addNodes(nodes);
      setResults((prev) => [...prev, { topic: topicToResearch, count: nodes.length }]);
      setStatus(`Learned ${nodes.length} things about ${topicToResearch}`);
    } catch (err) {
      setStatus(`Error: ${err}`);
    } finally {
      setIsResearching(false);
    }
  };

  const handleBulkResearch = async (topics?: string[], label?: string) => {
    setIsResearching(true);
    const batch = topics || ALL_TOPICS.slice(0, 15);
    const name = label || "Top 15";
    setStatus(`Bulk researching ${name} (${batch.length} topics)...`);

    try {
      const nodes = await researchBatch(batch);
      addNodes(nodes);
      setStatus(`${name} complete: ${nodes.length} new neurons from ${batch.length} topics`);
      setResults((prev) => [
        ...prev,
        ...batch.map((t) => ({ topic: t, count: Math.ceil(nodes.length / batch.length) })),
      ]);
    } catch (err) {
      setStatus(`Error: ${err}`);
    } finally {
      setIsResearching(false);
    }
  };

  return (
    <div className="p-4 flex flex-col h-full">
      <h2 className="text-lg font-semibold mb-4 flex items-center gap-2">
        <svg className="w-5 h-5 text-brain-research" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z" />
        </svg>
        Research & Learn
      </h2>

      {/* Topic input */}
      <div className="mb-4">
        <div className="flex gap-2">
          <input
            type="text"
            value={topic}
            onChange={(e) => setTopic(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleResearch(topic)}
            placeholder="Enter topic to research..."
            className="flex-1 bg-brain-bg/50 border border-brain-border/50 rounded-lg px-3 py-2 text-sm font-mono text-brain-text placeholder-brain-muted outline-none focus:border-brain-research/50"
            disabled={isResearching}
          />
          <button
            onClick={() => handleResearch(topic)}
            disabled={isResearching || !topic.trim()}
            className="px-4 py-2 rounded-lg bg-brain-research/20 text-brain-research text-sm font-mono hover:bg-brain-research/30 transition-colors disabled:opacity-50 border border-brain-research/20"
          >
            {isResearching ? "..." : "Learn"}
          </button>
        </div>
      </div>

      {/* Status */}
      {status && (
        <div className="mb-3 text-xs font-mono px-3 py-2 rounded-lg bg-brain-research/10 text-brain-research border border-brain-research/20">
          {status}
        </div>
      )}

      {/* Bulk research buttons */}
      <div className="flex gap-2 mb-4">
        <button
          onClick={() => handleBulkResearch()}
          disabled={isResearching}
          className="flex-1 py-2 rounded-lg bg-gradient-to-r from-brain-research/20 to-brain-accent/20 text-brain-text text-xs font-mono hover:from-brain-research/30 hover:to-brain-accent/30 transition-all disabled:opacity-50 border border-brain-research/20"
        >
          {isResearching ? "..." : "Bulk Learn Top 15"}
        </button>
        <button
          onClick={() => handleBulkResearch(ALL_TOPICS, "ALL Topics")}
          disabled={isResearching}
          className="flex-1 py-2 rounded-lg bg-gradient-to-r from-purple-500/20 to-pink-500/20 text-brain-text text-xs font-mono hover:from-purple-500/30 hover:to-pink-500/30 transition-all disabled:opacity-50 border border-purple-500/20"
        >
          {isResearching ? "..." : `Learn ALL ${ALL_TOPICS.length} Topics`}
        </button>
      </div>

      {/* Quick topic buttons - categorized */}
      <div className="flex items-center justify-between mb-2">
        <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider">
          Quick Research ({ALL_TOPICS.length} topics)
        </h3>
        <div className="flex gap-1">
          <button onClick={expandAll} className="text-[10px] px-1.5 py-0.5 rounded text-brain-muted hover:text-brain-research transition-colors">
            expand all
          </button>
          <button onClick={collapseAll} className="text-[10px] px-1.5 py-0.5 rounded text-brain-muted hover:text-brain-research transition-colors">
            collapse
          </button>
        </div>
      </div>
      <div className="flex-1 overflow-y-auto space-y-1 mb-4 min-h-0">
        {TOPIC_CATEGORIES.map((cat) => (
          <div key={cat.label}>
            <button
              onClick={() => toggleCategory(cat.label)}
              className="w-full flex items-center gap-1.5 text-xs font-mono py-1.5 px-2 rounded-md hover:bg-brain-bg/50 transition-colors group"
            >
              <span className={`transition-transform text-[10px] ${expandedCategories.has(cat.label) ? "rotate-90" : ""}`}>
                &#9654;
              </span>
              <span className={cat.color}>{cat.label}</span>
              <span className="text-brain-muted/50 ml-auto">{cat.topics.length}</span>
            </button>
            {expandedCategories.has(cat.label) && (
              <div className="pl-4 pb-1.5">
                <div className="flex flex-wrap gap-1 mb-1.5">
                  {cat.topics.map((t) => (
                    <button
                      key={t}
                      onClick={() => handleResearch(t)}
                      disabled={isResearching}
                      className={`text-xs px-2 py-1 rounded-md bg-brain-bg/50 border border-brain-border/30 text-brain-muted hover:${cat.color} hover:border-brain-research/30 transition-all disabled:opacity-50 font-mono`}
                    >
                      {t}
                    </button>
                  ))}
                </div>
                <button
                  onClick={() => handleBulkResearch(cat.topics, cat.label)}
                  disabled={isResearching}
                  className={`text-[10px] px-2 py-0.5 rounded-md border border-brain-border/20 ${cat.color} opacity-60 hover:opacity-100 transition-all disabled:opacity-30 font-mono`}
                >
                  Learn all {cat.label}
                </button>
              </div>
            )}
          </div>
        ))}
      </div>

      {/* Results log */}
      {results.length > 0 && (
        <>
          <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">
            Research Log
          </h3>
          <div className="flex-1 overflow-y-auto space-y-1">
            {results.map((r, i) => (
              <div key={i} className="flex items-center justify-between text-xs font-mono px-2 py-1 rounded bg-brain-bg/30">
                <span className="text-brain-research">{r.topic}</span>
                <span className="text-brain-muted">+{r.count} neurons</span>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
