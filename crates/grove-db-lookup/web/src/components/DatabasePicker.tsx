import { useEffect, useState } from "react";
import { fetchDatabases, type DatabaseEntry } from "../api";

interface Props {
  selected: string | null;
  onSelect: (db: DatabaseEntry) => void;
}

export default function DatabasePicker({ selected, onSelect }: Props) {
  const [databases, setDatabases] = useState<DatabaseEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setLoading(true);
    fetchDatabases()
      .then(setDatabases)
      .catch((e) => setError(e.message))
      .finally(() => setLoading(false));
  }, []);

  if (error) {
    return (
      <div style={{ padding: "16px 20px" }}>
        <div
          style={{
            padding: "10px 12px",
            background: "rgba(239, 68, 68, 0.08)",
            border: "1px solid rgba(239, 68, 68, 0.2)",
            borderRadius: 8,
            fontSize: 12,
            color: "#fca5a5",
            display: "flex",
            alignItems: "center",
            gap: 8,
          }}
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="#ef4444" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <circle cx="12" cy="12" r="10" />
            <line x1="15" y1="9" x2="9" y2="15" />
            <line x1="9" y1="9" x2="15" y2="15" />
          </svg>
          <span>{error}</span>
        </div>
      </div>
    );
  }

  return (
    <div style={{ padding: "16px 20px", borderBottom: "1px solid #1c1c22" }}>
      <label
        style={{
          fontSize: 10,
          fontWeight: 600,
          color: "#52525b",
          display: "block",
          marginBottom: 8,
          textTransform: "uppercase",
          letterSpacing: "0.08em",
        }}
      >
        Database
      </label>
      <div style={{ position: "relative" }}>
        <select
          value={selected ?? ""}
          onChange={(e) => {
            const db = databases.find((d) => d.path === e.target.value);
            if (db) onSelect(db);
          }}
          disabled={loading}
          style={{
            width: "100%",
            padding: "9px 32px 9px 12px",
            background: "#18181b",
            color: selected ? "#e4e4e7" : "#71717a",
            border: "1px solid #27272a",
            borderRadius: 8,
            fontSize: 13,
            cursor: loading ? "wait" : "pointer",
            appearance: "none",
            outline: "none",
            fontFamily: "inherit",
            transition: "border-color 0.15s",
          }}
          onFocus={(e) => { e.currentTarget.style.borderColor = "#3b82f6"; }}
          onBlur={(e) => { e.currentTarget.style.borderColor = "#27272a"; }}
        >
          <option value="">{loading ? "Loading..." : "Select a database..."}</option>
          {databases.map((db) => (
            <option key={db.path} value={db.path}>
              {db.name}
              {db.name !== db.id ? ` (${db.id.slice(0, 8)})` : ""}
            </option>
          ))}
        </select>
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="#52525b"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          style={{ position: "absolute", right: 10, top: "50%", transform: "translateY(-50%)", pointerEvents: "none" }}
        >
          <polyline points="6 9 12 15 18 9" />
        </svg>
      </div>

      {selected && (
        <div
          style={{
            fontSize: 10,
            color: "#3f3f46",
            marginTop: 8,
            wordBreak: "break-all",
            fontFamily: "'JetBrains Mono', monospace",
            lineHeight: 1.5,
            padding: "6px 8px",
            background: "#111114",
            borderRadius: 6,
            border: "1px solid #1c1c22",
          }}
        >
          {selected}
        </div>
      )}

      {!loading && databases.length > 0 && (
        <div style={{ fontSize: 10, color: "#3f3f46", marginTop: 6 }}>
          {databases.length} database{databases.length !== 1 ? "s" : ""} found
        </div>
      )}
    </div>
  );
}
