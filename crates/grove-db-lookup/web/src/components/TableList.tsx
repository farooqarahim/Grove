import { useEffect, useState } from "react";
import { fetchTables } from "../api";

interface Props {
  db: string;
  selected: string | null;
  onSelect: (table: string) => void;
}

export default function TableList({ db, selected, onSelect }: Props) {
  const [tables, setTables] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setTables([]);
    setError(null);
    setFilter("");
    setLoading(true);
    fetchTables(db)
      .then(setTables)
      .catch((e) => setError(e.message))
      .finally(() => setLoading(false));
  }, [db]);

  const filtered = filter
    ? tables.filter((t) => t.toLowerCase().includes(filter.toLowerCase()))
    : tables;

  if (error) {
    return (
      <div style={{ padding: "12px 20px" }}>
        <div
          style={{
            padding: "10px 12px",
            background: "rgba(239, 68, 68, 0.08)",
            border: "1px solid rgba(239, 68, 68, 0.2)",
            borderRadius: 8,
            fontSize: 12,
            color: "#fca5a5",
          }}
        >
          {error}
        </div>
      </div>
    );
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      {/* Section header + filter */}
      <div style={{ padding: "14px 20px 8px" }}>
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            marginBottom: 10,
          }}
        >
          <span
            style={{
              fontSize: 10,
              fontWeight: 600,
              color: "#52525b",
              textTransform: "uppercase",
              letterSpacing: "0.08em",
            }}
          >
            Tables
          </span>
          <span
            style={{
              fontSize: 10,
              fontWeight: 500,
              color: "#3f3f46",
              background: "#18181b",
              padding: "2px 7px",
              borderRadius: 10,
              border: "1px solid #27272a",
            }}
          >
            {loading ? "..." : tables.length}
          </span>
        </div>

        {tables.length > 5 && (
          <div style={{ position: "relative", marginBottom: 4 }}>
            <svg
              width="13"
              height="13"
              viewBox="0 0 24 24"
              fill="none"
              stroke="#52525b"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              style={{ position: "absolute", left: 9, top: "50%", transform: "translateY(-50%)" }}
            >
              <circle cx="11" cy="11" r="8" />
              <line x1="21" y1="21" x2="16.65" y2="16.65" />
            </svg>
            <input
              type="text"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder="Filter tables..."
              style={{
                width: "100%",
                padding: "7px 10px 7px 30px",
                background: "#18181b",
                color: "#e4e4e7",
                border: "1px solid #27272a",
                borderRadius: 7,
                fontSize: 12,
                outline: "none",
                fontFamily: "inherit",
                transition: "border-color 0.15s",
              }}
              onFocus={(e) => { e.currentTarget.style.borderColor = "#3b82f6"; }}
              onBlur={(e) => { e.currentTarget.style.borderColor = "#27272a"; }}
            />
          </div>
        )}
      </div>

      {/* Table list */}
      <div style={{ flex: 1, overflow: "auto", padding: "0 8px 8px" }}>
        {loading ? (
          <div style={{ padding: "20px 12px", textAlign: "center", color: "#3f3f46", fontSize: 12 }}>
            Loading tables...
          </div>
        ) : filtered.length === 0 ? (
          <div style={{ padding: "20px 12px", textAlign: "center", color: "#3f3f46", fontSize: 12 }}>
            {filter ? "No tables match filter" : "No tables found"}
          </div>
        ) : (
          filtered.map((table) => {
            const isSelected = selected === table;
            return (
              <div
                key={table}
                onClick={() => onSelect(table)}
                style={{
                  padding: "8px 12px",
                  cursor: "pointer",
                  fontSize: 13,
                  fontFamily: "'JetBrains Mono', monospace",
                  background: isSelected
                    ? "linear-gradient(135deg, rgba(59,130,246,0.12), rgba(139,92,246,0.08))"
                    : "transparent",
                  color: isSelected ? "#93c5fd" : "#a1a1aa",
                  borderRadius: 6,
                  marginBottom: 2,
                  transition: "all 0.12s",
                  display: "flex",
                  alignItems: "center",
                  gap: 8,
                  border: isSelected ? "1px solid rgba(59,130,246,0.2)" : "1px solid transparent",
                }}
                onMouseEnter={(e) => {
                  if (!isSelected) {
                    e.currentTarget.style.background = "#18181b";
                    e.currentTarget.style.color = "#d4d4d8";
                  }
                }}
                onMouseLeave={(e) => {
                  if (!isSelected) {
                    e.currentTarget.style.background = "transparent";
                    e.currentTarget.style.color = "#a1a1aa";
                  }
                }}
              >
                <svg
                  width="13"
                  height="13"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke={isSelected ? "#3b82f6" : "#3f3f46"}
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  style={{ flexShrink: 0 }}
                >
                  <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
                  <line x1="3" y1="9" x2="21" y2="9" />
                  <line x1="9" y1="3" x2="9" y2="21" />
                </svg>
                <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                  {table}
                </span>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
