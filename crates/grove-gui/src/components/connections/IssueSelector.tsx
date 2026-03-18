import { useState, useEffect, useRef } from "react";
import { searchIssues, fetchReadyIssues } from "@/lib/api";
import { C } from "@/lib/theme";
import type { Issue } from "@/types";

interface IssueSelectorProps {
  onSelect: (issue: Issue | null) => void;
  selected: Issue | null;
}

export function IssueSelector({ onSelect, selected }: IssueSelectorProps) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Issue[]>([]);
  const [loading, setLoading] = useState(false);
  const [open, setOpen] = useState(false);
  const [mode, setMode] = useState<"search" | "ready">("search");
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // Debounced search
  useEffect(() => {
    if (mode !== "search") return;
    if (debounceRef.current) clearTimeout(debounceRef.current);
    if (!query.trim()) {
      setResults([]);
      return;
    }
    debounceRef.current = setTimeout(async () => {
      setLoading(true);
      try {
        const issues = await searchIssues(query.trim(), null, 15);
        setResults(issues);
      } catch {
        setResults([]);
      }
      setLoading(false);
    }, 300);
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [query, mode]);

  // Fetch ready issues
  useEffect(() => {
    if (mode !== "ready") return;
    setLoading(true);
    fetchReadyIssues()
      .then(setResults)
      .catch(() => setResults([]))
      .finally(() => setLoading(false));
  }, [mode]);

  // Close dropdown on outside click
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  if (selected) {
    return (
      <div style={{
        display: "flex", alignItems: "center", gap: 8,
        padding: "6px 10px", borderRadius: 6,
        background: "rgba(99,102,241,0.08)",


      }}>
        <span style={{
          fontSize: 10, fontWeight: 700, color: C.accent,
          fontFamily: C.mono,
        }}>
          #{selected.external_id}
        </span>
        <span style={{
          fontSize: 11, color: C.text2, flex: 1,
          overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
        }}>
          {selected.title}
        </span>
        <span style={{
          fontSize: 9, padding: "1px 5px", borderRadius: 6,
          background: "rgba(99,102,241,0.15)", color: "#818CF8",
        }}>
          {selected.provider}
        </span>
        <button
          onClick={() => onSelect(null)}
          style={{
            background: "none", border: "none",
            color: C.text4, cursor: "pointer", fontSize: 10,
            padding: "2px 4px",
          }}
        >
          Clear
        </button>
      </div>
    );
  }

  return (
    <div ref={containerRef} style={{ position: "relative" }}>
      <div style={{ display: "flex", gap: 6, marginBottom: 4 }}>
        <button
          onClick={() => { setMode("search"); setResults([]); setQuery(""); }}
          style={{
            padding: "2px 8px", borderRadius: 6, fontSize: 9,
            background: mode === "search" ? C.accent : "transparent",
            color: mode === "search" ? "#fff" : C.text4,
            cursor: "pointer", fontWeight: 600,
          }}
        >
          Search
        </button>
        <button
          onClick={() => { setMode("ready"); setQuery(""); }}
          style={{
            padding: "2px 8px", borderRadius: 6, fontSize: 9,
            background: mode === "ready" ? C.accent : "transparent",
            color: mode === "ready" ? "#fff" : C.text4,
            cursor: "pointer", fontWeight: 600,
          }}
        >
          Ready
        </button>
      </div>
      {mode === "search" && (
        <input
          value={query}
          onChange={e => { setQuery(e.target.value); setOpen(true); }}
          onFocus={() => setOpen(true)}
          placeholder="Search issues by title or ID..."
          style={{
            width: "100%", padding: "5px 8px", borderRadius: 6,
            background: C.base,
            color: C.text2, fontSize: 11, outline: "none",
            fontFamily: C.mono, boxSizing: "border-box",
          }}
        />
      )}

      {(open || mode === "ready") && (results.length > 0 || loading) && (
        <div style={{
          position: mode === "search" ? "absolute" : "relative",
          top: mode === "search" ? "100%" : 0,
          left: 0, right: 0, zIndex: 20,
          marginTop: 4,
          background: C.surfaceHover,
          borderRadius: 6,
          maxHeight: 200, overflowY: "auto",
        }}>
          {loading && (
            <div style={{ padding: "8px 10px", fontSize: 10, color: C.text4 }}>
              Searching...
            </div>
          )}
          {results.map(issue => (
            <button
              key={`${issue.provider}-${issue.external_id}`}
              onClick={() => { onSelect(issue); setOpen(false); setQuery(""); }}
              style={{
                display: "flex", alignItems: "center", gap: 8,
                width: "100%", padding: "7px 10px", background: "transparent",


                cursor: "pointer", textAlign: "left",
              }}
              onMouseEnter={e => (e.currentTarget.style.background = C.surfaceHover)}
              onMouseLeave={e => (e.currentTarget.style.background = "transparent")}
            >
              <span style={{
                fontSize: 10, fontWeight: 700, color: C.accent,
                fontFamily: C.mono, flexShrink: 0, minWidth: 50,
              }}>
                #{issue.external_id}
              </span>
              <span style={{
                fontSize: 11, color: C.text2, flex: 1,
                overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
              }}>
                {issue.title}
              </span>
              <span style={{
                fontSize: 9, padding: "1px 5px", borderRadius: 6, flexShrink: 0,
                background: issue.status === "open" ? "rgba(49,185,123,0.15)" : "rgba(156,163,175,0.15)",
                color: issue.status === "open" ? "#31B97B" : C.text4,
              }}>
                {issue.status}
              </span>
              {issue.assignee && (
                <span style={{ fontSize: 9, color: C.text4, flexShrink: 0 }}>
                  {issue.assignee}
                </span>
              )}
              <span style={{
                fontSize: 8, color: C.text4, fontFamily: C.mono, flexShrink: 0,
              }}>
                {issue.provider}
              </span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
