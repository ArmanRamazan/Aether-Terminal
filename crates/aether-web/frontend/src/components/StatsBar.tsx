import { useWorldStore } from "../stores/worldStore";

function cpuColor(percent: number): string {
  if (percent >= 80) return "#ef4444";
  if (percent >= 50) return "#eab308";
  return "#22c55e";
}

function memColor(used: number, total: number): string {
  if (total === 0) return "#22c55e";
  const ratio = used / total;
  if (ratio >= 0.8) return "#ef4444";
  if (ratio >= 0.5) return "#eab308";
  return "#22c55e";
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

export function StatsBar() {
  const stats = useWorldStore((s) => s.stats);

  return (
    <div
      style={{
        display: "flex",
        gap: "2rem",
        padding: "0.5rem 1.5rem",
        background: "#111118",
        borderBottom: "1px solid #1e1e2e",
        fontSize: "0.85rem",
        fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
      }}
    >
      <span>
        CPU{" "}
        <span style={{ color: cpuColor(stats.total_cpu) }}>
          {stats.total_cpu.toFixed(1)}%
        </span>
      </span>
      <span>
        MEM{" "}
        <span style={{ color: memColor(stats.total_memory, stats.total_memory) }}>
          {formatBytes(stats.total_memory)}
        </span>
      </span>
      <span>
        PID <span style={{ color: "#8b8baf" }}>{stats.process_count}</span>
      </span>
      <span>
        AVG HP{" "}
        <span style={{ color: cpuColor(100 - stats.avg_hp) }}>
          {stats.avg_hp.toFixed(1)}
        </span>
      </span>
    </div>
  );
}
