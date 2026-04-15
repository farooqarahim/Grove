import { useState, useEffect, useRef, useCallback } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Search, ChevronDown, Check, Gear, Home, Plus, Sparkles } from "@/components/ui/icons";
import { relativeTime } from "@/lib/hooks";
import { qk } from "@/lib/queryKeys";
import { listConversations, getConversation, listRunsForConversation } from "@/lib/api";
import type { ProjectRow } from "@/types";
import { C } from "@/lib/theme";

interface SidebarProps {
  selectedConversationId: string | null;
  onSelectConversation: (id: string | null) => void;
  onNewRun: () => void;
  projects: ProjectRow[];
  selectedProjectId: string | null;
  onSelectProject: (id: string) => void;
  onCreateProject: () => void;
  projectView: "home" | "settings";
  onSetProjectView: (view: "home" | "settings") => void;
}

// ── Date grouping ─────────────────────────────────────────────────────────────

function getDateGroup(dateStr: string): string {
  const d = new Date(dateStr);
  const now = new Date();
  const todayStart = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const yesterdayStart = new Date(todayStart.getTime() - 864e5);
  const dayOfWeek = now.getDay(); // 0=Sun
  const daysToMon = dayOfWeek === 0 ? 6 : dayOfWeek - 1;
  const thisWeekStart = new Date(todayStart.getTime() - daysToMon * 864e5);
  const lastWeekStart = new Date(thisWeekStart.getTime() - 7 * 864e5);
  if (d >= todayStart) return "Today";
  if (d >= yesterdayStart) return "Yesterday";
  if (d >= thisWeekStart) return "This Week";
  if (d >= lastWeekStart) return "Last Week";
  return "Older";
}

// ── Sidebar ───────────────────────────────────────────────────────────────────

