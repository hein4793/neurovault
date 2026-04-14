import { Billboard, Text } from "@react-three/drei";
import { DOMAIN_LABEL_POSITIONS } from "@/lib/constants";
import { getDomainColor } from "@/lib/tauri";

const LABELS = Object.entries(DOMAIN_LABEL_POSITIONS);

export function DomainLabels() {
  return (
    <>
      {LABELS.map(([domain, position]) => (
        <Billboard key={domain} position={position} follow lockX={false} lockY={false} lockZ={false}>
          <Text
            fontSize={6}
            color={getDomainColor(domain)}
            anchorX="center"
            anchorY="middle"
            outlineWidth={0.3}
            outlineColor="#000000"
            fillOpacity={0.5}
            font={undefined}
          >
            {domain.toUpperCase()}
          </Text>
        </Billboard>
      ))}
    </>
  );
}
