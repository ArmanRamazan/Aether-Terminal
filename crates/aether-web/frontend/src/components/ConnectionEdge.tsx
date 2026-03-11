import { Line } from "@react-three/drei";
import type { Connection } from "../types";

interface ConnectionEdgeProps {
  from: [number, number, number];
  to: [number, number, number];
  connection: Connection;
}

const PROTOCOL_COLORS: Record<string, string> = {
  TCP: "#4488ff",
  tcp: "#4488ff",
  UDP: "#44ff88",
  udp: "#44ff88",
  HTTP: "#44ffff",
  http: "#44ffff",
};

function bytesToLineWidth(bytesPerSec: number): number {
  const min = 1;
  const max = 5;
  if (bytesPerSec <= 0) return min;
  const t = Math.max(0, Math.min(1, Math.log10(bytesPerSec + 1) / 8));
  return min + t * (max - min);
}

export function ConnectionEdge({ from, to, connection }: ConnectionEdgeProps) {
  const color = PROTOCOL_COLORS[connection.protocol] ?? "#888888";
  const lineWidth = bytesToLineWidth(connection.bytes_per_sec);

  return (
    <Line
      points={[from, to]}
      color={color}
      lineWidth={lineWidth}
      transparent
      opacity={0.6}
    />
  );
}
