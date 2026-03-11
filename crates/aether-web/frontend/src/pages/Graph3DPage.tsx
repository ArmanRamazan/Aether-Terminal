import { Graph3D } from "../components/Graph3D";
import { useWorldStore } from "../stores/worldStore";

function SelectedProcessInfo() {
  const selectedPid = useWorldStore((s) => s.selectedPid);
  const processes = useWorldStore((s) => s.processes);
  const clearSelection = useWorldStore((s) => s.clearSelection);

  if (selectedPid === null) return null;
  const proc = processes.find((p) => p.pid === selectedPid);
  if (!proc) return null;

  return (
    <div
      style={{
        position: "absolute",
        top: 16,
        right: 16,
        background: "rgba(10, 10, 26, 0.9)",
        border: "1px solid rgba(167, 139, 250, 0.3)",
        borderRadius: 8,
        padding: 16,
        minWidth: 220,
        color: "#e2e8f0",
        fontSize: 13,
        backdropFilter: "blur(8px)",
      }}
    >
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginBottom: 12,
        }}
      >
        <span style={{ fontWeight: 700, fontSize: 15, color: "#a78bfa" }}>
          {proc.name}
        </span>
        <button
          onClick={clearSelection}
          style={{
            background: "none",
            border: "none",
            color: "#94a3b8",
            cursor: "pointer",
            fontSize: 18,
            lineHeight: 1,
            padding: "0 4px",
          }}
        >
          ×
        </button>
      </div>
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8 }}>
        <Stat label="PID" value={proc.pid} />
        <Stat label="PPID" value={proc.ppid} />
        <Stat label="CPU" value={`${proc.cpu_percent.toFixed(1)}%`} />
        <Stat label="Memory" value={formatBytes(proc.mem_bytes)} />
        <Stat label="HP" value={proc.hp} color={hpColor(proc.hp)} />
        <Stat label="XP" value={proc.xp} />
        <Stat label="State" value={proc.state} />
      </div>
    </div>
  );
}

function Stat({
  label,
  value,
  color,
}: {
  label: string;
  value: string | number;
  color?: string;
}) {
  return (
    <div>
      <div style={{ color: "#64748b", fontSize: 11, marginBottom: 2 }}>
        {label}
      </div>
      <div style={{ color: color ?? "#e2e8f0", fontWeight: 600 }}>{value}</div>
    </div>
  );
}

function Legend() {
  const items = [
    { color: "hsl(120, 80%, 50%)", label: "HP > 70 (healthy)" },
    { color: "hsl(60, 80%, 50%)", label: "HP 40–70 (warning)" },
    { color: "hsl(0, 80%, 50%)", label: "HP ≤ 40 (critical)" },
    { color: "#4488ff", label: "TCP connection" },
    { color: "#44ff88", label: "UDP connection" },
    { color: "#44ffff", label: "HTTP connection" },
  ];

  return (
    <div
      style={{
        position: "absolute",
        bottom: 16,
        left: 16,
        background: "rgba(10, 10, 26, 0.9)",
        border: "1px solid rgba(167, 139, 250, 0.2)",
        borderRadius: 8,
        padding: 12,
        color: "#94a3b8",
        fontSize: 12,
        backdropFilter: "blur(8px)",
      }}
    >
      {items.map((item) => (
        <div
          key={item.label}
          style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 4 }}
        >
          <div
            style={{
              width: 10,
              height: 10,
              borderRadius: "50%",
              background: item.color,
              boxShadow: `0 0 6px ${item.color}`,
            }}
          />
          <span>{item.label}</span>
        </div>
      ))}
    </div>
  );
}

function ControlsHint() {
  return (
    <div
      style={{
        position: "absolute",
        bottom: 16,
        right: 16,
        color: "#475569",
        fontSize: 11,
      }}
    >
      Drag: rotate · Scroll: zoom · Right-drag: pan
    </div>
  );
}

function hpColor(hp: number): string {
  if (hp > 70) return "#4ade80";
  if (hp > 40) return "#facc15";
  return "#f87171";
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

export function Graph3DPage() {
  return (
    <div style={{ position: "relative", width: "100%", height: "100%" }}>
      <Graph3D />
      <SelectedProcessInfo />
      <Legend />
      <ControlsHint />
    </div>
  );
}
