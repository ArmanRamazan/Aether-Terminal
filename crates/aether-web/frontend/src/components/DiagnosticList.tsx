import { useMemo } from "react";
import type { Diagnostic } from "../types";

const SEVERITY_ORDER = { critical: 0, warning: 1, info: 2 } as const;

const SEVERITY_COLORS: Record<string, string> = {
  critical: "#ff3c3c",
  warning: "#ffc832",
  info: "#64c8ff",
};

function severityIcon(severity: string) {
  return severity === "info" ? "●" : "■";
}

interface DiagnosticListProps {
  diagnostics: Diagnostic[];
  selectedId: number | null;
  onSelect: (id: number) => void;
}

export function DiagnosticList({ diagnostics, selectedId, onSelect }: DiagnosticListProps) {
  const sorted = useMemo(
    () =>
      [...diagnostics].sort(
        (a, b) =>
          (SEVERITY_ORDER[a.severity] ?? 9) - (SEVERITY_ORDER[b.severity] ?? 9),
      ),
    [diagnostics],
  );

  if (sorted.length === 0) {
    return (
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
        No diagnostics
      </div>
    );
  }

  return (
    <div style={{ overflow: "auto", height: "100%" }}>
      <table
        style={{
          width: "100%",
          borderCollapse: "collapse",
          fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
          fontSize: 12,
        }}
      >
        <thead>
          <tr
            style={{
              position: "sticky",
              top: 0,
              background: "#0d0d14",
              borderBottom: "1px solid #1e1e3a",
              zIndex: 1,
            }}
          >
            <th style={thStyle}>Sev</th>
            <th style={thStyle}>Target</th>
            <th style={{ ...thStyle, width: "100%" }}>Summary</th>
            <th style={thStyle}>Category</th>
          </tr>
        </thead>
        <tbody>
          {sorted.map((d) => {
            const isSelected = d.id === selectedId;
            return (
              <tr
                key={d.id}
                onClick={() => onSelect(d.id)}
                style={{
                  cursor: "pointer",
                  background: isSelected ? "#1a1a2e" : "transparent",
                  borderBottom: "1px solid #12121f",
                  transition: "background 0.12s",
                }}
                onMouseEnter={(e) => {
                  if (!isSelected) e.currentTarget.style.background = "#14141f";
                }}
                onMouseLeave={(e) => {
                  if (!isSelected) e.currentTarget.style.background = "transparent";
                }}
              >
                <td style={{ ...tdStyle, textAlign: "center" }}>
                  <span style={{ color: SEVERITY_COLORS[d.severity] ?? "#8b8baf" }}>
                    {severityIcon(d.severity)}
                  </span>
                </td>
                <td
                  style={{
                    ...tdStyle,
                    whiteSpace: "nowrap",
                    color: "#c8c8d8",
                    maxWidth: 140,
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                  }}
                >
                  {d.target_name}
                </td>
                <td
                  style={{
                    ...tdStyle,
                    color: "#e0e0e0",
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                    maxWidth: 0,
                  }}
                >
                  {d.summary}
                </td>
                <td style={tdStyle}>
                  <span
                    style={{
                      padding: "2px 6px",
                      borderRadius: 4,
                      background: "#1e1e2e",
                      color: "#a78bfa",
                      fontSize: 11,
                      whiteSpace: "nowrap",
                    }}
                  >
                    {d.category}
                  </span>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

const thStyle: React.CSSProperties = {
  padding: "8px 10px",
  textAlign: "left",
  color: "#6b728a",
  fontWeight: 600,
  fontSize: 11,
  textTransform: "uppercase",
  letterSpacing: 0.5,
  whiteSpace: "nowrap",
};

const tdStyle: React.CSSProperties = {
  padding: "8px 10px",
};
