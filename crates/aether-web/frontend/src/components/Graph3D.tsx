import { Canvas } from "@react-three/fiber";
import { OrbitControls, Stars, Grid } from "@react-three/drei";
import { EffectComposer, Bloom } from "@react-three/postprocessing";
import { useWorldStore } from "../stores/worldStore";
import { ProcessNode } from "./ProcessNode";
import { ConnectionEdge } from "./ConnectionEdge";
import type { Process } from "../types";

function Scene() {
  const processes = useWorldStore((s) => s.processes);
  const connections = useWorldStore((s) => s.connections);
  const selectedPid = useWorldStore((s) => s.selectedPid);
  const selectProcess = useWorldStore((s) => s.selectProcess);

  const processByPid = new Map<number, Process>();
  for (const p of processes) {
    processByPid.set(p.pid, p);
  }

  return (
    <>
      <ambientLight intensity={0.4} />
      <pointLight position={[10, 10, 10]} intensity={1} />
      <pointLight position={[-10, -5, -10]} intensity={0.5} />

      <Stars radius={100} depth={50} count={2000} factor={4} fade speed={1} />

      <Grid
        position={[0, -5, 0]}
        args={[50, 50]}
        cellSize={1}
        cellThickness={0.5}
        cellColor="#1a1a2e"
        sectionSize={5}
        sectionThickness={1}
        sectionColor="#2a2a4e"
        fadeDistance={40}
        infiniteGrid
      />

      {processes.map((proc) => (
        <ProcessNode
          key={proc.pid}
          process={proc}
          selected={selectedPid === proc.pid}
          onClick={() => selectProcess(proc.pid)}
        />
      ))}

      {connections.map((conn) => {
        const fromProc = processByPid.get(conn.from_pid);
        const toProc = processByPid.get(conn.to_pid);
        if (!fromProc || !toProc) return null;
        return (
          <ConnectionEdge
            key={`${conn.from_pid}-${conn.to_pid}`}
            from={fromProc.position}
            to={toProc.position}
            connection={conn}
          />
        );
      })}

      <EffectComposer>
        <Bloom
          intensity={0.5}
          luminanceThreshold={0.6}
          luminanceSmoothing={0.9}
        />
      </EffectComposer>
    </>
  );
}

export function Graph3D() {
  return (
    <Canvas
      camera={{ position: [15, 10, 15], fov: 60 }}
      style={{ background: "#0a0a1a" }}
      onPointerMissed={() => useWorldStore.getState().clearSelection()}
    >
      <OrbitControls
        enableDamping
        dampingFactor={0.05}
        minDistance={5}
        maxDistance={80}
      />
      <Scene />
    </Canvas>
  );
}
