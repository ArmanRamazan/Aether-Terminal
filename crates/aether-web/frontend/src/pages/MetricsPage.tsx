import { useWorldStore } from "../stores/worldStore";
import { useMetricsStore } from "../stores/metricsStore";
import { MetricCard } from "../components/MetricCard";
import { SystemCharts } from "../components/SystemCharts";

type Trend = "up" | "down" | "stable";

function computeTrend(data: number[], threshold: number = 0.5): Trend {
  if (data.length < 2) return "stable";
  const recent = data[data.length - 1]!;
  const prev = data[data.length - 2]!;
  const diff = recent - prev;
  if (Math.abs(diff) < threshold) return "stable";
  return diff > 0 ? "up" : "down";
}

function formatMemory(bytes: number): string {
  if (bytes < 1024) return `${bytes}`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)}`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)}`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)}`;
}

function memoryUnit(bytes: number): string {
  if (bytes < 1024) return "B";
  if (bytes < 1024 * 1024) return "KB";
  if (bytes < 1024 * 1024 * 1024) return "MB";
  return "GB";
}

export function MetricsPage() {
  const stats = useWorldStore((s) => s.stats);
  const diagnosticStats = useWorldStore((s) => s.diagnosticStats);
  const cpuData = useMetricsStore((s) => s.history["cpu"] ?? []);
  const memData = useMetricsStore((s) => s.history["memory"] ?? []);
  const procData = useMetricsStore((s) => s.history["process_count"] ?? []);
  const hpData = useMetricsStore((s) => s.history["avg_hp"] ?? []);
  const critData = useMetricsStore((s) => s.history["diagnostics_critical"] ?? []);
  const warnData = useMetricsStore((s) => s.history["diagnostics_warning"] ?? []);

  const cpuValues = cpuData.map((s) => s.value);
  const memValues = memData.map((s) => s.value);
  const procValues = procData.map((s) => s.value);
  const hpValues = hpData.map((s) => s.value);
  const critValues = critData.map((s) => s.value);
  const warnValues = warnData.map((s) => s.value);


  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 20,
        height: "100%",
        overflow: "auto",
        fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
      }}
    >
      {/* Header */}
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <span style={{ fontSize: 16 }}>📊</span>
        <span
          style={{
            fontSize: 14,
            fontWeight: 700,
            color: "#e0e0e0",
            letterSpacing: 0.5,
          }}
        >
          Metrics
        </span>
      </div>

      {/* Metric cards grid */}
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fill, minmax(260px, 1fr))",
          gap: 14,
        }}
      >
        <MetricCard
          title="Total CPU"
          value={stats.total_cpu.toFixed(1)}
          unit="%"
          chartData={cpuValues}
          trend={computeTrend(cpuValues, 1)}
          color="#4ade80"
        />
        <MetricCard
          title="Memory Used"
          value={formatMemory(stats.total_memory)}
          unit={memoryUnit(stats.total_memory)}
          chartData={memValues}
          trend={computeTrend(memValues, 1024)}
          color="#60a5fa"
        />
        <MetricCard
          title="Processes"
          value={String(stats.process_count)}
          unit="active"
          chartData={procValues}
          trend={computeTrend(procValues, 1)}
          color="#a78bfa"
        />
        <MetricCard
          title="Avg HP"
          value={stats.avg_hp.toFixed(1)}
          unit="HP"
          chartData={hpValues}
          trend={computeTrend(hpValues, 0.5)}
          color="#22c55e"
        />
        <MetricCard
          title="Critical Issues"
          value={String(diagnosticStats.critical)}
          unit="active"
          chartData={critValues}
          trend={computeTrend(critValues, 0.5)}
          color="#ff3c3c"
        />
        <MetricCard
          title="Warnings"
          value={String(diagnosticStats.warning)}
          unit="active"
          chartData={warnValues}
          trend={computeTrend(warnValues, 0.5)}
          color="#ffc832"
        />
      </div>

      {/* Full-width charts */}
      <SystemCharts />
    </div>
  );
}
