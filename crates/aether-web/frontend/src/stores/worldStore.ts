import { create } from "zustand";
import type { Connection, Process, SystemStats } from "../types";

interface WorldState {
  processes: Process[];
  connections: Connection[];
  stats: SystemStats;
  selectedPid: number | null;
  lastUpdate: number;

  setWorldState: (
    processes: Process[],
    connections: Connection[],
    stats: SystemStats,
    timestamp: number,
  ) => void;
  selectProcess: (pid: number) => void;
  clearSelection: () => void;
}

const defaultStats: SystemStats = {
  process_count: 0,
  total_cpu: 0,
  total_memory: 0,
  avg_hp: 0,
};

export const useWorldStore = create<WorldState>((set) => ({
  processes: [],
  connections: [],
  stats: defaultStats,
  selectedPid: null,
  lastUpdate: 0,

  setWorldState: (processes, connections, stats, timestamp) =>
    set({ processes, connections, stats, lastUpdate: timestamp }),

  selectProcess: (pid) => set({ selectedPid: pid }),

  clearSelection: () => set({ selectedPid: null }),
}));
