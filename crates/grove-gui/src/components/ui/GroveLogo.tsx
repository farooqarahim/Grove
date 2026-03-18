import * as React from "react";

interface GroveLogoProps {
  size?: number;
  color?: string;
  className?: string;
}

export const GroveLogo: React.FC<GroveLogoProps> = ({ size = 24, color = "currentColor", className }) => (
  <svg
    xmlns="http://www.w3.org/2000/svg"
    viewBox="0 0 100 100"
    width={size}
    height={size}
    className={className}
  >
    <g
      fill="none"
      stroke={color}
      strokeWidth={4.5}
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M 15 35 L 15 22 A 7 7 0 0 1 22 15 L 35 15" />
      <path d="M 65 15 L 78 15 A 7 7 0 0 1 85 22 L 85 35" />
      <path d="M 15 65 L 15 78 A 7 7 0 0 0 22 85 L 35 85" />
      <path d="M 85 65 L 85 78 A 7 7 0 0 1 78 85 L 65 85" />
      <path d="M 26 31 L 44 50 L 26 69" />
      <circle cx={54} cy={50} r={4.5} />
      <path d="M 57.2 46.8 L 68.8 35.2" />
      <circle cx={72} cy={32} r={4.5} />
      <path d="M 57.2 53.2 L 68.8 64.8" />
      <circle cx={72} cy={68} r={4.5} />
    </g>
  </svg>
);
