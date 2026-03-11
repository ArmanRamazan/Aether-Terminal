import { useEffect, useRef } from "react";
import type { Connection, Process } from "../types";
import { SparklineChart } from "./SparklineChart";

const MAX_HISTORY = 60;

interface ProcessDetailProps {
  process: Process;
  connections: Connection[];
  allProcesses: Process[];
  onClose: () => void;
}

function formatMemory(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function stateBadge(state: string): React.CSSProperties {
  const base: React.CSSProperties = {
    display: "inline-block",
    padding: "3px 10px",
    borderRadius: 4,
    fontSize: 12,
    fontWeight: 600,
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

function StatCard({ label, value, color }: { label: string; value: string; color?: string }) {
  return (
    <div
      style={{
        padding: "10px 12px",
        background: "#12121f",
        borderRadius: 6,
        border: "1px solid #1e1e3a",
      }}
    >
      <div style={{ fontSize: 10, color: "#6b728a", textTransform: "uppercase", letterSpacing: 1 }}>
        {label}
      </div>
      <div style={{ fontSize: 16, fontWeight: 700, color: color ?? "#e0e0e0", marginTop: 2 }}>
        {value}
      </div>
    </div>
  );
}

export function ProcessDetail({ process, connections, allProcesses, onClose }: ProcessDetailProps) {
  const cpuHistory = useRef<number[]>([]);
  const memHistory = useRef<number[]>([]);
  const lastPid = useRef<number>(process.pid);

  // Reset history when selecting a different process
  if (lastPid.current !== process.pid) {
    cpuHistory.current = [];
    memHistory.current = [];
    lastPid.current = process.pid;
  }

  useEffect(() => {
    cpuHistory.current = [...cpuHistory.current, process.cpu_percent].slice(-MAX_HISTORY);
    memHistory.current = [...memHistory.current, process.mem_bytes].slice(-MAX_HISTORY);
  });

  const relatedConns = connections.filter(
    (c) => c.from_pid === process.pid || c.to_pid === process.pid,
  );

  const hpColor =
    process.hp >= 70 ? "#4ade80" : process.hp >= 30 ? "#facc15" : "#f87171";

  return (
    <div
      style={{
        height: "100%",
        display: "flex",
        flexDirection: "column",
        fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
        overflow: "auto",
      }}
    >
      {/* Header */}
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "flex-start",
          padding: "16px 16px 12px",
          borderBottom: "1px solid #1e1e3a",
        }}
      >
        <div>
          <div style={{ fontSize: 18, fontWeight: 700, color: "#e0e0e0" }}>
            {process.name}
          </div>
          <div style={{ fontSize: 12, color: "#6b728a", marginTop: 2 }}>
            PID {process.pid}
          </div>
        </div>
        <button
          onClick={onClose}
          style={{
            background: "transparent",
            border: "1px solid #2a2a4a",
            borderRadius: 4,
            color: "#8b8baf",
            cursor: "pointer",
            fontSize: 14,
            padding: "2px 8px",
            lineHeight: 1,
          }}
        >
          ✕
        </button>
      </div>

      {/* Stats grid */}
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "1fr 1fr",
          gap: 8,
          padding: "12px 16px",
        }}
      >
        <StatCard label="CPU" value={`${process.cpu_percent.toFixed(1)}%`} />
        <StatCard label="Memory" value={formatMemory(process.mem_bytes)} />
        <StatCard label="HP" value={`${process.hp.toFixed(0)}`} color={hpColor} />
        <StatCard label="XP" value={`${process.xp.toFixed(0)}`} color="#a78bfa" />
        <StatCard label="State" value={process.state} />
        <StatCard label="PPID" value={`${process.ppid}`} />
      </div>

      {/* Sparklines */}
      <div style={{ padding: "4px 16px 12px" }}>
        <div style={{ marginBottom: 12 }}>
          <div style={sparkLabelStyle}>CPU History</div>
          <SparklineChart
            data={cpuHistory.current}
            color="#4ade80"
            label={`cpu-${process.pid}`}
            width={260}
            height={50}
          />
        </div>
        <div>
          <div style={sparkLabelStyle}>Memory History</div>
          <SparklineChart
            data={memHistory.current}
            color="#60a5fa"
            label={`mem-${process.pid}`}
            width={260}
            height={50}
          />
        </div>
      </div>

      {/* Connections */}
      {relatedConns.length > 0 && (
        <div style={{ padding: "0 16px 16px" }}>
          <div style={{ ...sparkLabelStyle, marginBottom: 6 }}>
            Connections ({relatedConns.length})
          </div>
          {relatedConns.map((c, i) => {
            const otherPid = c.from_pid === process.pid ? c.to_pid : c.from_pid;
            const otherName = allProcesses.find((p) => p.pid === otherPid)?.name ?? "unknown";
            return (
              <div
                key={i}
                style={{
                  padding: "6px 10px",
                  background: "#12121f",
                  borderRadius: 4,
                  marginBottom: 4,
                  fontSize: 12,
                  color: "#c8c8d8",
                  display: "flex",
                  justifyContent: "space-between",
                }}
              >
                <span>
                  {otherName}{" "}
                  <span style={{ color: "#6b728a" }}>:{otherPid}</span>
                </span>
                <span style={{ color: "#6b728a" }}>{c.protocol}</span>
              </div>
            );
          })}
        </div>
      )}

      {/* State badge at bottom */}
      <div style={{ padding: "0 16px 16px" }}>
        <span style={stateBadge(process.state)}>{process.state}</span>
      </div>
    </div>
  );
}

const sparkLabelStyle: React.CSSProperties = {
  fontSize: 10,
  color: "#6b728a",
  textTransform: "uppercase",
  letterSpacing: 1,
  marginBottom: 4,
};
