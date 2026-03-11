/** Matches Rust ProcessResponse from aether-web/src/api.rs */
export interface Process {
  pid: number;
  ppid: number;
  name: string;
  cpu_percent: number;
  mem_bytes: number;
  state: string;
  hp: number;
  xp: number;
  position: [number, number, number];
}

/** Matches Rust ConnectionResponse from aether-web/src/api.rs */
export interface Connection {
  from_pid: number;
  to_pid: number;
  protocol: string;
  bytes_per_sec: number;
}

/** Matches Rust StatsResponse from aether-web/src/api.rs */
export interface SystemStats {
  process_count: number;
  total_cpu: number;
  total_memory: number;
  avg_hp: number;
}

/** Matches Rust WorldUpdate from aether-web/src/ws.rs */
export interface WorldUpdate {
  type: string;
  processes: Process[];
  connections: Connection[];
  stats: SystemStats;
  timestamp: number;
}
