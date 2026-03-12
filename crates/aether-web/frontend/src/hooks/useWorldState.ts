import { useEffect, useRef } from "react";
import { useWorldStore } from "../stores/worldStore";
import { useMetricsStore } from "../stores/metricsStore";
import type { WorldUpdate } from "../types";

const RECONNECT_DELAY = 1000;

export function useWorldState() {
  const setWorldState = useWorldStore((s) => s.setWorldState);
  const appendSample = useMetricsStore((s) => s.appendSample);
  const wsRef = useRef<WebSocket | null>(null);

  useEffect(() => {
    let disposed = false;
    let reconnectTimer: ReturnType<typeof setTimeout>;

    function connect() {
      if (disposed) return;

      const protocol = location.protocol === "https:" ? "wss:" : "ws:";
      const ws = new WebSocket(`${protocol}//${location.host}/ws`);
      wsRef.current = ws;

      ws.onmessage = (event) => {
        const update: WorldUpdate = JSON.parse(event.data as string);
        setWorldState(
          update.processes,
          update.connections,
          update.stats,
          update.diagnostics ?? [],
          update.diagnostic_stats ?? { critical: 0, warning: 0, info: 0, total: 0 },
          update.timestamp,
        );

        const ts = update.timestamp;
        const stats = update.stats;
        const ds = update.diagnostic_stats ?? { critical: 0, warning: 0, info: 0, total: 0 };
        appendSample("cpu", stats.total_cpu, ts);
        appendSample("memory", stats.total_memory, ts);
        appendSample("process_count", stats.process_count, ts);
        appendSample("avg_hp", stats.avg_hp, ts);
        appendSample("diagnostics_critical", ds.critical, ts);
        appendSample("diagnostics_warning", ds.warning, ts);
        appendSample("diagnostics_info", ds.info, ts);
      };

      ws.onclose = () => {
        if (!disposed) {
          reconnectTimer = setTimeout(connect, RECONNECT_DELAY);
        }
      };

      ws.onerror = () => {
        ws.close();
      };
    }

    connect();

    return () => {
      disposed = true;
      clearTimeout(reconnectTimer);
      wsRef.current?.close();
    };
  }, [setWorldState, appendSample]);
}
