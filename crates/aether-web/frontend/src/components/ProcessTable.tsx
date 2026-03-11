import { useMemo, useState } from "react";
import type { Process } from "../types";

type SortKey = "pid" | "name" | "cpu_percent" | "mem_bytes" | "state" | "hp" | "xp";
type SortDir = "asc" | "desc";

interface ProcessTableProps {
  processes: Process[];
  selectedPid: number | null;
  onSelect: (pid: number) => void;
}

const columns: { key: SortKey; label: string; width: string }[] = [
  { key: "pid", label: "PID", width: "8%" },
  { key: "name", label: "Name", width: "22%" },
  { key: "cpu_percent", label: "CPU%", width: "16%" },
  { key: "mem_bytes", label: "Memory", width: "16%" },
  { key: "state", label: "State", width: "12%" },
  { key: "hp", label: "HP", width: "13%" },
  { key: "xp", label: "XP", width: "13%" },
];

function stateBadge(state: string): React.CSSProperties {
  const base: React.CSSProperties = {
    display: "inline-block",
    padding: "2px 8px",
    borderRadius: 4,
    fontSize: 11,
    fontWeight: 600,
    letterSpacing: 0.5,
  };
  switch (state.toLowerCase()) {
    case "running":
      return { ...base, background: "#16a34a22", color: "#4ade80" };
    case "sleeping":
    case "idle":
      return { ...base, background: "#6b728022", color: "#9ca3af" };
    case "zombie":
      return { ...base, background: "#dc262622", color: "#f87171" };
    case "stopped":
      return { ...base, background: "#ca8a0422", color: "#facc15" };
    default:
      return { ...base, background: "#6b728022", color: "#9ca3af" };
  }
}

function hpColor(hp: number): string {
  const t = Math.max(0, Math.min(100, hp)) / 100;
  const r = Math.round(255 * (1 - t));
  const g = Math.round(200 * t);
  return `rgb(${r}, ${g}, 60)`;
}

function formatMemory(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function InlineBar({ value, max, color }: { value: number; max: number; color: string }) {
  const pct = max > 0 ? Math.min((value / max) * 100, 100) : 0;
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
      <div
        style={{
          flex: 1,
          height: 6,
          borderRadius: 3,
          background: "#1a1a2e",
          overflow: "hidden",
        }}
      >
        <div
          style={{
            width: `${pct}%`,
            height: "100%",
            borderRadius: 3,
            background: color,
            transition: "width 0.3s ease",
          }}
        />
      </div>
      <span style={{ minWidth: 42, textAlign: "right", fontSize: 12 }}>
        {typeof value === "number" && max <= 100 ? `${value.toFixed(1)}%` : formatMemory(value)}
      </span>
    </div>
  );
}

export function ProcessTable({ processes, selectedPid, onSelect }: ProcessTableProps) {
  const [sortKey, setSortKey] = useState<SortKey>("cpu_percent");
  const [sortDir, setSortDir] = useState<SortDir>("desc");
  const [filter, setFilter] = useState("");
  const [hoveredPid, setHoveredPid] = useState<number | null>(null);

  const maxMem = useMemo(
    () => Math.max(1, ...processes.map((p) => p.mem_bytes)),
    [processes],
  );

  const sorted = useMemo(() => {
    const filtered = filter
      ? processes.filter(
          (p) =>
            p.name.toLowerCase().includes(filter.toLowerCase()) ||
            String(p.pid).includes(filter),
        )
      : processes;

    return [...filtered].sort((a, b) => {
      const av = a[sortKey];
      const bv = b[sortKey];
      const cmp = typeof av === "string" ? av.localeCompare(bv as string) : (av as number) - (bv as number);
      return sortDir === "asc" ? cmp : -cmp;
    });
  }, [processes, sortKey, sortDir, filter]);

  function handleSort(key: SortKey) {
    if (sortKey === key) {
      setSortDir((d) => (d === "asc" ? "desc" : "asc"));
    } else {
      setSortKey(key);
      setSortDir(key === "name" || key === "state" ? "asc" : "desc");
    }
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%", overflow: "hidden" }}>
      <div style={{ padding: "8px 12px", borderBottom: "1px solid #1e1e3a" }}>
        <input
          type="text"
          placeholder="Search processes..."
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          style={{
            width: "100%",
            padding: "6px 10px",
            background: "#12121f",
            border: "1px solid #2a2a4a",
            borderRadius: 6,
            color: "#e0e0e0",
            fontSize: 13,
            fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
            outline: "none",
          }}
        />
      </div>

      <div style={{ flex: 1, overflow: "auto" }}>
        <table
          style={{
            width: "100%",
            borderCollapse: "collapse",
            fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
            fontSize: 13,
          }}
        >
          <thead>
            <tr>
              {columns.map((col) => (
                <th
                  key={col.key}
                  onClick={() => handleSort(col.key)}
                  style={{
                    width: col.width,
                    padding: "8px 12px",
                    textAlign: "left",
                    color: sortKey === col.key ? "#a78bfa" : "#8b8baf",
                    cursor: "pointer",
                    userSelect: "none",
                    borderBottom: "1px solid #1e1e3a",
                    position: "sticky",
                    top: 0,
                    background: "#0d0d18",
                    fontSize: 11,
                    fontWeight: 700,
                    letterSpacing: 1,
                    textTransform: "uppercase",
                  }}
                >
                  {col.label}
                  {sortKey === col.key && (
                    <span style={{ marginLeft: 4, fontSize: 10 }}>
                      {sortDir === "asc" ? "▲" : "▼"}
                    </span>
                  )}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {sorted.map((proc) => {
              const isSelected = proc.pid === selectedPid;
              const isHovered = proc.pid === hoveredPid;
              return (
                <tr
                  key={proc.pid}
                  onClick={() => onSelect(proc.pid)}
                  onMouseEnter={() => setHoveredPid(proc.pid)}
                  onMouseLeave={() => setHoveredPid(null)}
                  style={{
                    cursor: "pointer",
                    background: isSelected
                      ? "#a78bfa12"
                      : isHovered
                        ? "#ffffff06"
                        : "transparent",
                    borderLeft: isSelected ? "2px solid #a78bfa" : "2px solid transparent",
                    transition: "background 0.15s",
                  }}
                >
                  <td style={cellStyle}>{proc.pid}</td>
                  <td
                    style={{
                      ...cellStyle,
                      maxWidth: 0,
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {proc.name}
                  </td>
                  <td style={cellStyle}>
                    <InlineBar
                      value={proc.cpu_percent}
                      max={100}
                      color={proc.cpu_percent > 80 ? "#f87171" : proc.cpu_percent > 50 ? "#facc15" : "#4ade80"}
                    />
                  </td>
                  <td style={cellStyle}>
                    <InlineBar value={proc.mem_bytes} max={maxMem} color="#60a5fa" />
                  </td>
                  <td style={cellStyle}>
                    <span style={stateBadge(proc.state)}>{proc.state}</span>
                  </td>
                  <td style={{ ...cellStyle, color: hpColor(proc.hp) }}>
                    {proc.hp.toFixed(0)}
                  </td>
                  <td style={cellStyle}>{proc.xp.toFixed(0)}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
        {sorted.length === 0 && (
          <div
            style={{
              padding: 24,
              textAlign: "center",
              color: "#6b728080",
              fontSize: 13,
            }}
          >
            {filter ? "No processes match filter" : "Waiting for data..."}
          </div>
        )}
      </div>
    </div>
  );
}

const cellStyle: React.CSSProperties = {
  padding: "6px 12px",
  borderBottom: "1px solid #1e1e3a08",
  color: "#c8c8d8",
};
