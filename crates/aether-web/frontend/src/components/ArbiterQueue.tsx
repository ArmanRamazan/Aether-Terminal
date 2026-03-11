import { useState } from "react";

export interface ArbiterAction {
  id: string;
  action_type: string;
  target_pid: number;
  target_name: string;
  reason: string;
  status: "pending" | "approved" | "denied";
}

interface ArbiterQueueProps {
  actions: ArbiterAction[];
  onApprove: (id: string) => void;
  onDeny: (id: string) => void;
}

const actionIcons: Record<string, string> = {
  kill: "\u2620",
  renice: "\u2696",
  suspend: "\u23F8",
  resume: "\u25B6",
};

const actionColors: Record<string, string> = {
  kill: "#f87171",
  renice: "#facc15",
  suspend: "#fb923c",
  resume: "#4ade80",
};

function ActionBadge({ type }: { type: string }) {
  const color = actionColors[type.toLowerCase()] ?? "#9ca3af";
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 6,
        padding: "3px 10px",
        borderRadius: 4,
        fontSize: 12,
        fontWeight: 600,
        letterSpacing: 0.5,
        background: `${color}18`,
        color,
        textTransform: "uppercase",
      }}
    >
      <span style={{ fontSize: 14 }}>{actionIcons[type.toLowerCase()] ?? "\u2699"}</span>
      {type}
    </span>
  );
}

function StatusDot({ status }: { status: ArbiterAction["status"] }) {
  const colors = { pending: "#facc15", approved: "#4ade80", denied: "#f87171" };
  return (
    <span
      style={{
        display: "inline-block",
        width: 8,
        height: 8,
        borderRadius: "50%",
        background: colors[status],
        boxShadow: `0 0 6px ${colors[status]}40`,
      }}
    />
  );
}

export function ArbiterQueue({ actions, onApprove, onDeny }: ArbiterQueueProps) {
  const [confirming, setConfirming] = useState<{ id: string; type: "approve" | "deny" } | null>(null);
  const [loading, setLoading] = useState<string | null>(null);

  function handleAction(id: string, type: "approve" | "deny") {
    if (confirming?.id === id && confirming.type === type) {
      setLoading(id);
      setConfirming(null);
      if (type === "approve") {
        onApprove(id);
      } else {
        onDeny(id);
      }
      setTimeout(() => setLoading(null), 500);
    } else {
      setConfirming({ id, type });
    }
  }

  if (actions.length === 0) {
    return (
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          height: "100%",
          gap: 12,
          color: "#6b728a",
          fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
        }}
      >
        <span style={{ fontSize: 32, opacity: 0.4 }}>{"\u2696"}</span>
        <span style={{ fontSize: 14 }}>No pending actions</span>
        <span style={{ fontSize: 12, color: "#6b728060" }}>
          The arbiter queue is empty
        </span>
      </div>
    );
  }

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 10,
        padding: "12px",
        overflow: "auto",
        height: "100%",
      }}
    >
      {actions.map((action) => {
        const isLoading = loading === action.id;
        const isPending = action.status === "pending";

        return (
          <div
            key={action.id}
            style={{
              background: "#12121f",
              border: "1px solid #1e1e3a",
              borderRadius: 8,
              padding: "14px 16px",
              opacity: isLoading ? 0.5 : 1,
              transition: "opacity 0.2s, border-color 0.2s",
            }}
          >
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                marginBottom: 10,
              }}
            >
              <ActionBadge type={action.action_type} />
              <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                <StatusDot status={action.status} />
                <span
                  style={{
                    fontSize: 11,
                    color: "#8b8baf",
                    fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
                    textTransform: "capitalize",
                  }}
                >
                  {action.status}
                </span>
              </div>
            </div>

            <div style={{ marginBottom: 10 }}>
              <div
                style={{
                  fontSize: 14,
                  color: "#e0e0e0",
                  fontWeight: 600,
                  fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
                }}
              >
                {action.target_name}
                <span style={{ color: "#6b728a", fontSize: 12, marginLeft: 8 }}>
                  PID {action.target_pid}
                </span>
              </div>
              {action.reason && (
                <div
                  style={{
                    fontSize: 12,
                    color: "#8b8baf",
                    marginTop: 4,
                    fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
                  }}
                >
                  {action.reason}
                </div>
              )}
            </div>

            {isPending && (
              <div style={{ display: "flex", gap: 8 }}>
                <button
                  onClick={() => handleAction(action.id, "approve")}
                  disabled={isLoading}
                  style={{
                    flex: 1,
                    padding: "6px 12px",
                    borderRadius: 6,
                    border: "1px solid #4ade8040",
                    background:
                      confirming?.id === action.id && confirming.type === "approve"
                        ? "#4ade8030"
                        : "#4ade8012",
                    color: "#4ade80",
                    fontSize: 12,
                    fontWeight: 600,
                    fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
                    cursor: isLoading ? "default" : "pointer",
                    transition: "background 0.15s",
                  }}
                >
                  {confirming?.id === action.id && confirming.type === "approve"
                    ? "Confirm Approve"
                    : "Approve"}
                </button>
                <button
                  onClick={() => handleAction(action.id, "deny")}
                  disabled={isLoading}
                  style={{
                    flex: 1,
                    padding: "6px 12px",
                    borderRadius: 6,
                    border: "1px solid #f8717140",
                    background:
                      confirming?.id === action.id && confirming.type === "deny"
                        ? "#f8717130"
                        : "#f8717112",
                    color: "#f87171",
                    fontSize: 12,
                    fontWeight: 600,
                    fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
                    cursor: isLoading ? "default" : "pointer",
                    transition: "background 0.15s",
                  }}
                >
                  {confirming?.id === action.id && confirming.type === "deny"
                    ? "Confirm Deny"
                    : "Deny"}
                </button>
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
