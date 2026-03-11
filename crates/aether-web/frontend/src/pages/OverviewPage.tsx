import { useWorldStore } from "../stores/worldStore";
import { ProcessDetail } from "../components/ProcessDetail";
import { ProcessTable } from "../components/ProcessTable";
import { SparklineChart } from "../components/SparklineChart";
import { useEffect, useRef } from "react";

const MAX_HISTORY = 60;

function SystemSparklines() {
  const stats = useWorldStore((s) => s.stats);
  const cpuHistory = useRef<number[]>([]);
  const memHistory = useRef<number[]>([]);
  const hpHistory = useRef<number[]>([]);

  useEffect(() => {
    cpuHistory.current = [...cpuHistory.current, stats.total_cpu].slice(-MAX_HISTORY);
    memHistory.current = [...memHistory.current, stats.total_memory].slice(-MAX_HISTORY);
    hpHistory.current = [...hpHistory.current, stats.avg_hp].slice(-MAX_HISTORY);
  });

  return (
    <div
      style={{
        height: "100%",
        display: "flex",
        flexDirection: "column",
        justifyContent: "center",
        alignItems: "center",
        padding: 24,
        fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
      }}
    >
      <div style={{ color: "#6b728a", fontSize: 12, marginBottom: 24, textTransform: "uppercase", letterSpacing: 1 }}>
        System Overview
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 20, width: "100%" }}>
        <SparklineSection label="Total CPU" data={cpuHistory.current} color="#4ade80" suffix="%" value={stats.total_cpu} />
        <SparklineSection label="Total Memory" data={memHistory.current} color="#60a5fa" suffix=" B" value={stats.total_memory} formatMem />
        <SparklineSection label="Avg HP" data={hpHistory.current} color="#a78bfa" suffix="" value={stats.avg_hp} />
      </div>
      <div style={{ color: "#4a4a6a", fontSize: 11, marginTop: 24 }}>
        Select a process for details
      </div>
    </div>
  );
}

function SparklineSection({
  label,
  data,
  color,
  value,
  suffix,
  formatMem,
}: {
  label: string;
  data: number[];
  color: string;
  value: number;
  suffix: string;
  formatMem?: boolean;
}) {
  const display = formatMem ? formatMemory(value) : `${value.toFixed(1)}${suffix}`;
  return (
    <div>
      <div style={{ display: "flex", justifyContent: "space-between", marginBottom: 4 }}>
        <span style={{ fontSize: 11, color: "#8b8baf", textTransform: "uppercase", letterSpacing: 0.5 }}>{label}</span>
        <span style={{ fontSize: 13, color, fontWeight: 600 }}>{display}</span>
      </div>
      <SparklineChart data={data} color={color} label={label} width={280} height={50} />
    </div>
  );
}

function formatMemory(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

export function OverviewPage() {
  const processes = useWorldStore((s) => s.processes);
  const connections = useWorldStore((s) => s.connections);
  const selectedPid = useWorldStore((s) => s.selectedPid);
  const selectProcess = useWorldStore((s) => s.selectProcess);
  const clearSelection = useWorldStore((s) => s.clearSelection);

  const selectedProcess = selectedPid != null
    ? processes.find((p) => p.pid === selectedPid) ?? null
    : null;

  // Auto-clear selection if process disappears
  if (selectedPid != null && selectedProcess == null && processes.length > 0) {
    clearSelection();
  }

  return (
    <div style={{ display: "flex", height: "100%", overflow: "hidden" }}>
      {/* Process table — left */}
      <div
        style={{
          flex: "7 1 0",
          minWidth: 0,
          borderRight: "1px solid #1e1e3a",
          overflow: "hidden",
        }}
      >
        <ProcessTable
          processes={processes}
          selectedPid={selectedPid}
          onSelect={selectProcess}
        />
      </div>

      {/* Detail panel — right */}
      <div
        style={{
          flex: "3 1 0",
          minWidth: 240,
          maxWidth: 360,
          background: "#0a0a14",
          overflow: "hidden",
        }}
      >
        {selectedProcess ? (
          <ProcessDetail
            process={selectedProcess}
            connections={connections}
            allProcesses={processes}
            onClose={clearSelection}
          />
        ) : (
          <SystemSparklines />
        )}
      </div>
    </div>
  );
}
