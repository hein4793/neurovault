import { Suspense } from "react";
import { Canvas } from "@react-three/fiber";
import { EnvironmentSetup } from "./EnvironmentSetup";
import { BrainMesh } from "./BrainMesh";
import { CoreNode } from "./CoreNode";
import { NeuronCloud } from "./NeuronCloud";
import { NeuronInstances } from "./NeuronInstances";
import { NeuralEffects } from "./NeuralEffects";
import { DomainLabels } from "./DomainLabels";
import { PostProcessing } from "./PostProcessing";

export function BrainScene() {
  return (
    <div className="w-full h-full">
      <Canvas
        camera={{ position: [0, 40, 550], fov: 60, near: 1, far: 2000 }}
        dpr={[1, 1.5]}
        gl={{
          antialias: true,
          alpha: false,
          powerPreference: "high-performance",
        }}
        onCreated={({ gl }) => {
          gl.toneMapping = 0;
          gl.setClearColor("#050510");
        }}
      >
        <EnvironmentSetup />

        <Suspense fallback={null}>
          <BrainMesh />
          <CoreNode />
          <NeuralEffects />
          <DomainLabels />
        </Suspense>

        {/* Neural cloud — ALL 199K+ nodes as the primary visualization */}
        <Suspense fallback={null}>
          <NeuronCloud />
        </Suspense>

        {/* Top 600 nodes — interactive clickable spheres */}
        <Suspense fallback={null}>
          <NeuronInstances />
        </Suspense>

        <PostProcessing />
      </Canvas>
    </div>
  );
}
