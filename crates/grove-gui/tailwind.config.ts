import type { Config } from "tailwindcss";

export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        base: "#0C0C0E",
        surface: {
          DEFAULT: "#131316",
          hover: "#1A1A1F",
          active: "#1E1E23",
          raised: "#222228",
        },
        t1: "#FFFFFF",
        t2: "#FFFFFF",
        t3: "#FFFFFF",
        t4: "#FFFFFF",
        accent: {
          DEFAULT: "#34D399",
          dim: "rgba(52,211,153,0.12)",
          muted: "rgba(52,211,153,0.06)",
        },
        blue: {
          DEFAULT: "#60A5FA",
          dim: "rgba(96,165,250,0.12)",
          muted: "rgba(96,165,250,0.06)",
        },
        danger: {
          DEFAULT: "#F87171",
          dim: "rgba(248,113,113,0.12)",
          muted: "rgba(248,113,113,0.06)",
        },
        warn: {
          DEFAULT: "#FBBF24",
          dim: "rgba(251,191,36,0.12)",
        },
        purple: {
          DEFAULT: "#818CF8",
          dim: "rgba(99,102,241,0.12)",
        },
      },
      fontFamily: {
        sans: [
          "-apple-system",
          "BlinkMacSystemFont",
          "SF Pro Text",
          "Helvetica Neue",
          "sans-serif",
        ],
        mono: [
          "SF Mono",
          "Menlo",
          "Monaco",
          "Cascadia Code",
          "monospace",
        ],
      },
      fontSize: {
        "2xs": ["9px", { lineHeight: "12px" }],
        xs: ["10px", { lineHeight: "14px" }],
        sm: ["11px", { lineHeight: "16px" }],
        base: ["12px", { lineHeight: "18px" }],
        md: ["13px", { lineHeight: "20px" }],
        lg: ["15px", { lineHeight: "22px" }],
        xl: ["20px", { lineHeight: "28px" }],
        "2xl": ["24px", { lineHeight: "32px" }],
        "3xl": ["36px", { lineHeight: "40px" }],
      },
      spacing: {
        "0.5": "2px",
        "1": "4px",
        "1.5": "6px",
        "2": "8px",
        "2.5": "10px",
        "3": "12px",
        "3.5": "14px",
        "4": "16px",
        "5": "20px",
        "6": "24px",
        "7": "28px",
        "8": "32px",
        "10": "40px",
        "12": "48px",
      },
      borderRadius: {
        sm: "2px",
        DEFAULT: "4px",
        md: "6px",
        lg: "8px",
        full: "9999px",
      },
      animation: {
        "ping-slow": "ping 1.5s cubic-bezier(0,0,0.2,1) infinite",
        "fade-in": "fadeIn 0.15s ease-out",
        pulse: "pulse 1.5s infinite",
        spin: "spin 0.6s linear infinite",
      },
      transitionDuration: {
        fast: "100ms",
        DEFAULT: "150ms",
        slow: "300ms",
      },
    },
  },
  plugins: [],
} satisfies Config;
