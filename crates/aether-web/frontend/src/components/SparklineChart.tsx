import { Area, AreaChart, ResponsiveContainer } from "recharts";

interface SparklineChartProps {
  data: number[];
  color: string;
  label: string;
  width?: number;
  height?: number;
}

export function SparklineChart({
  data,
  color,
  label,
  width = 200,
  height = 60,
}: SparklineChartProps) {
  const chartData = data.map((value, i) => ({ i, value }));
  const gradientId = `gradient-${label.replace(/\s+/g, "-")}`;

  return (
    <div style={{ width, height }}>
      <ResponsiveContainer width="100%" height="100%">
        <AreaChart data={chartData} margin={{ top: 2, right: 2, bottom: 2, left: 2 }}>
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
  );
}
