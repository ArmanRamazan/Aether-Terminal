import { NavLink, Outlet } from "react-router-dom";
import { StatsBar } from "./StatsBar";

const NAV_ITEMS = [
  { to: "/", label: "Overview", icon: "\u25A6" },
  { to: "/graph", label: "3D Graph", icon: "\u2B22" },
  { to: "/network", label: "Network", icon: "\u2B95" },
  { to: "/arbiter", label: "Arbiter", icon: "\u2696" },
] as const;

const linkStyle = (isActive: boolean): React.CSSProperties => ({
  display: "flex",
  alignItems: "center",
  gap: "0.5rem",
  padding: "0.6rem 1rem",
  borderRadius: "6px",
  textDecoration: "none",
  fontSize: "0.85rem",
  color: isActive ? "#a78bfa" : "#8b8baf",
  background: isActive ? "#1a1a2e" : "transparent",
  transition: "background 0.15s, color 0.15s",
});

export function Layout() {
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100vh",
        background: "#0a0a0f",
        color: "#e0e0e0",
      }}
    >
      <StatsBar />
      <div style={{ display: "flex", flex: 1, overflow: "hidden" }}>
        <nav
          style={{
            width: "180px",
            padding: "1rem 0.75rem",
            display: "flex",
            flexDirection: "column",
            gap: "0.25rem",
            borderRight: "1px solid #1e1e2e",
            background: "#0d0d14",
          }}
        >
          {NAV_ITEMS.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.to === "/"}
              style={({ isActive }) => linkStyle(isActive)}
            >
              <span style={{ fontSize: "1.1rem" }}>{item.icon}</span>
              {item.label}
            </NavLink>
          ))}
        </nav>
        <main style={{ flex: 1, padding: "1.5rem", overflow: "auto" }}>
          <Outlet />
        </main>
      </div>
    </div>
  );
}
