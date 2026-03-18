import { useEffect, useState } from "react";
import { fetchSchema, type ColumnInfo } from "../api";

interface Props {
  db: string;
  table: string;
  onClose: () => void;
}

const TYPE_COLORS: Record<string, { bg: string; text: string; border: string }> = {
  INTEGER: { bg: "rgba(59,130,246,0.08)", text: "#60a5fa", border: "rgba(59,130,246,0.2)" },
  INT: { bg: "rgba(59,130,246,0.08)", text: "#60a5fa", border: "rgba(59,130,246,0.2)" },
  REAL: { bg: "rgba(245,158,11,0.08)", text: "#fbbf24", border: "rgba(245,158,11,0.2)" },
  FLOAT: { bg: "rgba(245,158,11,0.08)", text: "#fbbf24", border: "rgba(245,158,11,0.2)" },
  TEXT: { bg: "rgba(34,197,94,0.08)", text: "#4ade80", border: "rgba(34,197,94,0.2)" },
  VARCHAR: { bg: "rgba(34,197,94,0.08)", text: "#4ade80", border: "rgba(34,197,94,0.2)" },
  BLOB: { bg: "rgba(168,85,247,0.08)", text: "#c084fc", border: "rgba(168,85,247,0.2)" },
  BOOLEAN: { bg: "rgba(236,72,153,0.08)", text: "#f472b6", border: "rgba(236,72,153,0.2)" },
  DATETIME: { bg: "rgba(6,182,212,0.08)", text: "#22d3ee", border: "rgba(6,182,212,0.2)" },
  TIMESTAMP: { bg: "rgba(6,182,212,0.08)", text: "#22d3ee", border: "rgba(6,182,212,0.2)" },
};

function getTypeColor(colType: string) {
  const upper = colType.toUpperCase();
  for (const [key, val] of Object.entries(TYPE_COLORS)) {
    if (upper.includes(key)) return val;
  }
  return { bg: "rgba(113,113,122,0.08)", text: "#71717a", border: "rgba(113,113,122,0.2)" };
}

export default function SchemaPanel({ db, table, onClose }: Props) {
  const [columns, setColumns] = useState<ColumnInfo[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setError(null);
    fetchSchema(db, table)
      .then(setColumns)
      .catch((e) => setError(e.message));
  }, [db, table]);

  return (
    <div
      style={{
        width: 320,
        minWidth: 320,
        borderLeft: "1px solid #1c1c22",
        background: "#0c0c10",
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
      }}
    >
      {/* Header */}
      <div
        style={{
          padding: "14px 16px",
          borderBottom: "1px solid #1c1c22",
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
        }}
      >
        <div>
          <div style={{ fontSize: 13, fontWeight: 600, color: "#e4e4e7" }}>Schema</div>
          <div
            style={{
              fontSize: 11,
              color: "#52525b",
              fontFamily: "'JetBrains Mono', monospace",
              marginTop: 2,
            }}
          >
            {table}
          </div>
        </div>
        <button
          onClick={onClose}
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
          }}
          onMouseEnter={(e) => { e.currentTarget.style.color = "#a1a1aa"; e.currentTarget.style.background = "#18181b"; }}
          onMouseLeave={(e) => { e.currentTarget.style.color = "#52525b"; e.currentTarget.style.background = "none"; }}
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>

      {/* Stats */}
      <div
        style={{
          padding: "12px 16px",
          borderBottom: "1px solid #1c1c22",
          display: "flex",
          gap: 12,
        }}
      >
        <div
          style={{
            flex: 1,
            padding: "8px 10px",
            background: "#111114",
            borderRadius: 8,
            border: "1px solid #1c1c22",
            textAlign: "center",
          }}
        >
          <div style={{ fontSize: 18, fontWeight: 700, color: "#e4e4e7" }}>{columns.length}</div>
          <div style={{ fontSize: 9, color: "#52525b", textTransform: "uppercase", letterSpacing: "0.05em", marginTop: 2 }}>
            Columns
          </div>
        </div>
        <div
          style={{
            flex: 1,
            padding: "8px 10px",
            background: "#111114",
            borderRadius: 8,
            border: "1px solid #1c1c22",
            textAlign: "center",
          }}
        >
          <div style={{ fontSize: 18, fontWeight: 700, color: "#facc15" }}>
            {columns.filter((c) => c.pk).length}
          </div>
          <div style={{ fontSize: 9, color: "#52525b", textTransform: "uppercase", letterSpacing: "0.05em", marginTop: 2 }}>
            Primary Keys
          </div>
        </div>
        <div
          style={{
            flex: 1,
            padding: "8px 10px",
            background: "#111114",
            borderRadius: 8,
            border: "1px solid #1c1c22",
            textAlign: "center",
          }}
        >
          <div style={{ fontSize: 18, fontWeight: 700, color: "#f97316" }}>
            {columns.filter((c) => c.notnull).length}
          </div>
          <div style={{ fontSize: 9, color: "#52525b", textTransform: "uppercase", letterSpacing: "0.05em", marginTop: 2 }}>
            Not Null
          </div>
        </div>
      </div>

      {/* Column list */}
      <div style={{ flex: 1, overflow: "auto", padding: "8px 12px" }}>
        {error ? (
          <div style={{ color: "#fca5a5", padding: 12, fontSize: 12 }}>{error}</div>
        ) : (
          columns.map((col) => {
            const tc = getTypeColor(col.col_type);
            return (
              <div
                key={col.name}
                style={{
                  padding: "10px 12px",
                  marginBottom: 4,
                  borderRadius: 8,
                  background: "#111114",
                  border: "1px solid #1c1c22",
                  transition: "border-color 0.15s",
                }}
                onMouseEnter={(e) => { e.currentTarget.style.borderColor = "#27272a"; }}
                onMouseLeave={(e) => { e.currentTarget.style.borderColor = "#1c1c22"; }}
              >
                <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 8 }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                    <span
                      style={{
                        fontFamily: "'JetBrains Mono', monospace",
                        fontSize: 12,
                        fontWeight: 500,
                        color: "#e4e4e7",
                      }}
                    >
                      {col.name}
                    </span>
                    {col.pk && (
                      <span
                        style={{
                          fontSize: 9,
                          fontWeight: 600,
                          padding: "1px 5px",
                          borderRadius: 4,
                          background: "rgba(250,204,21,0.1)",
                          color: "#facc15",
                          border: "1px solid rgba(250,204,21,0.2)",
                          textTransform: "uppercase",
                          letterSpacing: "0.05em",
                        }}
                      >
                        PK
                      </span>
                    )}
                  </div>
                  <span
                    style={{
                      fontSize: 10,
                      fontWeight: 500,
                      padding: "2px 7px",
                      borderRadius: 5,
                      background: tc.bg,
                      color: tc.text,
                      border: `1px solid ${tc.border}`,
                      fontFamily: "'JetBrains Mono', monospace",
                    }}
                  >
                    {col.col_type || "ANY"}
                  </span>
                </div>

                <div style={{ display: "flex", gap: 12, marginTop: 6 }}>
                  {col.notnull && (
                    <span style={{ fontSize: 10, color: "#f97316" }}>NOT NULL</span>
                  )}
                  {col.default_value !== null && (
                    <span style={{ fontSize: 10, color: "#52525b" }}>
                      default: <span style={{ color: "#71717a", fontFamily: "'JetBrains Mono', monospace" }}>{col.default_value}</span>
                    </span>
                  )}
                </div>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
