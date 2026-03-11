import { useMemo, useState } from "react";
import type { Connection, Process } from "../types";

type SortKey = "from" | "to" | "protocol" | "bytes_per_sec";
type SortDir = "asc" | "desc";
type ProtocolFilter = "all" | "tcp" | "udp" | "http";

interface NetworkTableProps {
  connections: Connection[];
  processes: Process[];
  highlightedPids: Set<number>;
  onSelectConnection: (fromPid: number, toPid: number) => void;
}

const columns: { key: SortKey; label: string; width: string }[] = [
  { key: "from", label: "Source", width: "28%" },
  { key: "to", label: "Destination", width: "28%" },
  { key: "protocol", label: "Protocol", width: "16%" },
  { key: "bytes_per_sec", label: "Bytes/sec", width: "28%" },
];

const protocolColors: Record<string, string> = {
  tcp: "#4488ff",
  udp: "#44ff88",
  http: "#44ffff",
};

function protocolBadge(protocol: string): React.CSSProperties {
  const color = protocolColors[protocol.toLowerCase()] ?? "#9ca3af";
  return {
    display: "inline-block",
    padding: "2px 8px",
    borderRadius: 4,
    fontSize: 11,
    fontWeight: 600,
    letterSpacing: 0.5,
    background: `${color}18`,
    color,
    textTransform: "uppercase",
  };
}

function formatBandwidth(bytesPerSec: number): string {
  if (bytesPerSec < 1024) return `${bytesPerSec.toFixed(0)} B/s`;
  if (bytesPerSec < 1024 * 1024) return `${(bytesPerSec / 1024).toFixed(1)} KB/s`;
  if (bytesPerSec < 1024 * 1024 * 1024) return `${(bytesPerSec / (1024 * 1024)).toFixed(1)} MB/s`;
  return `${(bytesPerSec / (1024 * 1024 * 1024)).toFixed(2)} GB/s`;
}

function BandwidthBar({ value, max }: { value: number; max: number }) {
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
            background: "#a78bfa",
            transition: "width 0.3s ease",
          }}
        />
      </div>
      <span style={{ minWidth: 72, textAlign: "right", fontSize: 12 }}>
        {formatBandwidth(value)}
      </span>
    </div>
  );
}

export function NetworkTable({ connections, processes, highlightedPids, onSelectConnection }: NetworkTableProps) {
  const [sortKey, setSortKey] = useState<SortKey>("bytes_per_sec");
  const [sortDir, setSortDir] = useState<SortDir>("desc");
  const [protocolFilter, setProtocolFilter] = useState<ProtocolFilter>("all");
  const [hoveredIdx, setHoveredIdx] = useState<number | null>(null);

  const processMap = useMemo(() => {
    const map = new Map<number, string>();
    for (const p of processes) {
      map.set(p.pid, p.name);
    }
    return map;
  }, [processes]);

  const getName = (pid: number) => processMap.get(pid) ?? "unknown";

  const maxBandwidth = useMemo(
    () => Math.max(1, ...connections.map((c) => c.bytes_per_sec)),
    [connections],
  );

  const sorted = useMemo(() => {
    const filtered = protocolFilter === "all"
      ? connections
      : connections.filter((c) => c.protocol.toLowerCase() === protocolFilter);

    return [...filtered].sort((a, b) => {
      let cmp: number;
      switch (sortKey) {
        case "from":
          cmp = getName(a.from_pid).localeCompare(getName(b.from_pid));
          break;
        case "to":
          cmp = getName(a.to_pid).localeCompare(getName(b.to_pid));
          break;
        case "protocol":
          cmp = a.protocol.localeCompare(b.protocol);
          break;
        case "bytes_per_sec":
          cmp = a.bytes_per_sec - b.bytes_per_sec;
          break;
        default:
          cmp = 0;
      }
      return sortDir === "asc" ? cmp : -cmp;
    });
  }, [connections, sortKey, sortDir, protocolFilter, processMap]);

  function handleSort(key: SortKey) {
    if (sortKey === key) {
      setSortDir((d) => (d === "asc" ? "desc" : "asc"));
    } else {
      setSortKey(key);
      setSortDir(key === "from" || key === "to" || key === "protocol" ? "asc" : "desc");
    }
  }

  const filterButtons: { key: ProtocolFilter; label: string }[] = [
    { key: "all", label: "All" },
    { key: "tcp", label: "TCP" },
    { key: "udp", label: "UDP" },
    { key: "http", label: "HTTP" },
  ];

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%", overflow: "hidden" }}>
      <div style={{ padding: "8px 12px", borderBottom: "1px solid #1e1e3a", display: "flex", gap: 6 }}>
        {filterButtons.map((btn) => {
          const isActive = protocolFilter === btn.key;
          const color = btn.key === "all" ? "#a78bfa" : (protocolColors[btn.key] ?? "#9ca3af");
          return (
            <button
              key={btn.key}
              onClick={() => setProtocolFilter(btn.key)}
              style={{
                padding: "4px 12px",
                borderRadius: 6,
                border: `1px solid ${isActive ? color : "#2a2a4a"}`,
                background: isActive ? `${color}18` : "transparent",
                color: isActive ? color : "#6b728a",
                fontSize: 12,
                fontWeight: 600,
                fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
                cursor: "pointer",
                letterSpacing: 0.5,
                transition: "all 0.15s",
              }}
            >
              {btn.label}
            </button>
          );
        })}
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
            {sorted.map((conn, idx) => {
              const isHighlighted = highlightedPids.has(conn.from_pid) && highlightedPids.has(conn.to_pid);
              const isHovered = hoveredIdx === idx;
              return (
                <tr
                  key={`${conn.from_pid}-${conn.to_pid}-${conn.protocol}`}
                  onClick={() => onSelectConnection(conn.from_pid, conn.to_pid)}
                  onMouseEnter={() => setHoveredIdx(idx)}
                  onMouseLeave={() => setHoveredIdx(null)}
                  style={{
                    cursor: "pointer",
                    background: isHighlighted
                      ? "#a78bfa12"
                      : isHovered
                        ? "#ffffff06"
                        : "transparent",
                    borderLeft: isHighlighted ? "2px solid #a78bfa" : "2px solid transparent",
                    transition: "background 0.15s",
                  }}
                >
                  <td style={cellStyle}>
                    <span style={{ color: "#e0e0e0" }}>{getName(conn.from_pid)}</span>
                    <span style={{ color: "#6b728a", fontSize: 11, marginLeft: 6 }}>:{conn.from_pid}</span>
                  </td>
                  <td style={cellStyle}>
                    <span style={{ color: "#e0e0e0" }}>{getName(conn.to_pid)}</span>
                    <span style={{ color: "#6b728a", fontSize: 11, marginLeft: 6 }}>:{conn.to_pid}</span>
                  </td>
                  <td style={cellStyle}>
                    <span style={protocolBadge(conn.protocol)}>{conn.protocol}</span>
                  </td>
                  <td style={cellStyle}>
                    <BandwidthBar value={conn.bytes_per_sec} max={maxBandwidth} />
                  </td>
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
            {protocolFilter !== "all" ? `No ${protocolFilter.toUpperCase()} connections` : "No active connections"}
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
