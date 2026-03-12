import { BrowserRouter, Route, Routes } from "react-router-dom";
import { useWorldState } from "./hooks/useWorldState";
import { Layout } from "./components/Layout";
import { OverviewPage } from "./pages/OverviewPage";
import { Graph3DPage } from "./pages/Graph3DPage";
import { NetworkPage } from "./pages/NetworkPage";
import { ArbiterPage } from "./pages/ArbiterPage";
import { DiagnosticsPage } from "./pages/DiagnosticsPage";
import { MetricsPage } from "./pages/MetricsPage";

function AppRoutes() {
  useWorldState();

  return (
    <Routes>
      <Route element={<Layout />}>
        <Route path="/" element={<OverviewPage />} />
        <Route path="/graph" element={<Graph3DPage />} />
        <Route path="/network" element={<NetworkPage />} />
        <Route path="/arbiter" element={<ArbiterPage />} />
        <Route path="/diagnostics" element={<DiagnosticsPage />} />
        <Route path="/metrics" element={<MetricsPage />} />
      </Route>
    </Routes>
  );
}

export function App() {
  return (
    <BrowserRouter>
      <AppRoutes />
    </BrowserRouter>
  );
}
