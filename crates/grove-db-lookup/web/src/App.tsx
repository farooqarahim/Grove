import { useState } from "react";
import type { DatabaseEntry } from "./api";
import DatabasePicker from "./components/DatabasePicker";
import TableList from "./components/TableList";
import TableView from "./components/TableView";
import SchemaPanel from "./components/SchemaPanel";

export default function App() {
  const [db, setDb] = useState<DatabaseEntry | null>(null);
  const [table, setTable] = useState<string | null>(null);
  const [showSchema, setShowSchema] = useState(false);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);

  return (
    <div style={{ display: "flex", height: "100vh", background: "#09090b", color: "#e4e4e7" }}>
      {/* Sidebar */}
      <div
        style={{
          width: sidebarCollapsed ? 48 : 280,
          minWidth: sidebarCollapsed ? 48 : 280,
          borderRight: "1px solid #1c1c22",
          display: "flex",
          flexDirection: "column",
          overflow: "hidden",
          transition: "width 0.2s ease, min-width 0.2s ease",
          background: "#0c0c10",
        }}
      >
        {/* Logo / Header */}
        <div
          style={{
            padding: sidebarCollapsed ? "16px 8px" : "20px 20px 16px",
            borderBottom: "1px solid #1c1c22",
            display: "flex",
            alignItems: "center",
            justifyContent: sidebarCollapsed ? "center" : "space-between",
            gap: 8,
          }}
        >
          {!sidebarCollapsed && (
            <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
              <div
                style={{
                  width: 28,
                  height: 28,
                  borderRadius: 8,
                  background: "linear-gradient(135deg, #3b82f6, #8b5cf6)",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  fontSize: 13,
                  fontWeight: 700,
                  color: "#fff",
                  flexShrink: 0,
                }}
              >
                DB
              </div>
              <div>
                <div style={{ fontSize: 14, fontWeight: 700, letterSpacing: "-0.02em", color: "#f4f4f5" }}>
                  Grove DB Lookup
                </div>
                <div style={{ fontSize: 10, color: "#52525b", fontWeight: 500, marginTop: 1 }}>
                  SQLite Explorer
                </div>
              </div>
            </div>
          )}
          <button
            onClick={() => setSidebarCollapsed((c) => !c)}
            title={sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}
            style={{
              background: "none",
              border: "none",
              color: "#52525b",
              cursor: "pointer",
              padding: 4,
              borderRadius: 4,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              fontSize: 14,
              lineHeight: 1,
              flexShrink: 0,
            }}
            onMouseEnter={(e) => { e.currentTarget.style.color = "#a1a1aa"; e.currentTarget.style.background = "#18181b"; }}
            onMouseLeave={(e) => { e.currentTarget.style.color = "#52525b"; e.currentTarget.style.background = "none"; }}
          >
            {sidebarCollapsed ? "\u276F" : "\u276E"}
          </button>
        </div>

        {!sidebarCollapsed && (
          <>
            <DatabasePicker
              selected={db?.path ?? null}
              onSelect={(d) => {
                setDb(d);
                setTable(null);
                setShowSchema(false);
              }}
            />
            {db && (
              <div style={{ flex: 1, overflow: "auto" }}>
                <TableList db={db.path} selected={table} onSelect={(t) => { setTable(t); setShowSchema(false); }} />
              </div>
            )}

            {/* Sidebar Footer */}
            <div
              style={{
                padding: "12px 20px",
                borderTop: "1px solid #1c1c22",
                fontSize: 10,
                color: "#3f3f46",
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
              }}
            >
              <span>v0.1.0</span>
              <span style={{ fontFamily: "'JetBrains Mono', monospace" }}>localhost:3741</span>
            </div>
          </>
        )}
      </div>

      {/* Main content */}
      <div style={{ flex: 1, overflow: "hidden", display: "flex", flexDirection: "column" }}>
        {/* Breadcrumb / Toolbar bar */}
        {(db || table) && (
          <div
            style={{
              padding: "10px 24px",
              borderBottom: "1px solid #1c1c22",
              display: "flex",
              alignItems: "center",
              gap: 6,
              fontSize: 12,
              color: "#71717a",
              background: "#0c0c10",
            }}
          >
            {db && (
              <>
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="#52525b" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <ellipse cx="12" cy="5" rx="9" ry="3" />
                  <path d="M21 12c0 1.66-4 3-9 3s-9-1.34-9-3" />
                  <path d="M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5" />
                </svg>
                <span
                  style={{ cursor: "pointer", color: "#3b82f6", fontWeight: 500 }}
                  onClick={() => { setTable(null); setShowSchema(false); }}
                  onMouseEnter={(e) => { e.currentTarget.style.color = "#60a5fa"; }}
                  onMouseLeave={(e) => { e.currentTarget.style.color = "#3b82f6"; }}
                >
                  {db.name}
                </span>
                {table && (
                  <>
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="#27272a" strokeWidth="2">
                      <polyline points="9 18 15 12 9 6" />
                    </svg>
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="#52525b" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                      <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
                      <line x1="3" y1="9" x2="21" y2="9" />
                      <line x1="3" y1="15" x2="21" y2="15" />
                      <line x1="9" y1="3" x2="9" y2="21" />
                      <line x1="15" y1="3" x2="15" y2="21" />
                    </svg>
                    <span style={{ color: "#e4e4e7", fontWeight: 500, fontFamily: "'JetBrains Mono', monospace", fontSize: 12 }}>
                      {table}
                    </span>
                  </>
                )}
              </>
            )}

            <div style={{ flex: 1 }} />

            {db && table && (
              <button
                onClick={() => setShowSchema((s) => !s)}
                style={{
                  padding: "5px 12px",
                  fontSize: 11,
                  fontWeight: 500,
                  background: showSchema ? "#1e1b4b" : "#18181b",
                  color: showSchema ? "#818cf8" : "#71717a",
                  border: `1px solid ${showSchema ? "#312e81" : "#27272a"}`,
                  borderRadius: 6,
                  cursor: "pointer",
                  transition: "all 0.15s",
                  display: "flex",
                  alignItems: "center",
                  gap: 5,
                }}
                onMouseEnter={(e) => { if (!showSchema) { e.currentTarget.style.background = "#1c1c22"; e.currentTarget.style.color = "#a1a1aa"; } }}
                onMouseLeave={(e) => { if (!showSchema) { e.currentTarget.style.background = "#18181b"; e.currentTarget.style.color = "#71717a"; } }}
              >
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M12 3h7a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-7m0-18H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h7m0-18v18" />
                </svg>
                {showSchema ? "Hide Schema" : "Schema"}
              </button>
            )}
          </div>
        )}

        {/* Content area */}
        <div style={{ flex: 1, overflow: "hidden", display: "flex" }}>
          {db && table ? (
            <>
              <div style={{ flex: 1, overflow: "hidden", display: "flex", flexDirection: "column" }}>
                <TableView db={db.path} table={table} />
              </div>
              {showSchema && (
                <SchemaPanel db={db.path} table={table} onClose={() => setShowSchema(false)} />
              )}
            </>
          ) : (
            <div
              style={{
                flex: 1,
                display: "flex",
                flexDirection: "column",
                alignItems: "center",
                justifyContent: "center",
                gap: 20,
              }}
            >
              <div
                style={{
                  width: 72,
                  height: 72,
                  borderRadius: 20,
                  background: "linear-gradient(135deg, rgba(59,130,246,0.08), rgba(139,92,246,0.08))",
                  border: "1px solid #1c1c22",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                }}
              >
                <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="#3b82f6" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" style={{ opacity: 0.7 }}>
                  {!db ? (
                    <>
                      <ellipse cx="12" cy="5" rx="9" ry="3" />
                      <path d="M21 12c0 1.66-4 3-9 3s-9-1.34-9-3" />
                      <path d="M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5" />
                    </>
                  ) : (
                    <>
                      <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
                      <line x1="3" y1="9" x2="21" y2="9" />
                      <line x1="3" y1="15" x2="21" y2="15" />
                      <line x1="9" y1="3" x2="9" y2="21" />
                      <line x1="15" y1="3" x2="15" y2="21" />
                    </>
                  )}
                </svg>
              </div>
              <div style={{ textAlign: "center" }}>
                <div style={{ fontSize: 16, fontWeight: 600, color: "#a1a1aa", marginBottom: 8 }}>
                  {!db ? "Select a Database" : "Select a Table"}
                </div>
                <div style={{ fontSize: 13, color: "#52525b", maxWidth: 320, lineHeight: 1.6 }}>
                  {!db
                    ? "Choose a Grove workspace database from the sidebar to explore its tables and data."
                    : "Pick a table from the sidebar to view rows, inspect schema, and edit data inline."}
                </div>
              </div>
              <div style={{ display: "flex", gap: 24, marginTop: 12 }}>
                {[
                  { label: "Browse", desc: "View table rows" },
                  { label: "Inspect", desc: "Column schemas" },
                  { label: "Edit", desc: "Inline updates" },
                ].map((item) => (
                  <div
                    key={item.label}
                    style={{
                      padding: "14px 20px",
                      borderRadius: 10,
                      border: "1px solid #1c1c22",
                      background: "#0c0c10",
                      textAlign: "center",
                      minWidth: 100,
                    }}
                  >
                    <div style={{ fontSize: 12, fontWeight: 600, color: "#71717a", marginBottom: 3 }}>
                      {item.label}
                    </div>
                    <div style={{ fontSize: 10, color: "#3f3f46" }}>{item.desc}</div>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
