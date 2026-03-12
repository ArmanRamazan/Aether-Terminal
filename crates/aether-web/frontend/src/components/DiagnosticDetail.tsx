import { useCallback, useState } from "react";
import type { Diagnostic, Evidence } from "../types";

const SEVERITY_COLORS: Record<string, string> = {
  critical: "#ff3c3c",
  warning: "#ffc832",
  info: "#64c8ff",
};

const URGENCY_COLORS: Record<string, string> = {
  critical: "#ff3c3c",
  high: "#f87171",
  medium: "#ffc832",
  low: "#64c8ff",
};

const TREND_ARROWS: Record<string, string> = {
  growing: "↑",
  stable: "→",
  declining: "↓",
};

function EvidenceCard({ ev }: { ev: Evidence }) {
  const ratio = ev.threshold > 0 ? Math.min(ev.current / ev.threshold, 1.5) : 0;
  const barPercent = Math.min(ratio * 100, 100);
  const isOver = ev.current >= ev.threshold;

  return (
    <div
      style={{
        background: "#12121f",
        border: "1px solid #1e1e3a",
        borderRadius: 6,
        padding: "10px 12px",
        display: "flex",
        flexDirection: "column",
        gap: 6,
      }}
    >
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <span style={{ color: "#8b8baf", fontSize: 11, textTransform: "uppercase", letterSpacing: 0.5 }}>
          {ev.metric}
        </span>
        {ev.trend && (
          <span style={{ color: isOver ? "#f87171" : "#4ade80", fontSize: 13 }}>
            {TREND_ARROWS[ev.trend] ?? ""}
          </span>
        )}
      </div>
      <div style={{ display: "flex", alignItems: "baseline", gap: 6 }}>
        <span style={{ color: isOver ? "#f87171" : "#e0e0e0", fontSize: 16, fontWeight: 700 }}>
          {ev.current.toFixed(1)}
        </span>
        <span style={{ color: "#6b728a", fontSize: 11 }}>/ {ev.threshold.toFixed(1)}</span>
      </div>
      <div
        style={{
          height: 4,
          background: "#1e1e2e",
          borderRadius: 2,
          overflow: "hidden",
        }}
      >
        <div
          style={{
            width: `${barPercent}%`,
            height: "100%",
            background: isOver ? "#f87171" : "#4ade80",
            borderRadius: 2,
            transition: "width 0.3s",
          }}
        />
      </div>
      <span style={{ color: "#6b728a", fontSize: 11 }}>{ev.context}</span>
    </div>
  );
}

interface DiagnosticDetailProps {
  diagnostic: Diagnostic;
}

export function DiagnosticDetail({ diagnostic }: DiagnosticDetailProps) {
  const [actionStatus, setActionStatus] = useState<string | null>(null);
  const d = diagnostic;
  const sevColor = SEVERITY_COLORS[d.severity] ?? "#8b8baf";

  const handleAction = useCallback(
    async (action: "execute" | "dismiss") => {
      try {
        const res = await fetch(`/api/diagnostics/${d.id}/${action}`, { method: "POST" });
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        setActionStatus(action === "execute" ? "Executed" : "Dismissed");
      } catch {
        setActionStatus(`Failed to ${action}`);
      }
    },
    [d.id],
  );

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 16,
        overflow: "auto",
        height: "100%",
        padding: "16px",
      }}
    >
      {/* Header */}
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <span
          style={{
            padding: "3px 8px",
            borderRadius: 4,
            background: sevColor + "22",
            color: sevColor,
            fontSize: 11,
            fontWeight: 700,
            textTransform: "uppercase",
          }}
        >
          {d.severity}
        </span>
        <span style={{ color: "#e0e0e0", fontSize: 14, fontWeight: 600 }}>{d.summary}</span>
      </div>

      {/* Target + Category */}
      <div style={{ display: "flex", gap: 16, fontSize: 12 }}>
        <div>
          <span style={{ color: "#6b728a" }}>Target: </span>
          <span style={{ color: "#c8c8d8" }}>
            {d.target_type} / {d.target_name}
          </span>
        </div>
        <div>
          <span style={{ color: "#6b728a" }}>Host: </span>
          <span style={{ color: "#c8c8d8" }}>{d.host}</span>
        </div>
        <span
          style={{
            padding: "2px 6px",
            borderRadius: 4,
            background: "#1e1e2e",
            color: "#a78bfa",
            fontSize: 11,
          }}
        >
          {d.category}
        </span>
      </div>

      {/* Evidence */}
      {d.evidence.length > 0 && (
        <div>
          <div
            style={{
              color: "#8b8baf",
              fontSize: 11,
              textTransform: "uppercase",
              letterSpacing: 0.5,
              marginBottom: 8,
              fontWeight: 600,
            }}
          >
            Evidence
          </div>
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8 }}>
            {d.evidence.map((ev, i) => (
              <EvidenceCard key={i} ev={ev} />
            ))}
          </div>
        </div>
      )}

      {/* Recommendation */}
      <div
        style={{
          background: "#1a1a2e",
          border: "1px solid #2a2a4a",
          borderRadius: 6,
          padding: "12px 14px",
          display: "flex",
          flexDirection: "column",
          gap: 8,
        }}
      >
        <div
          style={{
            color: "#8b8baf",
            fontSize: 11,
            textTransform: "uppercase",
            letterSpacing: 0.5,
            fontWeight: 600,
          }}
        >
          Recommendation
        </div>
        <div style={{ color: "#e0e0e0", fontSize: 13, fontWeight: 600 }}>
          {d.recommendation.action}
        </div>
        <div style={{ color: "#c8c8d8", fontSize: 12 }}>{d.recommendation.reason}</div>
        <span
          style={{
            alignSelf: "flex-start",
            padding: "2px 6px",
            borderRadius: 4,
            background: (URGENCY_COLORS[d.recommendation.urgency] ?? "#6b728a") + "22",
            color: URGENCY_COLORS[d.recommendation.urgency] ?? "#6b728a",
            fontSize: 11,
            fontWeight: 600,
            textTransform: "uppercase",
          }}
        >
          {d.recommendation.urgency}
        </span>
      </div>

      {/* Actions */}
      <div style={{ display: "flex", gap: 10, alignItems: "center" }}>
        <button
          onClick={() => handleAction("execute")}
          style={{
            padding: "6px 16px",
            borderRadius: 4,
            border: "none",
            background: "#166534",
            color: "#4ade80",
            fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
            fontSize: 12,
            fontWeight: 600,
            cursor: "pointer",
          }}
        >
          Execute
        </button>
        <button
          onClick={() => handleAction("dismiss")}
          style={{
            padding: "6px 16px",
            borderRadius: 4,
            border: "1px solid #2a2a4a",
            background: "transparent",
            color: "#8b8baf",
            fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
            fontSize: 12,
            cursor: "pointer",
          }}
        >
          Dismiss
        </button>
        {actionStatus && (
          <span
            style={{
              fontSize: 11,
              color: actionStatus.startsWith("Failed") ? "#f87171" : "#4ade80",
            }}
          >
            {actionStatus}
          </span>
        )}
      </div>
    </div>
  );
}
