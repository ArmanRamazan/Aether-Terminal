import { create } from "zustand";
import type { Connection, Diagnostic, DiagnosticStats, Process, SystemStats } from "../types";

interface WorldState {
  processes: Process[];
  connections: Connection[];
  stats: SystemStats;
  diagnostics: Diagnostic[];
  diagnosticStats: DiagnosticStats;
  selectedPid: number | null;
  lastUpdate: number;
  availableHosts: string[];
  selectedHost: string | null;

  setWorldState: (
    processes: Process[],
    connections: Connection[],
    stats: SystemStats,
    diagnostics: Diagnostic[],
    diagnosticStats: DiagnosticStats,
    timestamp: number,
  ) => void;
  selectProcess: (pid: number) => void;
  clearSelection: () => void;
  setSelectedHost: (host: string | null) => void;
  clearHostFilter: () => void;
}

const defaultStats: SystemStats = {
  process_count: 0,
  total_cpu: 0,
  total_memory: 0,
  avg_hp: 0,
};

const defaultDiagnosticStats: DiagnosticStats = {
  critical: 0,
  warning: 0,
  info: 0,
  total: 0,
};

export const useWorldStore = create<WorldState>((set) => ({
  processes: [],
  connections: [],
  stats: defaultStats,
  diagnostics: [],
  diagnosticStats: defaultDiagnosticStats,
  selectedPid: null,
  lastUpdate: 0,
  availableHosts: [],
  selectedHost: null,

  setWorldState: (processes, connections, stats, diagnostics, diagnosticStats, timestamp) => {
    const hostSet = new Set<string>();
    hostSet.add("local");
    for (const d of diagnostics) {
      if (d.host) hostSet.add(d.host);
    }
    set({
      processes,
      connections,
      stats,
      diagnostics,
      diagnosticStats,
      lastUpdate: timestamp,
      availableHosts: Array.from(hostSet).sort(),
    });
  },

  selectProcess: (pid) => set({ selectedPid: pid }),

  clearSelection: () => set({ selectedPid: null }),

  setSelectedHost: (host) => set({ selectedHost: host }),

  clearHostFilter: () => set({ selectedHost: null }),
}));
