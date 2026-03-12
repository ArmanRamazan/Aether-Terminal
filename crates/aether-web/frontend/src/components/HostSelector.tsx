import { useMemo, useRef, useState } from "react";
import { useWorldStore } from "../stores/worldStore";

function hostStatusColor(criticalCount: number, warningCount: number): string {
  if (criticalCount > 0) return "#ef4444";
  if (warningCount > 0) return "#eab308";
  return "#22c55e";
}

export function HostSelector() {
  const availableHosts = useWorldStore((s) => s.availableHosts);
  const selectedHost = useWorldStore((s) => s.selectedHost);
  const setSelectedHost = useWorldStore((s) => s.setSelectedHost);
  const clearHostFilter = useWorldStore((s) => s.clearHostFilter);
  const processes = useWorldStore((s) => s.processes);
  const diagnostics = useWorldStore((s) => s.diagnostics);
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  const hostInfo = useMemo(() => {
    const info: Record<string, { processCount: number; criticalCount: number; warningCount: number; diagnosticCount: number }> = {};
    for (const host of availableHosts) {
      info[host] = { processCount: 0, criticalCount: 0, warningCount: 0, diagnosticCount: 0 };
    }
    // All processes are "local" for now
    if (info["local"]) {
      info["local"].processCount = processes.length;
    }
    for (const d of diagnostics) {
      const host = d.host || "local";
      if (!info[host]) {
        info[host] = { processCount: 0, criticalCount: 0, warningCount: 0, diagnosticCount: 0 };
      }
      info[host].diagnosticCount++;
      if (d.severity === "critical") info[host].criticalCount++;
      if (d.severity === "warning") info[host].warningCount++;
    }
    return info;
  }, [availableHosts, processes, diagnostics]);

  const handleSelect = (host: string | null) => {
    if (host === null) {
      clearHostFilter();
    } else {
      setSelectedHost(host);
    }
    setOpen(false);
  };

  const displayLabel = selectedHost ?? "All Hosts";

  return (
    <div
      ref={containerRef}
      style={{ position: "relative" }}
      onBlur={(e) => {
        if (!containerRef.current?.contains(e.relatedTarget as Node)) {
          setOpen(false);
        }
      }}
    >
      <button
        onClick={() => setOpen(!open)}
        style={{
          padding: "3px 10px",
          borderRadius: 4,
          border: "1px solid #1e1e3a",
          background: selectedHost ? "#1a1a2e" : "transparent",
          color: selectedHost ? "#a78bfa" : "#8b8baf",
          fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
          fontSize: 12,
          cursor: "pointer",
          display: "flex",
          alignItems: "center",
          gap: 6,
        }}
      >
        {selectedHost && (
          <span
            style={{
              width: 6,
              height: 6,
              borderRadius: "50%",
              background: hostStatusColor(
                hostInfo[selectedHost]?.criticalCount ?? 0,
                hostInfo[selectedHost]?.warningCount ?? 0,
              ),
              display: "inline-block",
            }}
          />
        )}
        <span>&#x2B26;</span>
        {displayLabel}
        <span style={{ fontSize: 9, marginLeft: 2 }}>&#x25BE;</span>
      </button>

      {open && (
        <div
          style={{
            position: "absolute",
            top: "100%",
            right: 0,
            marginTop: 4,
            background: "#111118",
            border: "1px solid #1e1e3a",
            borderRadius: 6,
            minWidth: 200,
            zIndex: 100,
            boxShadow: "0 4px 12px rgba(0,0,0,0.5)",
            fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
            fontSize: 12,
          }}
        >
          {/* All Hosts option */}
          <div
            tabIndex={0}
            onClick={() => handleSelect(null)}
            onKeyDown={(e) => e.key === "Enter" && handleSelect(null)}
            style={{
              padding: "8px 12px",
              cursor: "pointer",
              background: selectedHost === null ? "#1a1a2e" : "transparent",
              color: selectedHost === null ? "#a78bfa" : "#8b8baf",
              borderBottom: "1px solid #1e1e3a",
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
            }}
          >
            <span>All Hosts</span>
            <span style={{ color: "#6b728a", fontSize: 11 }}>
              {processes.length} proc
            </span>
          </div>

          {/* Individual hosts */}
          {availableHosts.map((host) => {
            const hi = hostInfo[host];
            const isSelected = selectedHost === host;
            return (
              <div
                key={host}
                tabIndex={0}
                onClick={() => handleSelect(host)}
                onKeyDown={(e) => e.key === "Enter" && handleSelect(host)}
                style={{
                  padding: "8px 12px",
                  cursor: "pointer",
                  background: isSelected ? "#1a1a2e" : "transparent",
                  color: isSelected ? "#a78bfa" : "#c0c0d0",
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  gap: 10,
                }}
              >
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <span
                    style={{
                      width: 7,
                      height: 7,
                      borderRadius: "50%",
                      background: hostStatusColor(hi?.criticalCount ?? 0, hi?.warningCount ?? 0),
                      display: "inline-block",
                      flexShrink: 0,
                    }}
                  />
                  <span>{host}</span>
                </div>
                <div style={{ display: "flex", gap: 8, color: "#6b728a", fontSize: 11 }}>
                  <span>{hi?.processCount ?? 0} proc</span>
                  {(hi?.diagnosticCount ?? 0) > 0 && (
                    <span>{hi?.diagnosticCount} diag</span>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
