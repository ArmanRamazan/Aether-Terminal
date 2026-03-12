import {
  Area,
  AreaChart,
  CartesianGrid,
  Legend,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { useMetricsStore } from "../stores/metricsStore";

const FONT = "'JetBrains Mono', 'Fira Code', monospace";

const chartContainerStyle: React.CSSProperties = {
  background: "#0d0d14",
  border: "1px solid #1e1e3a",
  borderRadius: 8,
  padding: "16px 18px",
  fontFamily: FONT,
};

const chartTitleStyle: React.CSSProperties = {
  fontSize: 12,
  color: "#8b8baf",
  textTransform: "uppercase",
  letterSpacing: 1,
  marginBottom: 12,
};

const axisStyle = { fontSize: 10, fill: "#6b728a", fontFamily: FONT };
const gridStroke = "#1e1e2e";

function formatTime(ts: number): string {
  const d = new Date(ts);
  const m = d.getMinutes().toString().padStart(2, "0");
  const s = d.getSeconds().toString().padStart(2, "0");
  return `${m}:${s}`;
}

function tooltipStyle() {
  return {
    contentStyle: {
      background: "#12121f",
      border: "1px solid #1e1e3a",
      borderRadius: 6,
      fontFamily: FONT,
      fontSize: 11,
      color: "#e0e0e0",
    },
    labelFormatter: (v: number) => formatTime(v),
  };
}

function CpuChart() {
  const history = useMetricsStore((s) => s.history["cpu"] ?? []);
  const data = history.map((s) => ({ ts: s.timestamp, cpu: s.value }));

  return (
    <div style={chartContainerStyle}>
      <div style={chartTitleStyle}>CPU Usage Over Time</div>
      <ResponsiveContainer width="100%" height={200}>
        <LineChart data={data} margin={{ top: 4, right: 12, bottom: 4, left: 12 }}>
          <CartesianGrid strokeDasharray="3 3" stroke={gridStroke} />
          <XAxis dataKey="ts" tickFormatter={formatTime} tick={axisStyle} />
          <YAxis tick={axisStyle} />
          <Tooltip {...tooltipStyle()} />
          <Legend wrapperStyle={{ fontSize: 11, fontFamily: FONT }} />
          <Line
            type="monotone"
            dataKey="cpu"
            name="Total CPU %"
            stroke="#4ade80"
            strokeWidth={1.5}
            dot={false}
            isAnimationActive={false}
          />
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}

function MemoryChart() {
  const history = useMetricsStore((s) => s.history["memory"] ?? []);
  const data = history.map((s) => ({
    ts: s.timestamp,
    mem: s.value / (1024 * 1024),
  }));

  return (
    <div style={chartContainerStyle}>
      <div style={chartTitleStyle}>Memory Usage Over Time</div>
      <ResponsiveContainer width="100%" height={200}>
        <AreaChart data={data} margin={{ top: 4, right: 12, bottom: 4, left: 12 }}>
          <defs>
            <linearGradient id="memGrad" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="#60a5fa" stopOpacity={0.4} />
              <stop offset="100%" stopColor="#60a5fa" stopOpacity={0.05} />
            </linearGradient>
          </defs>
          <CartesianGrid strokeDasharray="3 3" stroke={gridStroke} />
          <XAxis dataKey="ts" tickFormatter={formatTime} tick={axisStyle} />
          <YAxis tick={axisStyle} unit=" MB" />
          <Tooltip {...tooltipStyle()} />
          <Legend wrapperStyle={{ fontSize: 11, fontFamily: FONT }} />
          <Area
            type="monotone"
            dataKey="mem"
            name="Memory (MB)"
            stroke="#60a5fa"
            strokeWidth={1.5}
            fill="url(#memGrad)"
            isAnimationActive={false}
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}

function DiagnosticsChart() {
  const critHistory = useMetricsStore((s) => s.history["diagnostics_critical"] ?? []);
  const warnHistory = useMetricsStore((s) => s.history["diagnostics_warning"] ?? []);
  const infoHistory = useMetricsStore((s) => s.history["diagnostics_info"] ?? []);

  // Join series by timestamp for correct alignment
  const warnByTs = new Map(warnHistory.map((s) => [s.timestamp, s.value]));
  const infoByTs = new Map(infoHistory.map((s) => [s.timestamp, s.value]));
  const data = critHistory.map((s) => ({
    ts: s.timestamp,
    critical: s.value,
    warning: warnByTs.get(s.timestamp) ?? 0,
    info: infoByTs.get(s.timestamp) ?? 0,
  }));

  return (
    <div style={chartContainerStyle}>
      <div style={chartTitleStyle}>Active Diagnostics Over Time</div>
      <ResponsiveContainer width="100%" height={200}>
        <AreaChart data={data} margin={{ top: 4, right: 12, bottom: 4, left: 12 }}>
          <defs>
            <linearGradient id="critGrad" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="#ff3c3c" stopOpacity={0.4} />
              <stop offset="100%" stopColor="#ff3c3c" stopOpacity={0.05} />
            </linearGradient>
            <linearGradient id="warnGrad" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="#ffc832" stopOpacity={0.4} />
              <stop offset="100%" stopColor="#ffc832" stopOpacity={0.05} />
            </linearGradient>
            <linearGradient id="infoGrad" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="#64c8ff" stopOpacity={0.4} />
              <stop offset="100%" stopColor="#64c8ff" stopOpacity={0.05} />
            </linearGradient>
          </defs>
          <CartesianGrid strokeDasharray="3 3" stroke={gridStroke} />
          <XAxis dataKey="ts" tickFormatter={formatTime} tick={axisStyle} />
          <YAxis tick={axisStyle} allowDecimals={false} />
          <Tooltip {...tooltipStyle()} />
          <Legend wrapperStyle={{ fontSize: 11, fontFamily: FONT }} />
          <Area
            type="monotone"
            dataKey="critical"
            name="Critical"
            stroke="#ff3c3c"
            strokeWidth={1.5}
            fill="url(#critGrad)"
            stackId="diag"
            isAnimationActive={false}
          />
          <Area
            type="monotone"
            dataKey="warning"
            name="Warning"
            stroke="#ffc832"
            strokeWidth={1.5}
            fill="url(#warnGrad)"
            stackId="diag"
            isAnimationActive={false}
          />
          <Area
            type="monotone"
            dataKey="info"
            name="Info"
            stroke="#64c8ff"
            strokeWidth={1.5}
            fill="url(#infoGrad)"
            stackId="diag"
            isAnimationActive={false}
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}

export function SystemCharts() {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
      <CpuChart />
      <MemoryChart />
      <DiagnosticsChart />
    </div>
  );
}
