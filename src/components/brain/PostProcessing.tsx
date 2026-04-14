import { EffectComposer, Bloom, Vignette } from "@react-three/postprocessing";

export function PostProcessing() {
  return (
    <EffectComposer>
      <Bloom
        luminanceThreshold={1.2}
        luminanceSmoothing={0.9}
        intensity={0.2}
        mipmapBlur
      />
      <Vignette
        eskil={false}
        offset={0.1}
        darkness={0.7}
      />
    </EffectComposer>
  );
}