export function Sidebar({
  selectedConversationId,
  onSelectConversation,
  onNewRun,
  projects,
  selectedProjectId,
  onSelectProject,
  onCreateProject,
  projectView,
  onSetProjectView,
}: SidebarProps) {
  const queryClient = useQueryClient();
  const [search, setSearch] = useState("");
  const [projectDropdownOpen, setProjectDropdownOpen] = useState(false);
  const [projectSearch, setProjectSearch] = useState("");
  const [convLimit, setConvLimit] = useState(100);
  const dropdownRef = useRef<HTMLDivElement>(null);
  const projectSearchRef = useRef<HTMLInputElement>(null);

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

      {/* ── Project switcher ────────────────────────────── */}
      <div style={{ padding: "14px 14px 10px", position: "relative" }} ref={dropdownRef}>
        <div style={{
          fontSize: 9.5,
          fontWeight: 700,
          letterSpacing: "0.08em",
          textTransform: "uppercase",
          color: "rgba(255,255,255,0.22)",
          marginBottom: 5,
          paddingLeft: 2,
        }}>
          Project
        </div>

        <button
          onClick={() => setProjectDropdownOpen(!projectDropdownOpen)}
          className="flex items-center justify-between cursor-pointer text-left"
          style={{
            width: "100%",
            background: projectDropdownOpen ? "rgba(255,255,255,0.06)" : "transparent",
            borderRadius: 5,
            padding: "6px 8px",
            color: selectedProject ? "rgba(255,255,255,0.85)" : "rgba(255,255,255,0.3)",
            fontSize: 12.5,
            fontWeight: 500,
            border: "none",
            minWidth: 0,
            transition: "background 0.12s",
          }}
          onMouseEnter={e => { if (!projectDropdownOpen) e.currentTarget.style.background = "rgba(255,255,255,0.04)"; }}
          onMouseLeave={e => { if (!projectDropdownOpen) e.currentTarget.style.background = "transparent"; }}
        >
          <span className="overflow-hidden text-ellipsis whitespace-nowrap">
            {selectedProject?.name ?? "Select project"}
          </span>
          <span style={{
            color: "rgba(255,255,255,0.25)",
            marginLeft: 6,
            flexShrink: 0,
            transition: "transform 0.15s",
            transform: projectDropdownOpen ? "rotate(180deg)" : "none",
            display: "flex",
          }}>
            <ChevronDown size={10} />
          </span>
        </button>

        {/* ── Project dropdown ── */}
        {projectDropdownOpen && (
          <div
            className="absolute left-0 right-0 z-50"
            style={{
              top: "calc(100% + 4px)",
              background: "#13161C",
              border: `1px solid rgba(255,255,255,0.07)`,
              borderRadius: 8,
              overflow: "hidden",
              display: "flex",
              flexDirection: "column",
              maxHeight: 300,
            }}
          >
            <div style={{
              padding: "8px 8px 6px",
              borderBottom: "1px solid rgba(255,255,255,0.05)",
              flexShrink: 0,
            }}>
              <div style={{
                display: "flex", alignItems: "center", gap: 7,
                background: "rgba(255,255,255,0.04)",
                borderRadius: 5,
                padding: "5px 9px",
              }}>
                <span style={{ color: "rgba(255,255,255,0.25)", flexShrink: 0, display: "flex" }}><Search size={11} /></span>
                <input
                  ref={projectSearchRef}
                  type="text"
                  placeholder="Search projects…"
                  value={projectSearch}
                  onChange={e => setProjectSearch(e.target.value)}
                  onKeyDown={e => e.key === "Escape" && setProjectDropdownOpen(false)}
                  style={{
                    flex: 1, background: "transparent", border: "none",
                    outline: "none", fontSize: 12, color: "rgba(255,255,255,0.75)",
                    fontFamily: "inherit",
                  }}
                />
                {projectSearch && (
                  <button
                    onClick={() => setProjectSearch("")}
                    style={{ background: "none", border: "none", cursor: "pointer", color: "rgba(255,255,255,0.3)", padding: 0, lineHeight: 1, fontSize: 14 }}
                  >
                    ×
                  </button>
                )}
              </div>
            </div>

            <div style={{ overflowY: "auto", flex: 1 }}>
              {(() => {
                const term = projectSearch.toLowerCase();
                const visible = projects
                  .filter(p => p.state === "active")
                  .filter(p => !term || (p.name || p.root_path.split("/").pop() || "").toLowerCase().includes(term));

                if (visible.length === 0) {
                  return (
                    <div style={{ padding: "14px", fontSize: 12, color: "rgba(255,255,255,0.25)", textAlign: "center" }}>
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
                        width: "100%", padding: "8px 12px",
                        background: isSelected ? "rgba(49,185,123,0.08)" : "transparent",
                        border: "none", cursor: "pointer", textAlign: "left",
                        display: "flex", alignItems: "center", gap: 10,
                        transition: "background 0.1s",
                        fontFamily: "inherit",
                      }}
                      onMouseEnter={e => { if (!isSelected) e.currentTarget.style.background = "rgba(255,255,255,0.04)"; }}
                      onMouseLeave={e => { if (!isSelected) e.currentTarget.style.background = "transparent"; }}
                    >
                      <div style={{ flex: 1, minWidth: 0 }}>
                        <div style={{
                          fontSize: 12.5, fontWeight: isSelected ? 600 : 400,
                          color: isSelected ? "rgba(255,255,255,0.9)" : "rgba(255,255,255,0.6)",
                          overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                        }}>
                          {name}
                        </div>
                        <div style={{
                          fontSize: 10.5, color: "rgba(255,255,255,0.22)", marginTop: 1,
                          overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                          fontFamily: C.mono,
                        }}>
                          {path}
                        </div>
                      </div>
                      {isSelected && <span style={{ color: C.accent, flexShrink: 0, display: "flex" }}><Check size={10} /></span>}
                    </button>
                  );
                });
              })()}
            </div>

            <div style={{ borderTop: "1px solid rgba(255,255,255,0.05)", flexShrink: 0 }}>
              <button
                onClick={() => { setProjectDropdownOpen(false); onCreateProject(); }}
                style={{
                  width: "100%", padding: "9px 12px",
                  background: "transparent", border: "none", cursor: "pointer",
                  display: "flex", alignItems: "center", gap: 7,
                  color: C.accent, fontSize: 12, fontWeight: 600,
                  fontFamily: "inherit", textAlign: "left",
                  transition: "background 0.12s",
                }}
                onMouseEnter={e => { e.currentTarget.style.background = "rgba(49,185,123,0.06)"; }}
                onMouseLeave={e => { e.currentTarget.style.background = "transparent"; }}
              >
                <Plus size={10} /> New Project
              </button>
            </div>
          </div>
        )}
      </div>

      {/* ── Divider ── */}
      <div style={{ height: 1, background: "rgba(255,255,255,0.04)", margin: "0 14px" }} />

      {/* ── Search ── */}
      <div style={{ padding: "10px 14px 8px" }}>
        <div style={{
          display: "flex", alignItems: "center", gap: 8,
          padding: "6px 10px", borderRadius: 5,
          background: "rgba(255,255,255,0.04)",
        }}>
          <span style={{ color: "rgba(255,255,255,0.2)", flexShrink: 0, display: "flex" }}><Search size={11} /></span>
          <input
            type="text"
            placeholder="Search sessions…"
            value={search}
            onChange={e => setSearch(e.target.value)}
            style={{
              flex: 1, background: "transparent", border: "none", outline: "none",
              fontSize: 12, color: "rgba(255,255,255,0.65)", fontFamily: "inherit",
            }}
          />
          {!search && (
            <span style={{
              fontSize: 10, color: "rgba(255,255,255,0.18)",
              background: "rgba(255,255,255,0.05)",
              padding: "1px 5px", borderRadius: 3,
              letterSpacing: "0.02em",
            }}>
              ⌘K
            </span>
          )}
          {search && (
            <button
              onClick={() => setSearch("")}
              style={{ background: "none", border: "none", cursor: "pointer", color: "rgba(255,255,255,0.25)", padding: 0, lineHeight: 1, fontSize: 14 }}
            >
              ×
            </button>
          )}
        </div>
      </div>

      {/* ── Sessions header ── */}
      <div style={{
        display: "flex", alignItems: "center", justifyContent: "space-between",
        padding: "6px 16px 4px",
      }}>
        <span style={{
          fontSize: 9.5, fontWeight: 700, letterSpacing: "0.08em",
          textTransform: "uppercase", color: "rgba(255,255,255,0.22)",
        }}>
          Sessions
        </span>
        {filtered && filtered.length > 0 && (
          <span style={{ fontSize: 10, color: "rgba(255,255,255,0.2)" }}>
            {filtered.length}
          </span>
        )}
      </div>

      {/* ── Session list ── */}
      <div className="smooth-scroll flex-1 overflow-y-auto" style={{ padding: "2px 8px 8px" }}>
        {!conversations && (
          <div style={{ padding: "8px 6px", display: "flex", flexDirection: "column", gap: 2 }}>
            {[80, 60, 70, 55].map((w, i) => (
              <div key={i} style={{ padding: "10px 10px", borderRadius: 6 }}>
                <div className="skeleton" style={{ height: 11, width: `${w}%`, marginBottom: 7, borderRadius: 3 }} />
                <div className="skeleton" style={{ height: 9, width: "40%", borderRadius: 3 }} />
              </div>
            ))}
          </div>
        )}

        {conversations && (!filtered || filtered.length === 0) && (
          <div style={{ padding: "40px 16px", textAlign: "center" }}>
            <div style={{ fontSize: 12, color: "rgba(255,255,255,0.2)", marginBottom: 12 }}>
              {conversations.length > 0 ? "No matches" : "No sessions yet"}
            </div>
            {conversations.length === 0 && (
              <button
                onClick={onNewRun}
                style={{
                  padding: "7px 16px", borderRadius: 6,
                  background: "rgba(62,207,142,0.1)",
                  border: "1px solid rgba(62,207,142,0.22)", color: C.accent,
                  cursor: "pointer",
                  fontSize: 11.5, fontWeight: 600, fontFamily: "inherit",
                  display: "inline-flex", alignItems: "center", gap: 6,
                }}
              >
                <Sparkles size={11} /> New Session
              </button>
            )}
          </div>
        )}

        {(() => {
          if (!filtered) return null;
          const items: React.ReactNode[] = [];
          let lastGroup = "";
          for (const conv of filtered) {
            const group = getDateGroup(conv.updated_at);
            if (group !== lastGroup) {
              lastGroup = group;
              items.push(
                <div key={`hdr-${group}`} style={{
                  padding: "10px 12px 4px",
                  fontSize: 10, fontWeight: 700,
                  letterSpacing: "0.07em", textTransform: "uppercase",
                  color: "rgba(255,255,255,0.18)",
                }}>
                  {group}
                </div>
              );
            }

            const active = selectedConversationId === conv.id;
            const isRunning = ["executing", "waiting_for_gate", "planning", "verifying", "publishing", "merging"].includes(conv.state);
            const kindColor =
              conv.conversation_kind === "cli" ? C.blue
              : conv.conversation_kind === "hive_loom" ? "#F59E0B"
              : C.accent;
            const kindLabel =
              conv.conversation_kind === "cli" ? "CLI"
              : conv.conversation_kind === "hive_loom" ? "HIVE"
              : "RUN";

            items.push(
              <button
                key={conv.id}
                onClick={() => onSelectConversation(conv.id)}
                style={{
                  width: "100%", textAlign: "left",
                  padding: "8px 10px 8px 12px",
                  borderRadius: 6, marginBottom: 1,
                  background: active ? "rgba(255,255,255,0.07)" : "transparent",
                  border: "none", cursor: "pointer",
                  fontFamily: "inherit", position: "relative",
                  transition: "background 0.1s", display: "block",
                }}
                onMouseEnter={e => {
                  prefetchConversation(conv.id);
                  if (!active) e.currentTarget.style.background = "rgba(255,255,255,0.04)";
                }}
                onMouseLeave={e => {
                  if (!active) e.currentTarget.style.background = "transparent";
                }}
              >
                {active && (
                  <span style={{
                    position: "absolute", left: 0, top: "50%",
                    transform: "translateY(-50%)",
                    width: 2, height: 18, borderRadius: 1,
                    background: kindColor,
                  }} />
                )}
                {isRunning && (
                  <span style={{
                    position: "absolute", top: 10, right: 10,
                    width: 5, height: 5, borderRadius: "50%",
                    background: kindColor, opacity: 0.85,
                  }} />
                )}
                <div style={{
                  display: "flex", alignItems: "flex-start", gap: 6,
                  paddingRight: isRunning ? 14 : 0,
                }}>
                  <span style={{
                    flex: 1,
                    overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                    fontSize: 12.5, fontWeight: active ? 500 : 400,
                    color: active ? "rgba(255,255,255,0.9)" : "rgba(255,255,255,0.6)",
                    lineHeight: "18px",
                  }}>
                    {conv.title || `Session ${conv.id.slice(0, 8)}`}
                  </span>
                  <div style={{ flexShrink: 0, textAlign: "right" }}>
                    <div style={{
                      fontSize: 9, fontWeight: 700,
                      letterSpacing: "0.04em", textTransform: "uppercase",
                      color: kindColor, opacity: active ? 0.9 : 0.55,
                      lineHeight: "18px",
                    }}>
                      {kindLabel}
                    </div>
                    {conv.conversation_kind === "cli" && conv.cli_provider && (
                      <div style={{
                        fontSize: 9.5, color: "rgba(59,130,246,0.55)",
                        fontWeight: 500, lineHeight: "12px", marginTop: 1,
                      }}>
                        {conv.cli_provider}
                      </div>
                    )}
                  </div>
                </div>
                <div style={{
                  display: "flex", alignItems: "center", gap: 4,
                  marginTop: 2, fontSize: 10.5,
                  color: "rgba(255,255,255,0.25)", lineHeight: 1,
                }}>
                  {isRunning && (
                    <>
                      <span style={{ color: kindColor, fontWeight: 600, fontSize: 9, opacity: 0.9 }}>
                        Active
                      </span>
                      <span style={{ opacity: 0.4 }}>·</span>
                    </>
                  )}
                  <span>{relativeTime(conv.updated_at)}</span>
                </div>
              </button>
            );
          }
          return items;
        })()}

        {conversations && conversations.length >= convLimit && (
          <button
            onClick={() => setConvLimit(prev => prev + 100)}
            style={{
              width: "100%", padding: "8px 0", marginTop: 4,
              background: "transparent", border: "none",
              color: "rgba(255,255,255,0.2)", fontSize: 11,
              cursor: "pointer", fontFamily: "inherit",
            }}
            onMouseEnter={e => { e.currentTarget.style.color = "rgba(255,255,255,0.4)"; }}
            onMouseLeave={e => { e.currentTarget.style.color = "rgba(255,255,255,0.2)"; }}
          >
            Load more…
          </button>
        )}
      </div>

      {/* ── Bottom actions ── */}
      <div style={{ padding: "10px 12px 14px", display: "flex", flexDirection: "column", gap: 6 }}>
        {/* Row 1: New Session */}
        <button
          onClick={onNewRun}
          style={{
            width: "100%", padding: "9px 0",
            borderRadius: 7, border: "1px solid rgba(62,207,142,0.22)",
            background: "rgba(62,207,142,0.1)",
            color: C.accent,
            fontSize: 12, fontWeight: 600,
            cursor: "pointer", fontFamily: "inherit",
            display: "flex", alignItems: "center", justifyContent: "center", gap: 7,
            transition: "background 0.14s, border-color 0.14s",
            letterSpacing: "0.01em",
          }}
          onMouseEnter={e => {
            e.currentTarget.style.background = "rgba(62,207,142,0.17)";
            e.currentTarget.style.borderColor = "rgba(62,207,142,0.36)";
          }}
          onMouseLeave={e => {
            e.currentTarget.style.background = "rgba(62,207,142,0.1)";
            e.currentTarget.style.borderColor = "rgba(62,207,142,0.22)";
          }}
        >
          <Sparkles size={12} /> New Session
        </button>

        {/* Row 2: Home | Settings */}
        {selectedProjectId && (
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 6 }}>
            {(() => {
              const homeActive = !selectedConversationId && projectView === "home";
              return (
                <button
                  onClick={() => { onSelectConversation(null); onSetProjectView("home"); }}
                  title="Project home"
                  style={{
                    padding: "7px 0", borderRadius: 6, border: "none",
                    background: homeActive ? "rgba(49,185,123,0.08)" : "rgba(255,255,255,0.04)",
                    color: homeActive ? C.accent : "rgba(255,255,255,0.35)",
                    fontSize: 11.5, fontWeight: 500, cursor: "pointer", fontFamily: "inherit",
                    display: "flex", alignItems: "center", justifyContent: "center", gap: 5,
                    transition: "background 0.12s, color 0.12s",
                  }}
                  onMouseEnter={e => { if (!homeActive) { e.currentTarget.style.background = "rgba(255,255,255,0.07)"; e.currentTarget.style.color = "rgba(255,255,255,0.6)"; } }}
                  onMouseLeave={e => { if (!homeActive) { e.currentTarget.style.background = "rgba(255,255,255,0.04)"; e.currentTarget.style.color = "rgba(255,255,255,0.35)"; } }}
                >
                  <Home size={11} /> Home
                </button>
              );
            })()}
            {(() => {
              const settingsActive = !selectedConversationId && projectView === "settings";
              return (
                <button
                  onClick={() => { onSelectConversation(null); onSetProjectView("settings"); }}
                  title="Project settings"
                  style={{
                    padding: "7px 0", borderRadius: 6, border: "none",
                    background: settingsActive ? "rgba(49,185,123,0.08)" : "rgba(255,255,255,0.04)",
                    color: settingsActive ? C.accent : "rgba(255,255,255,0.35)",
                    fontSize: 11.5, fontWeight: 500, cursor: "pointer", fontFamily: "inherit",
                    display: "flex", alignItems: "center", justifyContent: "center", gap: 5,
                    transition: "background 0.12s, color 0.12s",
                  }}
                  onMouseEnter={e => { if (!settingsActive) { e.currentTarget.style.background = "rgba(255,255,255,0.07)"; e.currentTarget.style.color = "rgba(255,255,255,0.6)"; } }}
                  onMouseLeave={e => { if (!settingsActive) { e.currentTarget.style.background = "rgba(255,255,255,0.04)"; e.currentTarget.style.color = "rgba(255,255,255,0.35)"; } }}
                >
                  <Gear size={11} /> Settings
                </button>
              );
            })()}
          </div>
        )}
      </div>
    </div>
  );
}
