import { Area, AreaChart, ResponsiveContainer } from "recharts";

type Trend = "up" | "down" | "stable";

interface MetricCardProps {
  title: string;
  value: string;
  unit: string;
  chartData: number[];
  trend: Trend;
  color: string;
}

const TREND_ARROWS: Record<Trend, string> = {
  up: "▲",
  down: "▼",
  stable: "─",
};

function trendColor(trend: Trend, color: string): string {
  if (trend === "stable") return "#6b728a";
  return color;
}

const cardStyle: React.CSSProperties = {
  background: "#0d0d14",
  border: "1px solid #1e1e3a",
  borderRadius: 8,
  padding: "16px 18px",
  display: "flex",
  flexDirection: "column",
  gap: 8,
  fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
};

const titleStyle: React.CSSProperties = {
  fontSize: 11,
  color: "#6b728a",
  textTransform: "uppercase",
  letterSpacing: 1,
};

export function MetricCard({ title, value, unit, chartData, trend, color }: MetricCardProps) {
  const data = chartData.map((v, i) => ({ i, value: v }));
  const gradientId = `metric-grad-${title.toLowerCase().replace(/[^a-z0-9]/g, "-")}`;

  return (
    <div style={cardStyle}>
      <div style={titleStyle}>{title}</div>

      <div style={{ display: "flex", alignItems: "baseline", gap: 6 }}>
        <span style={{ fontSize: 24, fontWeight: 700, color }}>{value}</span>
        <span style={{ fontSize: 12, color: "#8b8baf" }}>{unit}</span>
        <span
          style={{
            marginLeft: "auto",
            fontSize: 12,
            color: trendColor(trend, color),
            fontWeight: 600,
          }}
        >
          {TREND_ARROWS[trend]}
        </span>
      </div>

      <div style={{ height: 50, marginTop: 4 }}>
        <ResponsiveContainer width="100%" height="100%">
          <AreaChart data={data} margin={{ top: 2, right: 2, bottom: 2, left: 2 }}>
            <defs>
              <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor={color} stopOpacity={0.4} />
                <stop offset="100%" stopColor={color} stopOpacity={0.05} />
              </linearGradient>
            </defs>
            <Area
              type="monotone"
              dataKey="value"
              stroke={color}
              strokeWidth={1.5}
              fill={`url(#${gradientId})`}
              isAnimationActive={false}
            />
          </AreaChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}
