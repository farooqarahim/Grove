import * as React from "react";
import { C } from "../../lib/theme";

interface ScreenLoaderProps {
  label?: string;
  size?: number;
}

export const ScreenLoader: React.FC<ScreenLoaderProps> = ({
  label = "Loading",
  size = 28,
}) => (
  <div
    role="status"
    aria-live="polite"
    aria-busy="true"
    style={{
      display: "flex",
      flexDirection: "column",
      alignItems: "center",
      justifyContent: "center",
      gap: 12,
      width: "100%",
      height: "100%",
      color: C.text4,
      fontSize: 12,
      letterSpacing: 0.3,
      animation: "screen-loader-fade 240ms ease-out",
    }}
  >
    <style>{`
      @keyframes screen-loader-spin {
        from { transform: rotate(0deg); }
        to   { transform: rotate(360deg); }
      }
      @keyframes screen-loader-dash {
        0%   { stroke-dashoffset: 60; }
        50%  { stroke-dashoffset: 15; }
        100% { stroke-dashoffset: 60; }
      }
      @keyframes screen-loader-fade {
        from { opacity: 0; transform: translateY(2px); }
        to   { opacity: 1; transform: translateY(0); }
      }
      @keyframes screen-loader-dots {
        0%, 20%   { content: ""; }
        40%       { content: "."; }
        60%       { content: ".."; }
        80%, 100% { content: "..."; }
      }
      .screen-loader-label::after {
        content: "";
        display: inline-block;
        width: 12px;
        text-align: left;
        animation: screen-loader-dots 1.4s steps(1, end) infinite;
      }
    `}</style>
    <svg
      width={size}
      height={size}
      viewBox="0 0 50 50"
      style={{
        animation: "screen-loader-spin 1.1s linear infinite",
        transformOrigin: "center",
      }}
      aria-hidden="true"
    >
      <circle
        cx="25"
        cy="25"
        r="20"
        fill="none"
        stroke={C.borderSubtle}
        strokeWidth="3"
      />
      <circle
        cx="25"
        cy="25"
        r="20"
        fill="none"
        stroke={C.accent}
        strokeWidth="3"
        strokeLinecap="round"
        strokeDasharray="60 200"
        style={{
          animation: "screen-loader-dash 1.4s ease-in-out infinite",
        }}
      />
    </svg>
    <span className="screen-loader-label" style={{ opacity: 0.75 }}>
      {label}
    </span>
  </div>
);
