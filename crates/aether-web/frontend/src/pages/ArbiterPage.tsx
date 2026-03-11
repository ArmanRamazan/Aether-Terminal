import { useCallback, useEffect, useState } from "react";
import { ArbiterQueue, type ArbiterAction } from "../components/ArbiterQueue";

export function ArbiterPage() {
  const [actions, setActions] = useState<ArbiterAction[]>([]);
  const [error, setError] = useState<string | null>(null);

  const fetchActions = useCallback(async () => {
    try {
      const res = await fetch("/api/arbiter/pending");
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const data: ArbiterAction[] = await res.json();
      setActions(data);
      setError(null);
    } catch {
      setError("Failed to fetch arbiter actions");
    }
  }, []);

  useEffect(() => {
    fetchActions();
    const interval = setInterval(fetchActions, 3000);
    return () => clearInterval(interval);
  }, [fetchActions]);

  const handleApprove = useCallback(async (id: string) => {
    try {
      const res = await fetch(`/api/arbiter/${id}/approve`, { method: "POST" });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      setActions((prev) =>
        prev.map((a) => (a.id === id ? { ...a, status: "approved" as const } : a)),
      );
    } catch {
      setError("Failed to approve action");
    }
  }, []);

  const handleDeny = useCallback(async (id: string) => {
    try {
      const res = await fetch(`/api/arbiter/${id}/deny`, { method: "POST" });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      setActions((prev) =>
        prev.map((a) => (a.id === id ? { ...a, status: "denied" as const } : a)),
      );
    } catch {
      setError("Failed to deny action");
    }
  }, []);

  const pendingActions = actions.filter((a) => a.status === "pending");

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
          <span style={{ fontSize: 16 }}>{"\u2696"}</span>
          <span
            style={{
              fontSize: 14,
              fontWeight: 700,
              color: "#e0e0e0",
              letterSpacing: 0.5,
              fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
            }}
          >
            Arbiter Queue
          </span>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
          {error && (
            <span
              style={{
                fontSize: 11,
                color: "#f87171",
                fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
              }}
            >
              {error}
            </span>
          )}
          <span
            style={{
              fontSize: 12,
              color: pendingActions.length > 0 ? "#facc15" : "#6b728a",
              fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
            }}
          >
            {pendingActions.length} pending
          </span>
        </div>
      </div>

      <div style={{ flex: 1, overflow: "hidden" }}>
        <ArbiterQueue actions={actions} onApprove={handleApprove} onDeny={handleDeny} />
      </div>
    </div>
  );
}
