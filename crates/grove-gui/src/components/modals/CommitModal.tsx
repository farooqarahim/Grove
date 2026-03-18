import { useState } from "react";
import { Commit, PullRequest, Upload, GitBranch, XIcon, Check, Sparkles, Loader } from "@/components/ui/icons";
import { C } from "@/lib/theme";
import { gitGeneratePrContent, gitProjectGeneratePrContent } from "@/lib/api";

interface CommitModalProps {
  open: boolean;
  onClose: () => void;
  branch: string;
  fileCount: number;
  additions: number;
  removals: number;
  runId: string | null;
  projectRoot: string | null;
  onCommit?: (message: string, nextStep: string, includeUnstaged: boolean, prTitle?: string, prBody?: string) => void;
}

export function CommitModal({
  open,
  onClose,
  branch,
  fileCount,
  additions,
  removals,
  runId,
  projectRoot,
  onCommit,
}: CommitModalProps) {
  const [message, setMessage] = useState("");
  const [includeUnstaged, setIncludeUnstaged] = useState(true);
  // Default to "pr" when in a run context (most common intent), "push" for project commits
  const [nextStep, setNextStep] = useState(runId ? "pr" : "push");
  const [prTitle, setPrTitle] = useState("");
  const [prBody, setPrBody] = useState("");
  const [generating, setGenerating] = useState(false);

  if (!open) return null;

  const canGenerate = !!runId || !!projectRoot;
  const handleGenerate = async () => {
    if (!runId && !projectRoot) return;
    setGenerating(true);
    try {
      const content = runId
        ? await gitGeneratePrContent(runId)
        : await gitProjectGeneratePrContent(projectRoot!);
      setPrTitle(content.title);
      setPrBody(content.description);
      if (!message.trim()) {
        setMessage(content.title);
      }
    } catch (e) {
      console.error("PR content generation failed:", e);
    } finally {
      setGenerating(false);
    }
  };

  const steps: [string, React.ReactNode, string][] = [
    ["commit", <Commit size={13} />, "Commit"],
    ["push", <Upload size={13} />, "Commit and push"],
    ["pr", <PullRequest size={13} />, "Commit and create PR"],
  ];

  const handleContinue = () => {
    onCommit?.(message, nextStep, includeUnstaged, prTitle || undefined, prBody || undefined);
    onClose();
  };

  return (
    <div
      onClick={onClose}
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 200,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "rgba(0,0,0,0.5)",
        backdropFilter: "blur(8px)",
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          width: 380,
          background: C.surface,
          borderRadius: 10,
          overflow: "hidden",
        }}
      >
        {/* Header */}
        <div
          style={{
            padding: "18px 22px 14px",
            display: "flex",
            justifyContent: "space-between",
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <div
              style={{
                width: 30,
                height: 30,
                borderRadius: 6,
                background: "rgba(255,255,255,0.04)",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
              }}
            >
              <Commit size={15} />
            </div>
            <span style={{ fontSize: 15, fontWeight: 700, color: C.text1 }}>
              Commit your changes
            </span>
          </div>
          <button
            onClick={onClose}
            style={{
              background: "none",
              color: C.text4,
              cursor: "pointer",
            }}
          >
            <XIcon size={12} />
          </button>
        </div>

        {/* Body */}
        <div style={{ padding: "0 22px 18px" }}>
          {/* Branch */}
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              marginBottom: 10,
              fontSize: 12,
            }}
          >
            <span style={{ color: C.text3 }}>Branch</span>
            <span style={{ display: "flex", alignItems: "center", gap: 4 }}>
              <span style={{ color: C.accent }}>
                <GitBranch size={11} />
              </span>
              <span style={{ fontFamily: C.mono, fontSize: 11, color: C.text2 }}>
                {branch}
              </span>
            </span>
          </div>

          {/* Changes */}
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              marginBottom: 14,
              fontSize: 12,
            }}
          >
            <span style={{ color: C.text3 }}>Changes</span>
            <span>
              {fileCount} files{" "}
              <span style={{ color: "#31B97B" }}>+{additions}</span>{" "}
              <span style={{ color: "#EF4444" }}>-{removals}</span>
            </span>
          </div>

          {/* Include unstaged toggle */}
          <label
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              marginBottom: 14,
              cursor: "pointer",
            }}
          >
            <span style={{ position: "relative", width: 32, height: 18, flexShrink: 0 }}>
              <input
                type="checkbox"
                checked={includeUnstaged}
                onChange={(e) => setIncludeUnstaged(e.target.checked)}
                style={{
                  position: "absolute",
                  opacity: 0,
                  width: 32,
                  height: 18,
                  cursor: "pointer",
                }}
              />
              <span
                style={{
                  position: "absolute",
                  inset: 0,
                  borderRadius: 6,
                  background: includeUnstaged ? "rgba(59,130,246,0.5)" : "rgba(255,255,255,0.1)",
                  transition: "background 0.15s",
                }}
              />
              <span
                style={{
                  position: "absolute",
                  top: 2,
                  left: includeUnstaged ? 16 : 2,
                  width: 14,
                  height: 14,
                  borderRadius: 4,
                  background: includeUnstaged ? "#3B82F6" : C.text4,
                  transition: "left 0.15s",
                }}
              />
            </span>
            <span style={{ fontSize: 12, color: C.text2 }}>Include unstaged</span>
          </label>

          {/* Commit message */}
          <div style={{ fontSize: 12, color: C.text2, fontWeight: 500, marginBottom: 6 }}>
            Commit message
          </div>
          <textarea
            placeholder="Leave blank to autogenerate"
            value={message}
            onChange={(e) => setMessage(e.target.value)}
            style={{
              width: "100%",
              height: 64,
              background: C.base,
              borderRadius: 6,
              padding: "8px 12px",
              color: C.text1,
              fontSize: 12,
              resize: "none",
              outline: "none",
              boxSizing: "border-box",
            }}
          />

          {/* Next steps */}
          <div style={{ fontSize: 12, color: C.text3, fontWeight: 500, margin: "14px 0 6px" }}>
            Next steps
          </div>
          {steps.map(([id, icon, label]) => (
            <div
              key={id}
              onClick={() => setNextStep(id)}
              className="hover-row"
              style={{
                display: "flex",
                alignItems: "center",
                gap: 8,
                padding: "8px 10px",
                borderRadius: 6,
                cursor: "pointer",
                marginBottom: 1,
              }}
            >
              <span style={{ color: C.text3 }}>{icon}</span>
              <span style={{ flex: 1, fontSize: 12, color: C.text2, fontWeight: 500 }}>
                {label}
              </span>
              {nextStep === id && (
                <span style={{ color: C.accent }}>
                  <Check size={12} />
                </span>
              )}
            </div>
          ))}

          {/* PR title/body when PR is selected */}
          {nextStep === "pr" && (
            <div style={{ marginTop: 12 }}>
              <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 6 }}>
                <span style={{ fontSize: 12, color: C.text2, fontWeight: 500 }}>PR details</span>
                <button
                  onClick={handleGenerate}
                  disabled={generating || !canGenerate}
                  style={{
                    display: "flex", alignItems: "center", gap: 4,
                    padding: "3px 8px", borderRadius: 6,
                    background: "rgba(59,130,246,0.08)",
                    color: "#3B82F6", fontSize: 10, fontWeight: 500,
                    cursor: generating ? "default" : "pointer",
                    opacity: generating ? 0.6 : 1,
                  }}
                >
                  {generating ? <Loader size={10} /> : <Sparkles size={10} />}
                  {generating ? "Generating..." : "Auto-generate"}
                </button>
              </div>
              <input
                type="text"
                placeholder="PR title"
                value={prTitle}
                onChange={(e) => setPrTitle(e.target.value)}
                style={{
                  width: "100%", padding: "7px 12px",
                  background: C.base,
                  borderRadius: 6, color: C.text1, fontSize: 12,
                  outline: "none", boxSizing: "border-box",
                  marginBottom: 6,
                }}
              />
              <textarea
                placeholder="PR description (markdown)"
                value={prBody}
                onChange={(e) => setPrBody(e.target.value)}
                style={{
                  width: "100%", height: 80,
                  background: C.base,
                  borderRadius: 6, padding: "8px 12px",
                  color: C.text1, fontSize: 12,
                  resize: "vertical", outline: "none",
                  boxSizing: "border-box",
                }}
              />
            </div>
          )}
        </div>

        {/* Footer */}
        <div
          style={{
            padding: "10px 22px",
            background: C.base,
          }}
        >
          <button
            onClick={handleContinue}
            style={{
              width: "100%",
              padding: "9px 0",
              borderRadius: 6,
              background: C.text1,
              color: C.base,
              fontSize: 13,
              fontWeight: 600,
              cursor: "pointer",
            }}
          >
            Continue
          </button>
        </div>
      </div>
    </div>
  );
}
