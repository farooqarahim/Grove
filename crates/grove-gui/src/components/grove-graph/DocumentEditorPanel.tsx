import { useState, useEffect, useCallback } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { C } from "@/lib/theme";
import { qk } from "@/lib/queryKeys";
import {
  getGraphDocument,
  saveGraphDocument,
  retryDocumentGeneration,
  deleteGraph,
} from "@/lib/api";
import type { GraphRecord } from "@/types";

interface DocumentEditorPanelProps {
  graph: GraphRecord;
  onSaved: () => void;
  onDiscarded: () => void;
}

export function DocumentEditorPanel({
  graph,
  onSaved,
  onDiscarded,
}: DocumentEditorPanelProps) {
  const queryClient = useQueryClient();

  const isDraftReady = graph.parsing_status === "draft_ready";
  const isGenerating = graph.parsing_status === "generating";
  const isError = graph.parsing_status === "error";

  const [title, setTitle] = useState("");
  const [content, setContent] = useState("");
  const [loaded, setLoaded] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [retrying, setRetrying] = useState(false);
  const [discarding, setDiscarding] = useState(false);

  // Listen for Tauri event to trigger immediate invalidation when document is ready
  useEffect(() => {
    const unlisten = listen<{ graph_id: string }>("grove://graph-document-ready", (event) => {
      if (event.payload.graph_id === graph.id) {
        void queryClient.invalidateQueries({ queryKey: qk.graphDetail(graph.id) });
      }
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [graph.id, queryClient]);

  // Load the document when draft_ready
  const { data: doc } = useQuery({
    queryKey: qk.graphDocument(graph.id),
    queryFn: () => getGraphDocument(graph.id),
    enabled: isDraftReady && !loaded,
  });

  // Populate title/content from loaded doc; use loaded flag to avoid reset after user edits
  useEffect(() => {
    if (doc && !loaded) {
      setTitle(doc.title);
      setContent(doc.content);
      setLoaded(true);
    }
  }, [doc, loaded]);

  // Reset loaded flag when parsing_status leaves draft_ready (e.g., retry kicked off)
  useEffect(() => {
    if (!isDraftReady) {
      setLoaded(false);
    }
  }, [isDraftReady]);

  const handleSave = useCallback(async () => {
    setSaving(true);
    setSaveError(null);
    try {
      await saveGraphDocument(graph.id, title, content);
      void queryClient.invalidateQueries({ queryKey: qk.graphDetail(graph.id) });
      void queryClient.invalidateQueries({ queryKey: qk.graphDocument(graph.id) });
      onSaved();
    } catch (e) {
      setSaveError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  }, [graph.id, title, content, queryClient, onSaved]);

  const handleDiscard = useCallback(async () => {
    setDiscarding(true);
    try {
      await deleteGraph(graph.id);
      void queryClient.invalidateQueries({ queryKey: qk.graphDetail(graph.id) });
      onDiscarded();
    } catch {
      // Discard failures are non-fatal — still navigate away
      onDiscarded();
    } finally {
      setDiscarding(false);
    }
  }, [graph.id, queryClient, onDiscarded]);

  const handleRetry = useCallback(async () => {
    setRetrying(true);
    try {
      await retryDocumentGeneration(graph.id);
      void queryClient.invalidateQueries({ queryKey: qk.graphDetail(graph.id) });
    } catch {
      // Error will be reflected via graph polling
    } finally {
      setRetrying(false);
    }
  }, [graph.id, queryClient]);

  const handleErrorDiscard = useCallback(async () => {
    setDiscarding(true);
    try {
      await deleteGraph(graph.id);
      void queryClient.invalidateQueries({ queryKey: qk.graphDetail(graph.id) });
      onDiscarded();
    } catch {
      onDiscarded();
    } finally {
      setDiscarding(false);
    }
  }, [graph.id, queryClient, onDiscarded]);

  // ── State 1: Generating ───────────────────────────────────────────────────
  if (isGenerating) {
    return (
      <>
        <style>{`
          @keyframes grove-spin { to { transform: rotate(360deg); } }
        `}</style>
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            justifyContent: "center",
            height: "100%",
            gap: 16,
            background: C.base,
          }}
        >
          <div
            style={{
              width: 32,
              height: 32,
              borderRadius: "50%",
              border: "2px solid rgba(165,180,252,0.18)",
              borderTopColor: "#A5B4FC",
              animation: "grove-spin 0.85s linear infinite",
            }}
          />
          <div style={{ textAlign: "center", display: "flex", flexDirection: "column", gap: 6 }}>
            <div
              style={{
                fontSize: 13,
                fontWeight: 600,
                color: "#A5B4FC",
              }}
            >
              Generating planning document...
            </div>
            <div
              style={{
                fontSize: 11,
                color: "rgba(165,180,252,0.55)",
              }}
            >
              The coding agent is analyzing your objective
            </div>
          </div>
        </div>
      </>
    );
  }

  // ── State 2: Error ────────────────────────────────────────────────────────
  if (isError) {
    return (
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          height: "100%",
          gap: 16,
          background: C.base,
          padding: "0 24px",
        }}
      >
        {/* Error icon: X in circle */}
        <div
          style={{
            width: 36,
            height: 36,
            borderRadius: "50%",
            background: "rgba(239,68,68,0.12)",
            border: "1.5px solid rgba(239,68,68,0.35)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            flexShrink: 0,
          }}
        >
          <svg
            width={14}
            height={14}
            viewBox="0 0 14 14"
            fill="none"
            stroke="#F87171"
            strokeWidth={2}
            strokeLinecap="round"
          >
            <line x1={3} y1={3} x2={11} y2={11} />
            <line x1={11} y1={3} x2={3} y2={11} />
          </svg>
        </div>

        <div style={{ textAlign: "center", display: "flex", flexDirection: "column", gap: 6 }}>
          <div style={{ fontSize: 13, fontWeight: 600, color: "#F87171" }}>
            Document generation failed
          </div>
          {graph.pipeline_error && (
            <div
              style={{
                fontSize: 11,
                color: "rgba(248,113,113,0.70)",
                maxWidth: 360,
                lineHeight: 1.5,
              }}
            >
              {graph.pipeline_error}
            </div>
          )}
        </div>

        <div style={{ display: "flex", gap: 8, marginTop: 4 }}>
          <button
            onClick={() => void handleRetry()}
            disabled={retrying || discarding}
            style={{
              padding: "7px 16px",
              borderRadius: 6,
              background: retrying ? C.surfaceHover : C.accent,
              border: "none",
              color: retrying ? "rgba(255,255,255,0.35)" : "#fff",
              fontSize: 12,
              fontWeight: 600,
              cursor: retrying || discarding ? "default" : "pointer",
            }}
          >
            {retrying ? "Retrying…" : "Retry"}
          </button>
          <button
            onClick={() => void handleErrorDiscard()}
            disabled={retrying || discarding}
            style={{
              padding: "7px 16px",
              borderRadius: 6,
              background: "transparent",
              border: `1px solid ${C.border}`,
              color: discarding ? "rgba(255,255,255,0.30)" : "rgba(255,255,255,0.55)",
              fontSize: 12,
              fontWeight: 500,
              cursor: retrying || discarding ? "default" : "pointer",
            }}
          >
            {discarding ? "Discarding…" : "Discard"}
          </button>
        </div>
      </div>
    );
  }

  // ── State 3: Draft Ready ──────────────────────────────────────────────────
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100%",
        background: C.base,
        overflow: "hidden",
      }}
    >
      {/* Title input */}
      <div
        style={{
          padding: "10px 14px 0 14px",
          flexShrink: 0,
        }}
      >
        <input
          type="text"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder="Document title…"
          style={{
            width: "100%",
            background: "transparent",
            border: "none",
            borderBottom: `1px solid ${C.border}`,
            outline: "none",
            color: C.text1,
            fontSize: 14,
            fontWeight: 700,
            padding: "4px 0 8px 0",
            boxSizing: "border-box",
            lineHeight: 1.4,
          }}
        />
      </div>

      {/* Markdown textarea */}
      <div style={{ flex: 1, overflow: "hidden", padding: "10px 14px" }}>
        <textarea
          value={content}
          onChange={(e) => setContent(e.target.value)}
          placeholder="Planning document content (Markdown)…"
          style={{
            width: "100%",
            height: "100%",
            background: C.surface,
            border: `1px solid ${C.border}`,
            borderRadius: 6,
            padding: "10px 12px",
            color: C.text1,
            fontSize: 12,
            fontFamily: C.mono,
            resize: "none",
            outline: "none",
            lineHeight: 1.65,
            boxSizing: "border-box",
          }}
        />
      </div>

      {/* Save error */}
      {saveError && (
        <div
          style={{
            padding: "0 14px 6px 14px",
            fontSize: 11,
            color: "#F87171",
            flexShrink: 0,
          }}
        >
          {saveError}
        </div>
      )}

      {/* Footer */}
      <div
        style={{
          display: "flex",
          justifyContent: "flex-end",
          alignItems: "center",
          gap: 8,
          padding: "10px 14px",
          borderTop: `1px solid ${C.border}`,
          background: C.surface,
          flexShrink: 0,
        }}
      >
        <button
          onClick={() => void handleDiscard()}
          disabled={saving || discarding}
          style={{
            padding: "6px 14px",
            borderRadius: 5,
            background: "transparent",
            border: `1px solid ${C.border}`,
            color: discarding ? "rgba(255,255,255,0.28)" : "rgba(255,255,255,0.55)",
            fontSize: 11,
            fontWeight: 500,
            cursor: saving || discarding ? "default" : "pointer",
          }}
        >
          {discarding ? "Discarding…" : "Discard"}
        </button>
        <button
          onClick={() => void handleSave()}
          disabled={saving || discarding || !title.trim()}
          style={{
            padding: "6px 16px",
            borderRadius: 5,
            background:
              saving || discarding || !title.trim() ? C.surfaceHover : C.accent,
            border: "none",
            color:
              saving || discarding || !title.trim()
                ? "rgba(255,255,255,0.28)"
                : "#fff",
            fontSize: 11,
            fontWeight: 600,
            cursor:
              saving || discarding || !title.trim() ? "default" : "pointer",
          }}
        >
          {saving ? "Saving…" : "Save & Continue"}
        </button>
      </div>
    </div>
  );
}
