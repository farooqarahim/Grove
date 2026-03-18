import { useEffect, useRef, useState, type CSSProperties } from "react";
import { StreamIcon, Terminal, XIcon } from "@/components/ui/icons";
import { C } from "@/lib/theme";

interface SessionNameModalProps {
  open: boolean;
  onClose: () => void;
  onContinue: (name: string, kind: "run" | "cli" | "hive_loom") => void;
}

const MIN_LEN = 5;
const MAX_LEN = 128;
const OPTION_CARD: CSSProperties = {
  flex: 1,
  borderRadius: 14,
  border: `1px solid ${C.border}`,
  background: `linear-gradient(180deg, ${C.surfaceHover} 0%, ${C.base} 100%)`,
  padding: 16,
  textAlign: "left",
  cursor: "pointer",
  position: "relative",
  overflow: "hidden",
};

export function SessionNameModal({ open, onClose, onContinue }: SessionNameModalProps) {
  const [name, setName] = useState("");
  const [kind, setKind] = useState<"run" | "cli" | "hive_loom">("run");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!open) {
      setName("");
      setKind("run");
      return;
    }
    // Focus input on open
    setTimeout(() => inputRef.current?.focus(), 50);
  }, [open]);

  if (!open) return null;

  const trimmed = name.trim();
  const tooShort = trimmed.length > 0 && trimmed.length < MIN_LEN;
  const valid = trimmed.length >= MIN_LEN && trimmed.length <= MAX_LEN;

  const handleSubmit = () => {
    if (!valid) return;
    onContinue(trimmed, kind);
  };

  return (
    <div
      onClick={onClose}
      style={{
        position: "fixed", inset: 0, zIndex: 100,
        display: "flex", alignItems: "center", justifyContent: "center",
        background: "rgba(0,0,0,0.5)", backdropFilter: "blur(8px)",
        WebkitBackdropFilter: "blur(8px)",
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Name your session"
        onClick={e => e.stopPropagation()}
        style={{
          width: 720,
          background: C.surface,
          borderRadius: 18,
          overflow: "hidden",
          border: `1px solid ${C.border}`,
          boxShadow: "0 28px 80px rgba(0,0,0,0.42)",
        }}
      >
        {/* Header */}
        <div style={{
          display: "flex", alignItems: "center", justifyContent: "space-between",
          padding: "18px 22px",
          background: `linear-gradient(135deg, ${C.surfaceHover} 0%, rgba(59,130,246,0.14) 52%, rgba(49,185,123,0.08) 100%)`,
          borderBottom: `1px solid ${C.border}`,
        }}>
          <div>
            <div style={{ fontSize: 15, fontWeight: 700, color: C.text1 }}>
              New Session
            </div>
            <div style={{ fontSize: 11, color: "rgba(255,255,255,0.72)", marginTop: 4 }}>
              Name it once, choose the mode, then continue into run or CLI setup.
            </div>
          </div>
          <button
            onClick={onClose}
            aria-label="Close"
            style={{ background: "none", color: C.text4, cursor: "pointer", padding: 4 }}
          >
            <XIcon size={12} />
          </button>
        </div>

        {/* Body */}
        <div style={{ padding: 22 }}>
          <div style={{
            fontSize: 10, fontWeight: 600, color: C.text4,
            textTransform: "uppercase", letterSpacing: "0.06em", marginBottom: 6,
          }}>
            Session Name
          </div>
          <input
            ref={inputRef}
            type="text"
            value={name}
            onChange={e => {
              if (e.target.value.length <= MAX_LEN) setName(e.target.value);
            }}
            onKeyDown={e => {
              if (e.key === "Enter") handleSubmit();
              if (e.key === "Escape") onClose();
            }}
            placeholder="e.g. Auth system refactor"
            style={{
              width: "100%",
              background: C.base,
              borderRadius: 12,
              padding: "12px 14px",
              color: C.text1,
              fontSize: 13,
              outline: "none", boxSizing: "border-box",
              border: tooShort ? "1px solid rgba(239,68,68,0.4)" : `1px solid ${C.border}`,
              boxShadow: "inset 0 1px 0 rgba(255,255,255,0.03)",
            }}
          />
          <div style={{
            display: "flex", justifyContent: "space-between", marginTop: 6,
          }}>
            <span style={{ fontSize: 10, color: tooShort ? "#EF4444" : C.text4 }}>
              {tooShort ? `At least ${MIN_LEN} characters` : "\u00A0"}
            </span>
            <span style={{
              fontSize: 10, fontFamily: "monospace",
              color: trimmed.length > MAX_LEN - 10 ? C.warn : C.text4,
            }}>
              {trimmed.length}/{MAX_LEN}
            </span>
          </div>

          <div
            style={{
              fontSize: 10,
              fontWeight: 600,
              color: C.text4,
              textTransform: "uppercase",
              letterSpacing: "0.06em",
              marginTop: 16,
              marginBottom: 6,
            }}
          >
            Session Type
          </div>
          <div style={{ display: "flex", gap: 10 }}>
            <button
              type="button"
              onClick={() => setKind("run")}
              style={{
                ...OPTION_CARD,
                border: kind === "run" ? `1px solid ${C.accent}` : OPTION_CARD.border,
                boxShadow: kind === "run" ? `0 0 0 1px ${C.accentDim} inset` : "none",
              }}
            >
              <div
                style={{
                  position: "absolute",
                  inset: "auto auto 0 0",
                  width: 96,
                  height: 96,
                  background: "radial-gradient(circle, rgba(49,185,123,0.18) 0%, rgba(49,185,123,0) 72%)",
                  pointerEvents: "none",
                }}
              />
              <div
                style={{
                  width: 32,
                  height: 32,
                  borderRadius: 8,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  background: C.accentDim,
                  color: C.accent,
                  marginBottom: 10,
                }}
              >
                <StreamIcon size={15} />
              </div>
              <div
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  gap: 6,
                  padding: "3px 8px",
                  marginBottom: 8,
                  borderRadius: 999,
                  fontSize: 10,
                  fontWeight: 700,
                  letterSpacing: "0.04em",
                  textTransform: "uppercase",
                  background: "rgba(49,185,123,0.12)",
                  color: C.accent,
                }}
              >
                Default
              </div>
              <div style={{ fontSize: 12, fontWeight: 700, color: C.text1, marginBottom: 4 }}>
                Bundled run
              </div>
              <div style={{ fontSize: 11, color: C.text3, lineHeight: 1.45 }}>
                Creates the worktree when the first run starts. Uses automatic builder or fixer bundles with validator and judge.
              </div>
            </button>

            <button
              type="button"
              onClick={() => setKind("cli")}
              style={{
                ...OPTION_CARD,
                border: kind === "cli" ? `1px solid ${C.blue}` : OPTION_CARD.border,
                boxShadow: kind === "cli" ? `0 0 0 1px ${C.blueDim} inset` : "none",
              }}
            >
              <div
                style={{
                  position: "absolute",
                  top: -10,
                  right: -8,
                  width: 112,
                  height: 112,
                  background: "radial-gradient(circle, rgba(59,130,246,0.18) 0%, rgba(59,130,246,0) 72%)",
                  pointerEvents: "none",
                }}
              />
              <div
                style={{
                  width: 32,
                  height: 32,
                  borderRadius: 8,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  background: C.blueDim,
                  color: C.blue,
                  marginBottom: 10,
                }}
              >
                <Terminal size={15} />
              </div>
              <div
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  gap: 6,
                  padding: "3px 8px",
                  marginBottom: 8,
                  borderRadius: 999,
                  fontSize: 10,
                  fontWeight: 700,
                  letterSpacing: "0.04em",
                  textTransform: "uppercase",
                  background: "rgba(59,130,246,0.12)",
                  color: C.blue,
                }}
              >
                Live terminal
              </div>
              <div style={{ fontSize: 12, fontWeight: 700, color: C.text1, marginBottom: 4 }}>
                CLI-based
              </div>
              <div style={{ fontSize: 11, color: C.text3, lineHeight: 1.45 }}>
                Creates the worktree immediately and opens the real CLI. No runs or queue are created.
              </div>
            </button>

            <button
              type="button"
              onClick={() => setKind("hive_loom")}
              style={{
                ...OPTION_CARD,
                border: kind === "hive_loom" ? "1px solid #F59E0B" : OPTION_CARD.border,
                boxShadow: kind === "hive_loom" ? "0 0 0 1px rgba(245,158,11,0.2) inset" : "none",
              }}
            >
              <div
                style={{
                  position: "absolute",
                  top: -10,
                  right: -8,
                  width: 112,
                  height: 112,
                  background: "radial-gradient(circle, rgba(245,158,11,0.18) 0%, rgba(245,158,11,0) 72%)",
                  pointerEvents: "none",
                }}
              />
              <div
                style={{
                  width: 32,
                  height: 32,
                  borderRadius: 8,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  background: "rgba(245,158,11,0.12)",
                  color: "#F59E0B",
                  marginBottom: 10,
                  fontSize: 15,
                  fontWeight: 700,
                }}
              >
                <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <circle cx="12" cy="12" r="3" />
                  <path d="M12 1v4M12 19v4M4.22 4.22l2.83 2.83M16.95 16.95l2.83 2.83M1 12h4M19 12h4M4.22 19.78l2.83-2.83M16.95 7.05l2.83-2.83" />
                </svg>
              </div>
              <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 8 }}>
                <div
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 6,
                    padding: "3px 8px",
                    borderRadius: 999,
                    fontSize: 10,
                    fontWeight: 700,
                    letterSpacing: "0.04em",
                    textTransform: "uppercase" as const,
                    background: "rgba(245,158,11,0.12)",
                    color: "#F59E0B",
                  }}
                >
                  Graph DAG
                </div>
                <div
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    padding: "3px 8px",
                    borderRadius: 999,
                    fontSize: 9,
                    fontWeight: 700,
                    letterSpacing: "0.04em",
                    textTransform: "uppercase" as const,
                    background: "rgba(248,113,113,0.1)",
                    color: "#f87171",
                  }}
                >
                  Experimental
                </div>
              </div>
              <div style={{ fontSize: 12, fontWeight: 700, color: C.text1, marginBottom: 4 }}>
                Hive Loom
              </div>
              <div style={{ fontSize: 11, color: C.text3, lineHeight: 1.45 }}>
                Multi-phase graph execution with agents orchestrated by a DAG plan.
              </div>
            </button>
          </div>
        </div>

        {/* Footer */}
        <div style={{
          display: "flex", alignItems: "center", justifyContent: "flex-end",
          padding: "16px 22px", background: C.base, gap: 8, borderTop: `1px solid ${C.border}`,
        }}>
          <button
            onClick={onClose}
            style={{
              padding: "7px 16px", borderRadius: 6,
              background: "transparent",
              color: C.text3, fontSize: 11, fontWeight: 500, cursor: "pointer",
            }}
          >
            Cancel
          </button>
          <button
            onClick={handleSubmit}
            disabled={!valid}
            className="btn-accent"
            style={{
              padding: "7px 20px", borderRadius: 6,
              background: C.accent, color: "#fff",
              fontSize: 11, fontWeight: 700, cursor: "pointer",
              opacity: valid ? 1 : 0.5,
            }}
          >
            Continue
          </button>
        </div>
      </div>
    </div>
  );
}
