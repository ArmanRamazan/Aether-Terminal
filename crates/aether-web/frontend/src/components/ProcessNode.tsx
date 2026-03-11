import { useRef, useState } from "react";
import { useFrame } from "@react-three/fiber";
import { Text } from "@react-three/drei";
import type { Mesh } from "three";
import type { Process } from "../types";

interface ProcessNodeProps {
  process: Process;
  selected: boolean;
  onClick: () => void;
}

function hpToColor(hp: number): string {
  const hue = (hp / 100) * 120;
  return `hsl(${hue}, 80%, 50%)`;
}

function memToRadius(memBytes: number): number {
  const minR = 0.3;
  const maxR = 2.0;
  if (memBytes <= 0) return minR;
  const logMem = Math.log10(memBytes + 1);
  const logMin = Math.log10(1e4);
  const logMax = Math.log10(1e10);
  const t = Math.max(0, Math.min(1, (logMem - logMin) / (logMax - logMin)));
  return minR + t * (maxR - minR);
}

export function ProcessNode({ process, selected, onClick }: ProcessNodeProps) {
  const meshRef = useRef<Mesh>(null);
  const [hovered, setHovered] = useState(false);

  const radius = memToRadius(process.mem_bytes);
  const color = hpToColor(process.hp);
  const emissiveIntensity = process.cpu_percent / 100;

  useFrame(({ clock }) => {
    if (!meshRef.current) return;
    const t = clock.getElapsedTime();
    const pulse = 1 + Math.sin(t * (process.cpu_percent / 50)) * 0.1;
    const hover = hovered ? 1.1 : 1;
    const s = pulse * hover * radius;
    meshRef.current.scale.set(s, s, s);
  });

  return (
    <group position={process.position}>
      <mesh
        ref={meshRef}
        onClick={(e) => {
          e.stopPropagation();
          onClick();
        }}
        onPointerOver={(e) => {
          e.stopPropagation();
          setHovered(true);
          document.body.style.cursor = "pointer";
        }}
        onPointerOut={() => {
          setHovered(false);
          document.body.style.cursor = "auto";
        }}
      >
        <sphereGeometry args={[1, 32, 32]} />
        <meshStandardMaterial
          color={color}
          emissive={color}
          emissiveIntensity={emissiveIntensity}
          roughness={0.3}
          metalness={0.1}
        />
      </mesh>

      {selected && (
        <mesh scale={[radius * 1.3, radius * 1.3, radius * 1.3]}>
          <sphereGeometry args={[1, 16, 16]} />
          <meshBasicMaterial color="white" wireframe transparent opacity={0.3} />
        </mesh>
      )}

      <Text
        position={[0, radius + 0.5, 0]}
        fontSize={0.4}
        color="white"
        anchorX="center"
        anchorY="bottom"
      >
        {process.name}
      </Text>
    </group>
  );
}
