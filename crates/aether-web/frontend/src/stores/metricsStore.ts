import { create } from "zustand";

const MAX_SAMPLES = 300;

interface MetricSample {
  timestamp: number;
  value: number;
}

interface MetricsState {
  history: Record<string, MetricSample[]>;

  appendSample: (metric: string, value: number, timestamp: number) => void;
  getHistory: (metric: string) => MetricSample[];
}

export const useMetricsStore = create<MetricsState>((set, get) => ({
  history: {},

  appendSample: (metric, value, timestamp) =>
    set((state) => {
      const prev = state.history[metric] ?? [];
      const next = [...prev, { timestamp, value }].slice(-MAX_SAMPLES);
      return { history: { ...state.history, [metric]: next } };
    }),

  getHistory: (metric) => get().history[metric] ?? [],
}));
