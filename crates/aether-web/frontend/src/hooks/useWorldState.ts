import { useEffect, useRef } from "react";
import { useWorldStore } from "../stores/worldStore";
import type { WorldUpdate } from "../types";

const RECONNECT_DELAY = 1000;

export function useWorldState() {
  const setWorldState = useWorldStore((s) => s.setWorldState);
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
          update.timestamp,
        );
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
  }, [setWorldState]);
}
