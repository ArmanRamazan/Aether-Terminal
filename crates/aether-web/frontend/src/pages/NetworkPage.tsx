import { useCallback, useState } from "react";
import { useWorldStore } from "../stores/worldStore";
import { NetworkTable } from "../components/NetworkTable";

export function NetworkPage() {
  const processes = useWorldStore((s) => s.processes);
  const connections = useWorldStore((s) => s.connections);
  const [highlightedPids, setHighlightedPids] = useState<Set<number>>(new Set());

  const handleSelectConnection = useCallback((fromPid: number, toPid: number) => {
    setHighlightedPids((prev) => {
      if (prev.has(fromPid) && prev.has(toPid)) {
        return new Set();
      }
      return new Set([fromPid, toPid]);
    });
  }, []);

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%", overflow: "hidden" }}>
      <div
        style={{
          padding: "12px 16px",
          borderBottom: "1px solid #1e1e3a",
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <span style={{ fontSize: 16 }}>&#x27A1;</span>
          <span
            style={{
              fontSize: 14,
              fontWeight: 700,
              color: "#e0e0e0",
              letterSpacing: 0.5,
              fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
            }}
          >
            Network Connections
          </span>
        </div>
        <span
          style={{
            fontSize: 12,
            color: "#6b728a",
            fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
          }}
        >
          {connections.length} active
        </span>
      </div>

      <div style={{ flex: 1, overflow: "hidden" }}>
        <NetworkTable
          connections={connections}
          processes={processes}
          highlightedPids={highlightedPids}
          onSelectConnection={handleSelectConnection}
        />
      </div>
    </div>
  );
}
