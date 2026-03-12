import { useCallback, useMemo, useState } from "react";
import { useWorldStore } from "../stores/worldStore";
import { DiagnosticList } from "../components/DiagnosticList";
import { DiagnosticDetail } from "../components/DiagnosticDetail";
import type { Diagnostic } from "../types";

type SeverityFilter = "all" | "critical" | "warning" | "info";

const SEVERITY_COLORS: Record<string, string> = {
  critical: "#ff3c3c",
  warning: "#ffc832",
  info: "#64c8ff",
};

const FILTER_BUTTONS: { label: string; value: SeverityFilter }[] = [
  { label: "All", value: "all" },
  { label: "Critical", value: "critical" },
  { label: "Warning", value: "warning" },
  { label: "Info", value: "info" },
];

export function DiagnosticsPage() {
  const allDiagnostics = useWorldStore((s) => s.diagnostics);
  const stats = useWorldStore((s) => s.diagnosticStats);
  const selectedHost = useWorldStore((s) => s.selectedHost);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [filter, setFilter] = useState<SeverityFilter>("all");
  const [search, setSearch] = useState("");

  // Filter by host first
  const diagnostics = useMemo(() => {
    if (!selectedHost) return allDiagnostics;
    return allDiagnostics.filter((d) => d.host === selectedHost);
  }, [allDiagnostics, selectedHost]);

  const filtered = useMemo(() => {
    let result: Diagnostic[] = diagnostics;
    if (filter !== "all") {
      result = result.filter((d) => d.severity === filter);
    }
    if (search) {
      const q = search.toLowerCase();
      result = result.filter(
        (d) =>
          d.summary.toLowerCase().includes(q) ||
          d.target_name.toLowerCase().includes(q),
      );
    }
    return result;
  }, [diagnostics, filter, search]);

  const selected = useMemo(
    () => diagnostics.find((d) => d.id === selectedId) ?? null,
    [diagnostics, selectedId],
  );

  const handleSelect = useCallback((id: number) => {
    setSelectedId((prev) => (prev === id ? null : id));
  }, []);

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%", overflow: "hidden" }}>
      {/* Top bar */}
      <div
        style={{
          padding: "12px 16px",
          borderBottom: "1px solid #1e1e3a",
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          flexWrap: "wrap",
          gap: 10,
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <span style={{ fontSize: 16 }}>⚕</span>
          <span
            style={{
              fontSize: 14,
              fontWeight: 700,
              color: "#e0e0e0",
              letterSpacing: 0.5,
              fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
            }}
          >
            Diagnostics
          </span>
        </div>

        {/* Severity summary badges */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 14,
            fontSize: 12,
            fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
          }}
        >
          <span style={{ color: SEVERITY_COLORS.critical }}>
            ■ {stats.critical} Critical
          </span>
          <span style={{ color: SEVERITY_COLORS.warning }}>
            ■ {stats.warning} Warning
          </span>
          <span style={{ color: SEVERITY_COLORS.info }}>
            ● {stats.info} Info
          </span>
        </div>

        {/* Filters + search */}
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          {FILTER_BUTTONS.map((fb) => (
            <button
              key={fb.value}
              onClick={() => setFilter(fb.value)}
              style={{
                padding: "4px 10px",
                borderRadius: 4,
                border: filter === fb.value ? "1px solid #a78bfa" : "1px solid #1e1e3a",
                background: filter === fb.value ? "#1a1a2e" : "transparent",
                color: filter === fb.value ? "#a78bfa" : "#6b728a",
                fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
                fontSize: 11,
                cursor: "pointer",
              }}
            >
              {fb.label}
            </button>
          ))}
          <input
            type="text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search..."
            style={{
              padding: "4px 10px",
              borderRadius: 4,
              border: "1px solid #1e1e3a",
              background: "#0d0d14",
              color: "#e0e0e0",
              fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
              fontSize: 12,
              outline: "none",
              width: 160,
            }}
          />
        </div>
      </div>

      {/* Split layout */}
      <div style={{ display: "flex", flex: 1, overflow: "hidden" }}>
        {/* List panel — 55% */}
        <div
          style={{
            flex: "0 0 55%",
            borderRight: "1px solid #1e1e3a",
            overflow: "hidden",
          }}
        >
          <DiagnosticList
            diagnostics={filtered}
            selectedId={selectedId}
            onSelect={handleSelect}
          />
        </div>

        {/* Detail panel — 45% */}
        <div style={{ flex: "0 0 45%", overflow: "hidden" }}>
          {selected ? (
            <DiagnosticDetail diagnostic={selected} />
          ) : (
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                height: "100%",
                color: "#6b728a",
                fontSize: 13,
                fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
              }}
            >
              Select a diagnostic to view details
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
