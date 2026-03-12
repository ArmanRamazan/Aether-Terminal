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

  setWorldState: (processes, connections, stats, diagnostics, diagnosticStats, timestamp) =>
    set({ processes, connections, stats, diagnostics, diagnosticStats, lastUpdate: timestamp }),

  selectProcess: (pid) => set({ selectedPid: pid }),

  clearSelection: () => set({ selectedPid: null }),
}));
