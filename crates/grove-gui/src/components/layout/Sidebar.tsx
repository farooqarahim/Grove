import { useState, useEffect, useRef, useCallback } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, Search, ChevronDown, Check, Gear, Home } from "@/components/ui/icons";
import { relativeTime } from "@/lib/hooks";
import { qk } from "@/lib/queryKeys";
import { listConversations, getConversation, listRunsForConversation } from "@/lib/api";
import type { ProjectRow } from "@/types";
import { C } from "@/lib/theme";
import { ProjectSettingsPanel } from "@/components/settings/ProjectSettingsPanel";

interface SidebarProps {
  selectedConversationId: string | null;
  onSelectConversation: (id: string | null) => void;
  onNewRun: () => void;
  projects: ProjectRow[];
  selectedProjectId: string | null;
  onSelectProject: (id: string) => void;
  onCreateProject: () => void;
}

export function Sidebar({
  selectedConversationId,
  onSelectConversation,
  onNewRun,
  projects,
  selectedProjectId,
  onSelectProject,
  onCreateProject,
}: SidebarProps) {
  const queryClient = useQueryClient();
  const [search, setSearch] = useState("");
  const [projectDropdownOpen, setProjectDropdownOpen] = useState(false);
  const [projectSearch, setProjectSearch] = useState("");
  const [showProjectSettings, setShowProjectSettings] = useState(false);
  const [convLimit, setConvLimit] = useState(100);
  const dropdownRef = useRef<HTMLDivElement>(null);
  const projectSearchRef = useRef<HTMLInputElement>(null);

  // Prefetch conversation + runs on hover so data is cached before click
  const prefetchConversation = useCallback((convId: string) => {
    queryClient.prefetchQuery({
      queryKey: qk.conversation(convId),
      queryFn: () => getConversation(convId),
      staleTime: 30000,
    });
    queryClient.prefetchQuery({
      queryKey: qk.runsForConversation(convId),
      queryFn: () => listRunsForConversation(convId),
      staleTime: 30000,
    });
  }, [queryClient]);

  const { data: conversations } = useQuery({
    queryKey: qk.conversations(selectedProjectId, convLimit),
    queryFn: () => listConversations(convLimit, selectedProjectId),
    refetchInterval: 60000,
    staleTime: 30000,
  });
  useEffect(() => {
    if (!projectDropdownOpen) {
      setProjectSearch("");
      return;
    }
    setTimeout(() => projectSearchRef.current?.focus(), 30);
    const handler = (e: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(e.target as Node)) {
        setProjectDropdownOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [projectDropdownOpen]);

  const selectedProject = projects.find(p => p.id === selectedProjectId) ?? null;

  const projectFiltered = conversations?.filter(c => {
    if (!selectedProjectId) return true;
    return c.project_id === selectedProjectId;
  });

  const filtered = projectFiltered?.filter((c) => {
    if (!search) return true;
    const term = search.toLowerCase();
    return (c.title?.toLowerCase().includes(term) ?? false) || c.id.toLowerCase().includes(term);
  });

  return (
    <div className="w-full h-full flex flex-col" style={{ background: C.sidebar }}>

      {/* Project Switcher */}
      <div className="relative" style={{ padding: "16px 16px 12px" }} ref={dropdownRef}>
        <div style={{ fontSize: 10, color: C.text4, textTransform: "uppercase", letterSpacing: "0.06em", fontWeight: 600, marginBottom: 6, opacity: 0.6 }}>Project</div>
        <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
          <button
            onClick={() => setProjectDropdownOpen(!projectDropdownOpen)}
            className="flex items-center justify-between cursor-pointer text-left transition-colors"
            style={{
              flex: 1, background: "rgba(255,255,255,0.04)",
              borderRadius: 2,
              padding: "8px 12px", color: C.text1, fontSize: 12, fontWeight: 500,
              border: "none", minWidth: 0,
            }}
          >
            <span className="overflow-hidden text-ellipsis whitespace-nowrap">
              {selectedProject?.name ?? "Select project"}
            </span>
            <span
              className="shrink-0 ml-1.5 transition-transform"
              style={{
                color: C.text4,
                transform: projectDropdownOpen ? "rotate(180deg)" : "none",
              }}
            >
              <ChevronDown size={10} />
            </span>
          </button>
          {selectedProjectId && (
            <>
              <button
                onClick={() => { setProjectDropdownOpen(false); onSelectConversation(null); }}
                title="Project home"
                style={{
                  flexShrink: 0, padding: "0 4px", height: "100%",
                  background: "none", border: "none", cursor: "pointer",
                  color: !selectedConversationId ? C.accent : C.text4,
                  display: "flex", alignItems: "center",
                  transition: "color 0.12s",
                }}
                onMouseEnter={e => { if (selectedConversationId) e.currentTarget.style.color = C.text2; }}
                onMouseLeave={e => { if (selectedConversationId) e.currentTarget.style.color = C.text4; }}
              >
                <Home size={11} />
              </button>
              <button
                onClick={() => { setProjectDropdownOpen(false); setShowProjectSettings(true); }}
                title="Project settings"
                style={{
                  flexShrink: 0, padding: "0 4px", height: "100%",
                  background: "none", border: "none", cursor: "pointer",
                  color: showProjectSettings ? C.accent : C.text4,
                  display: "flex", alignItems: "center",
                  transition: "color 0.12s",
                }}
                onMouseEnter={e => { if (!showProjectSettings) e.currentTarget.style.color = C.text2; }}
                onMouseLeave={e => { if (!showProjectSettings) e.currentTarget.style.color = C.text4; }}
              >
                <Gear size={11} />
              </button>
            </>
          )}
        </div>

        {/* Dropdown */}
        {projectDropdownOpen && (
          <div
            className="absolute left-0 right-0 z-50"
            style={{
              top: "calc(100% + 4px)",
              background: "#16191F",
              border: `1px solid rgba(255,255,255,0.07)`,
              borderRadius: 6,
              boxShadow: "0 8px 32px rgba(0,0,0,0.5), 0 2px 8px rgba(0,0,0,0.3)",
              overflow: "hidden",
              display: "flex",
              flexDirection: "column",
              maxHeight: 320,
            }}
          >
            {/* Search — fixed at top */}
            <div style={{
              padding: "10px 10px 8px",
              borderBottom: "1px solid rgba(255,255,255,0.05)",
              flexShrink: 0,
            }}>
              <div style={{
                display: "flex", alignItems: "center", gap: 8,
                background: "rgba(255,255,255,0.05)",
                borderRadius: 4,
                padding: "6px 10px",
              }}>
                <Search size={11} />
                <input
                  ref={projectSearchRef}
                  type="text"
                  placeholder="Search projects…"
                  value={projectSearch}
                  onChange={e => setProjectSearch(e.target.value)}
                  onKeyDown={e => e.key === "Escape" && setProjectDropdownOpen(false)}
                  style={{
                    flex: 1, background: "transparent", border: "none",
                    outline: "none", fontSize: 12, color: C.text1,
                    fontFamily: "inherit",
                  }}
                />
                {projectSearch && (
                  <button
                    onClick={() => setProjectSearch("")}
                    style={{ background: "none", border: "none", cursor: "pointer", color: C.text4, padding: 0, lineHeight: 1 }}
                  >
                    ×
                  </button>
                )}
              </div>
            </div>

            {/* Project list — only this scrolls */}
            <div style={{ overflowY: "auto", flex: 1 }}>
              {(() => {
                const term = projectSearch.toLowerCase();
                const visible = projects
                  .filter(p => p.state === "active")
                  .filter(p => !term || (p.name || p.root_path.split("/").pop() || "").toLowerCase().includes(term));

                if (visible.length === 0) {
                  return (
                    <div style={{ padding: "16px 14px", fontSize: 12, color: C.text4, textAlign: "center" }}>
                      No projects match "{projectSearch}"
                    </div>
                  );
                }

                return visible.map(p => {
                  const isSelected = p.id === selectedProjectId;
                  const name = p.name || p.root_path.split("/").pop() || p.id;
                  const path = p.root_path.replace(/^\/Users\/[^/]+/, "~");
                  return (
                    <button
                      key={p.id}
                      onClick={() => { onSelectProject(p.id); setProjectDropdownOpen(false); }}
                      style={{
                        width: "100%", padding: "9px 12px",
                        background: isSelected ? "rgba(49,185,123,0.08)" : "transparent",
                        border: "none", cursor: "pointer", textAlign: "left",
                        display: "flex", alignItems: "center", gap: 10,
                        transition: "background 0.12s",
                      }}
                      onMouseEnter={e => { if (!isSelected) e.currentTarget.style.background = "rgba(255,255,255,0.04)"; }}
                      onMouseLeave={e => { if (!isSelected) e.currentTarget.style.background = "transparent"; }}
                    >
                      <div style={{ flex: 1, minWidth: 0 }}>
                        <div style={{
                          fontSize: 12, fontWeight: isSelected ? 600 : 400,
                          color: isSelected ? C.text1 : C.text2,
                          overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                        }}>
                          {name}
                        </div>
                        <div style={{
                          fontSize: 10.5, color: C.text4, marginTop: 1,
                          overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                          fontFamily: "'JetBrains Mono', monospace",
                        }}>
                          {path}
                        </div>
                      </div>
                      {isSelected && <Check size={10} />}
                    </button>
                  );
                });
              })()}
            </div>

            {/* New Project — fixed at bottom */}
            <div style={{ borderTop: "1px solid rgba(255,255,255,0.05)", flexShrink: 0 }}>
              <button
                onClick={() => { setProjectDropdownOpen(false); onCreateProject(); }}
                style={{
                  width: "100%", padding: "10px 14px",
                  background: "transparent", border: "none", cursor: "pointer",
                  display: "flex", alignItems: "center", gap: 8,
                  color: C.accent, fontSize: 12, fontWeight: 600,
                  fontFamily: "inherit", textAlign: "left",
                  transition: "background 0.12s",
                }}
                onMouseEnter={e => { e.currentTarget.style.background = "rgba(49,185,123,0.07)"; }}
                onMouseLeave={e => { e.currentTarget.style.background = "transparent"; }}
              >
                <Plus size={10} /> New Project
              </button>
            </div>
          </div>
        )}
      </div>

      {/* Search */}
      <div style={{ padding: "0 16px 10px" }}>
        <div
          className="flex items-center gap-1.5 text-sm"
          style={{
            padding: "6px 12px", borderRadius: 2,
            background: "rgba(255,255,255,0.03)", color: C.text4,
          }}
        >
          <Search size={11} />
          <input
            type="text"
            placeholder="Search..."
            value={search}
            onChange={e => setSearch(e.target.value)}
            className="flex-1 bg-transparent outline-none text-sm p-0"
            style={{ border: "none", color: C.text2 }}
          />
          <span
            className="text-2xs rounded-sm"
            style={{
              background: "rgba(255,255,255,0.04)",
              padding: "2px 6px", color: C.text4,
            }}
          >
            {"\u2318K"}
          </span>
        </div>
      </div>

      {/* Sessions label */}
      <div className="flex items-center justify-between" style={{ padding: "10px 16px 6px" }}>
        <span style={{ fontSize: 10, color: C.text4, textTransform: "uppercase", letterSpacing: "0.06em", fontWeight: 600 }}>Sessions</span>
        <span style={{
          fontSize: 10, color: C.text4,
          background: "rgba(255,255,255,0.04)",
          borderRadius: 2, padding: "1px 6px",
        }}>{filtered?.length ?? 0}</span>
      </div>

      {/* Session list */}
      <div className="smooth-scroll flex-1 overflow-y-auto" style={{ padding: "0 8px 8px" }}>
        {/* Loading skeleton */}
        {!conversations && (
          <div className="flex flex-col gap-1.5" style={{ padding: "8px 4px" }}>
            {[1, 2, 3, 4].map(i => (
              <div key={i} style={{ padding: "10px 12px", borderRadius: 2 }}>
                <div className="skeleton" style={{ height: 12, width: "70%", marginBottom: 8 }} />
                <div className="skeleton" style={{ height: 10, width: "50%" }} />
              </div>
            ))}
          </div>
        )}
        {conversations && (!filtered || filtered.length === 0) && (
          <div className="text-center" style={{ padding: "48px 16px" }}>
            <div className="text-sm mb-2" style={{ color: C.text4 }}>
              {conversations.length > 0 ? "No matches" : "No sessions yet"}
            </div>
            {conversations.length === 0 && (
              <button
                onClick={onNewRun}
                className="btn-accent text-xs font-semibold cursor-pointer"
                style={{
                  padding: "6px 14px", borderRadius: 2,
                  background: C.accentDim, color: C.accent,
                  border: "none",
                }}
              >
                Start your first bundled run
              </button>
            )}
          </div>
        )}
        {filtered?.map((conv) => {
          const active = selectedConversationId === conv.id;
          const isRunning = ["executing", "waiting_for_gate", "planning", "verifying", "publishing", "merging"].includes(conv.state);

          const kindColor =
            conv.conversation_kind === "cli" ? C.blue
            : conv.conversation_kind === "hive_loom" ? "#F59E0B"
            : C.accent;
          const kindBg =
            conv.conversation_kind === "cli" ? C.blueDim
            : conv.conversation_kind === "hive_loom" ? "rgba(245,158,11,0.12)"
            : C.accentDim;
          const kindLabel =
            conv.conversation_kind === "cli" ? "CLI"
            : conv.conversation_kind === "hive_loom" ? "HIVE"
            : "RUN";

          return (
            <button
              key={conv.id}
              onClick={() => onSelectConversation(conv.id)}
              onMouseEnter={() => prefetchConversation(conv.id)}
              className={`session-item ${active ? "active" : ""} w-full text-left cursor-pointer`}
              style={{
                padding: "10px 12px",
                borderRadius: 8,
                marginBottom: 2,
                background: active ? "rgba(255,255,255,0.06)" : "transparent",
                border: active ? `1px solid ${C.border}` : "1px solid transparent",
                transition: "all 120ms ease",
                position: "relative",
              }}
            >
              {/* Running indicator — left edge accent bar */}
              {isRunning && (
                <span style={{
                  position: "absolute", left: 0, top: 8, bottom: 8,
                  width: 2, borderRadius: 1, background: kindColor,
                  boxShadow: `0 0 6px ${kindColor}60`,
                }} />
              )}

              {/* Row 1: Title + kind tag */}
              <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                <span
                  style={{
                    flex: 1, overflow: "hidden", textOverflow: "ellipsis",
                    whiteSpace: "nowrap", fontSize: 13, fontWeight: 500,
                    color: active ? "#fff" : "rgba(255,255,255,0.82)",
                  }}
                >
                  {conv.title || `Session ${conv.id.slice(0, 8)}`}
                </span>
                <span
                  style={{
                    flexShrink: 0, fontSize: 9, fontWeight: 700,
                    letterSpacing: "0.05em", textTransform: "uppercase",
                    color: kindColor, background: kindBg,
                    padding: "1px 5px", borderRadius: 3, lineHeight: "14px",
                  }}
                >
                  {kindLabel}
                </span>
              </div>

              {/* Row 2: Meta info */}
              <div style={{
                display: "flex", alignItems: "center", gap: 4,
                marginTop: 4, fontSize: 10, color: "rgba(255,255,255,0.35)",
                lineHeight: 1,
              }}>
                {conv.conversation_kind === "cli" && conv.cli_provider && (
                  <>
                    <span style={{ color: "rgba(59,130,246,0.7)", fontWeight: 500 }}>
                      {conv.cli_provider}
                    </span>
                    {conv.cli_model && (
                      <>
                        <span style={{ opacity: 0.4 }}>·</span>
                        <span>{conv.cli_model}</span>
                      </>
                    )}
                    <span style={{ opacity: 0.4 }}>·</span>
                  </>
                )}
                {isRunning && (
                  <>
                    <span style={{
                      display: "inline-block", width: 5, height: 5,
                      borderRadius: "50%", background: kindColor, flexShrink: 0,
                      boxShadow: `0 0 4px ${kindColor}80`,
                    }} />
                    <span style={{ color: kindColor, fontWeight: 600, fontSize: 9 }}>
                      Active
                    </span>
                    <span style={{ opacity: 0.4 }}>·</span>
                  </>
                )}
                <span>{relativeTime(conv.updated_at)}</span>
              </div>
            </button>
          );
        })}
        {conversations && conversations.length >= convLimit && (
          <button
            onClick={() => setConvLimit(prev => prev + 100)}
            className="btn-ghost w-full text-center text-xs cursor-pointer"
            style={{
              padding: "8px 0", marginTop: 4,
              background: "transparent", border: "none",
              color: C.text4,
            }}
          >
            Load more sessions...
          </button>
        )}
      </div>

      {/* New Session button */}
      <div style={{ padding: "12px 12px" }}>
        <button
          onClick={onNewRun}
          className="btn-accent w-full flex items-center justify-center gap-1.5 text-base font-semibold cursor-pointer"
          style={{
            padding: "10px 0", borderRadius: 2,
            background: C.surfaceRaised, color: "#FFFFFF",
            border: "none",
          }}
        >
          <Plus size={11} /> New Session
        </button>
      </div>

      {/* Project Settings Modal */}
      {showProjectSettings && selectedProjectId && (
        <div
          onClick={() => setShowProjectSettings(false)}
          style={{
            position: "fixed", inset: 0, zIndex: 500,
            background: "rgba(0,0,0,0.6)", backdropFilter: "blur(4px)",
            display: "flex", alignItems: "center", justifyContent: "center", padding: 20,
          }}
        >
          <div
            onClick={e => e.stopPropagation()}
            style={{
              background: C.surface, border: `1px solid ${C.border}`, borderRadius: 12,
              width: "100%", maxWidth: 560, maxHeight: "80vh", overflowY: "auto",
              boxShadow: "0 25px 80px rgba(0,0,0,0.5)",
            }}
          >
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", padding: "16px 24px", borderBottom: `1px solid ${C.border}` }}>
              <div>
                <div style={{ fontSize: 14, fontWeight: 700, color: C.text1 }}>Project Settings</div>
                <div style={{ fontSize: 11, color: C.text4, marginTop: 2 }}>
                  {projects.find(p => p.id === selectedProjectId)?.name ?? selectedProjectId}
                </div>
              </div>
              <button
                onClick={() => setShowProjectSettings(false)}
                style={{
                  background: "transparent", border: "none", color: C.text4,
                  cursor: "pointer", fontSize: 20, lineHeight: 1, padding: "0 4px",
                }}
              >
                ×
              </button>
            </div>
            <ProjectSettingsPanel
              projectId={selectedProjectId}
              projectName={projects.find(p => p.id === selectedProjectId)?.name}
              rootPath={projects.find(p => p.id === selectedProjectId)?.root_path}
              compact
              onSaved={() => setShowProjectSettings(false)}
            />
          </div>
        </div>
      )}
    </div>
  );
}
