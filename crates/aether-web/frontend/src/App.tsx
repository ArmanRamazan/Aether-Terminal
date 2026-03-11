import { BrowserRouter, Route, Routes } from "react-router-dom";
import { useWorldState } from "./hooks/useWorldState";
import { Layout } from "./components/Layout";
import { OverviewPage } from "./pages/OverviewPage";
import { Graph3DPage } from "./pages/Graph3DPage";

function Network() {
  return <div>Network — coming soon</div>;
}

function Arbiter() {
  return <div>Arbiter — coming soon</div>;
}

function AppRoutes() {
  useWorldState();

  return (
    <Routes>
      <Route element={<Layout />}>
        <Route path="/" element={<OverviewPage />} />
        <Route path="/graph" element={<Graph3DPage />} />
        <Route path="/network" element={<Network />} />
        <Route path="/arbiter" element={<Arbiter />} />
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
