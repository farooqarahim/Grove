import { useCallback, useEffect, useRef, useState } from "react";
import { fetchRows, updateRow, type TableRows } from "../api";

interface Props {
  db: string;
  table: string;
}

interface EditState {
  rowIdx: number;
  column: string;
  value: string;
}

const TYPE_COLORS: Record<string, string> = {
  INTEGER: "#60a5fa",
  INT: "#60a5fa",
  REAL: "#fbbf24",
  FLOAT: "#fbbf24",
  TEXT: "#4ade80",
  VARCHAR: "#4ade80",
  BLOB: "#c084fc",
  BOOLEAN: "#f472b6",
  DATETIME: "#22d3ee",
  TIMESTAMP: "#22d3ee",
};

function getTypeColor(colType: string): string {
  const upper = colType.toUpperCase();
  for (const [key, color] of Object.entries(TYPE_COLORS)) {
    if (upper.includes(key)) return color;
  }
  return "#71717a";
}

export default function TableView({ db, table }: Props) {
  const [data, setData] = useState<TableRows | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [page, setPage] = useState(1);
  const [sort, setSort] = useState<string | undefined>();
  const [order, setOrder] = useState<string | undefined>();
  const [editing, setEditing] = useState<EditState | null>(null);
  const [saving, setSaving] = useState(false);
  const [searchFilter, setSearchFilter] = useState("");
  const [loading, setLoading] = useState(false);
  const [toast, setToast] = useState<{ message: string; type: "success" | "error" } | null>(null);
  const toastTimeout = useRef<ReturnType<typeof setTimeout>>();

  const showToast = (message: string, type: "success" | "error") => {
    if (toastTimeout.current) clearTimeout(toastTimeout.current);
    setToast({ message, type });
    toastTimeout.current = setTimeout(() => setToast(null), 3000);
  };

  const load = useCallback(() => {
    setError(null);
    setLoading(true);
    fetchRows(db, table, page, 50, sort, order)
      .then(setData)
      .catch((e) => setError(e.message))
      .finally(() => setLoading(false));
  }, [db, table, page, sort, order]);

  useEffect(() => {
    setPage(1);
    setSort(undefined);
    setOrder(undefined);
    setSearchFilter("");
    setEditing(null);
  }, [db, table]);

  useEffect(() => {
    load();
  }, [load]);

  const handleSort = (col: string) => {
    if (sort === col && order === "asc") {
      setOrder("desc");
    } else if (sort === col && order === "desc") {
      setSort(undefined);
      setOrder(undefined);
    } else {
      setSort(col);
      setOrder("asc");
    }
  };

  const pkColumn = data?.columns.find((c) => c.pk);

  const handleSave = async () => {
    if (!editing || !data || !pkColumn) return;
    setSaving(true);

    const row = data.rows[editing.rowIdx];
    const pkValue = String(row[pkColumn.name] ?? "");

    let parsedValue: unknown = editing.value;
    if (editing.value === "") {
      parsedValue = null;
    } else if (editing.value === "null") {
      parsedValue = null;
    } else if (!isNaN(Number(editing.value)) && editing.value.trim() !== "") {
      parsedValue = Number(editing.value);
    }

    try {
      await updateRow(db, table, pkValue, pkColumn.name, {
        [editing.column]: parsedValue,
      });
      setEditing(null);
      showToast("Row updated successfully", "success");
      load();
    } catch (e) {
      showToast(e instanceof Error ? e.message : "Update failed", "error");
    } finally {
      setSaving(false);
    }
  };

  if (error) {
    return (
      <div style={{ padding: 24, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", height: "100%", gap: 16 }}>
        <div
          style={{
            padding: "16px 20px",
            background: "rgba(239, 68, 68, 0.06)",
            border: "1px solid rgba(239, 68, 68, 0.15)",
            borderRadius: 12,
            maxWidth: 400,
            textAlign: "center",
          }}
        >
          <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#ef4444" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" style={{ marginBottom: 8 }}>
            <circle cx="12" cy="12" r="10" />
            <line x1="12" y1="8" x2="12" y2="12" />
            <line x1="12" y1="16" x2="12.01" y2="16" />
          </svg>
          <div style={{ color: "#fca5a5", fontSize: 13, marginBottom: 12, lineHeight: 1.5 }}>{error}</div>
          <button
            onClick={() => { setError(null); load(); }}
            style={{
              padding: "7px 16px",
              background: "rgba(239, 68, 68, 0.1)",
              color: "#fca5a5",
              border: "1px solid rgba(239, 68, 68, 0.2)",
              borderRadius: 8,
              cursor: "pointer",
              fontSize: 12,
              fontWeight: 500,
            }}
          >
            Retry
          </button>
        </div>
      </div>
    );
  }

  if (!data) {
    return (
      <div style={{ padding: 24, display: "flex", alignItems: "center", justifyContent: "center", height: "100%", gap: 10, color: "#52525b" }}>
        <div
          style={{
            width: 16,
            height: 16,
            border: "2px solid #27272a",
            borderTopColor: "#3b82f6",
            borderRadius: "50%",
            animation: "spin 0.8s linear infinite",
          }}
        />
        <style>{`@keyframes spin { to { transform: rotate(360deg); } }`}</style>
        <span style={{ fontSize: 13 }}>Loading data...</span>
      </div>
    );
  }

  const totalPages = Math.ceil(data.total / data.page_size);

  // Client-side search filter on visible rows
  const filteredRows = searchFilter
    ? data.rows.filter((row) =>
        Object.values(row).some((v) =>
          String(v ?? "").toLowerCase().includes(searchFilter.toLowerCase())
        )
      )
    : data.rows;

  const startRow = (page - 1) * data.page_size + 1;
  const endRow = Math.min(page * data.page_size, data.total);

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%", position: "relative" }}>
      {/* Toast */}
      {toast && (
        <div
          style={{
            position: "absolute",
            top: 12,
            right: 12,
            zIndex: 100,
            padding: "10px 16px",
            borderRadius: 10,
            fontSize: 12,
            fontWeight: 500,
            display: "flex",
            alignItems: "center",
            gap: 8,
            background: toast.type === "success" ? "rgba(34,197,94,0.1)" : "rgba(239,68,68,0.1)",
            color: toast.type === "success" ? "#4ade80" : "#fca5a5",
            border: `1px solid ${toast.type === "success" ? "rgba(34,197,94,0.2)" : "rgba(239,68,68,0.2)"}`,
            backdropFilter: "blur(8px)",
            animation: "slideIn 0.2s ease-out",
          }}
        >
          <style>{`@keyframes slideIn { from { opacity: 0; transform: translateY(-8px); } to { opacity: 1; transform: translateY(0); } }`}</style>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            {toast.type === "success" ? (
              <><path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" /><polyline points="22 4 12 14.01 9 11.01" /></>
            ) : (
              <><circle cx="12" cy="12" r="10" /><line x1="15" y1="9" x2="9" y2="15" /><line x1="9" y1="9" x2="15" y2="15" /></>
            )}
          </svg>
          {toast.message}
        </div>
      )}

      {/* Toolbar */}
      <div
        style={{
          padding: "10px 20px",
          borderBottom: "1px solid #1c1c22",
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          gap: 12,
          background: "#0c0c10",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 16 }}>
          {/* Row/column stats */}
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span
              style={{
                fontSize: 12,
                fontWeight: 600,
                color: "#e4e4e7",
                background: "#18181b",
                padding: "4px 10px",
                borderRadius: 6,
                border: "1px solid #27272a",
                display: "flex",
                alignItems: "center",
                gap: 5,
              }}
            >
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="#52525b" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <line x1="8" y1="6" x2="21" y2="6" /><line x1="8" y1="12" x2="21" y2="12" /><line x1="8" y1="18" x2="21" y2="18" />
                <line x1="3" y1="6" x2="3.01" y2="6" /><line x1="3" y1="12" x2="3.01" y2="12" /><line x1="3" y1="18" x2="3.01" y2="18" />
              </svg>
              {data.total.toLocaleString()} row{data.total !== 1 ? "s" : ""}
            </span>
            <span
              style={{
                fontSize: 12,
                color: "#52525b",
                background: "#111114",
                padding: "4px 10px",
                borderRadius: 6,
                border: "1px solid #1c1c22",
              }}
            >
              {data.columns.length} col{data.columns.length !== 1 ? "s" : ""}
            </span>
          </div>

          {/* Search */}
          <div style={{ position: "relative" }}>
            <svg
              width="13"
              height="13"
              viewBox="0 0 24 24"
              fill="none"
              stroke="#52525b"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              style={{ position: "absolute", left: 10, top: "50%", transform: "translateY(-50%)" }}
            >
              <circle cx="11" cy="11" r="8" />
              <line x1="21" y1="21" x2="16.65" y2="16.65" />
            </svg>
            <input
              type="text"
              value={searchFilter}
              onChange={(e) => setSearchFilter(e.target.value)}
              placeholder="Search visible rows..."
              style={{
                padding: "6px 10px 6px 30px",
                background: "#18181b",
                color: "#e4e4e7",
                border: "1px solid #27272a",
                borderRadius: 7,
                fontSize: 12,
                outline: "none",
                fontFamily: "inherit",
                width: 200,
                transition: "border-color 0.15s, width 0.2s",
              }}
              onFocus={(e) => { e.currentTarget.style.borderColor = "#3b82f6"; e.currentTarget.style.width = "280px"; }}
              onBlur={(e) => { e.currentTarget.style.borderColor = "#27272a"; e.currentTarget.style.width = "200px"; }}
            />
          </div>
        </div>

        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          {loading && (
            <div
              style={{
                width: 14,
                height: 14,
                border: "2px solid #27272a",
                borderTopColor: "#3b82f6",
                borderRadius: "50%",
                animation: "spin 0.8s linear infinite",
              }}
            />
          )}
          <button
            onClick={load}
            style={{
              padding: "6px 14px",
              background: "#18181b",
              color: "#a1a1aa",
              border: "1px solid #27272a",
              borderRadius: 7,
              cursor: "pointer",
              fontSize: 12,
              fontWeight: 500,
              display: "flex",
              alignItems: "center",
              gap: 5,
              transition: "all 0.15s",
            }}
            onMouseEnter={(e) => { e.currentTarget.style.background = "#1c1c22"; e.currentTarget.style.borderColor = "#3f3f46"; }}
            onMouseLeave={(e) => { e.currentTarget.style.background = "#18181b"; e.currentTarget.style.borderColor = "#27272a"; }}
          >
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="23 4 23 10 17 10" /><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" />
            </svg>
            Refresh
          </button>
        </div>
      </div>

      {/* Table */}
      <div style={{ flex: 1, overflow: "auto" }}>
        <table
          style={{
            width: "100%",
            borderCollapse: "collapse",
            fontSize: 13,
            fontFamily: "'JetBrains Mono', monospace",
          }}
        >
          <thead>
            <tr>
              {/* Row number column */}
              <th
                style={{
                  padding: "10px 12px",
                  textAlign: "center",
                  borderBottom: "1px solid #1c1c22",
                  background: "#0c0c10",
                  position: "sticky",
                  top: 0,
                  zIndex: 2,
                  color: "#3f3f46",
                  fontSize: 10,
                  fontWeight: 500,
                  width: 48,
                  minWidth: 48,
                }}
              >
                #
              </th>
              {data.columns.map((col) => {
                const isSorted = sort === col.name;
                const typeColor = getTypeColor(col.col_type);
                return (
                  <th
                    key={col.name}
                    onClick={() => handleSort(col.name)}
                    style={{
                      padding: "10px 14px",
                      textAlign: "left",
                      borderBottom: "1px solid #1c1c22",
                      background: "#0c0c10",
                      cursor: "pointer",
                      position: "sticky",
                      top: 0,
                      zIndex: 1,
                      userSelect: "none",
                      whiteSpace: "nowrap",
                      color: isSorted ? "#e4e4e7" : "#a1a1aa",
                      fontSize: 11,
                      fontWeight: 600,
                      letterSpacing: "0.01em",
                      transition: "color 0.15s",
                    }}
                  >
                    <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                      <span>{col.name}</span>
                      {col.pk && (
                        <span
                          style={{
                            fontSize: 8,
                            fontWeight: 700,
                            padding: "1px 4px",
                            borderRadius: 3,
                            background: "rgba(250,204,21,0.1)",
                            color: "#facc15",
                            border: "1px solid rgba(250,204,21,0.2)",
                            letterSpacing: "0.05em",
                          }}
                        >
                          PK
                        </span>
                      )}
                      <span
                        style={{
                          fontSize: 9,
                          color: typeColor,
                          opacity: 0.6,
                          fontWeight: 400,
                        }}
                      >
                        {col.col_type || "?"}
                      </span>
                      {isSorted && (
                        <svg
                          width="10"
                          height="10"
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="#3b82f6"
                          strokeWidth="3"
                          strokeLinecap="round"
                          strokeLinejoin="round"
                        >
                          {order === "asc" ? (
                            <polyline points="18 15 12 9 6 15" />
                          ) : (
                            <polyline points="6 9 12 15 18 9" />
                          )}
                        </svg>
                      )}
                    </div>
                  </th>
                );
              })}
            </tr>
          </thead>
          <tbody>
            {filteredRows.length === 0 ? (
              <tr>
                <td
                  colSpan={data.columns.length + 1}
                  style={{
                    padding: 40,
                    textAlign: "center",
                    color: "#3f3f46",
                    fontSize: 13,
                    fontFamily: "'Inter', sans-serif",
                  }}
                >
                  {searchFilter ? "No rows match your search" : "No data in this table"}
                </td>
              </tr>
            ) : (
              filteredRows.map((row, rowIdx) => {
                const actualRowNum = startRow + rowIdx;
                return (
                  <tr
                    key={rowIdx}
                    style={{
                      borderBottom: "1px solid #111114",
                      transition: "background 0.1s",
                    }}
                    onMouseEnter={(e) => { e.currentTarget.style.background = "rgba(59,130,246,0.03)"; }}
                    onMouseLeave={(e) => { e.currentTarget.style.background = "transparent"; }}
                  >
                    {/* Row number */}
                    <td
                      style={{
                        padding: "7px 12px",
                        textAlign: "center",
                        color: "#27272a",
                        fontSize: 10,
                        fontWeight: 400,
                        borderRight: "1px solid #111114",
                      }}
                    >
                      {actualRowNum}
                    </td>
                    {data.columns.map((col) => {
                      const isEditing = editing?.rowIdx === rowIdx && editing?.column === col.name;
                      const cellValue = row[col.name];
                      const isNull = cellValue === null || cellValue === undefined;
                      const displayValue = isNull ? "NULL" : String(cellValue);
                      const isBool = typeof cellValue === "boolean" || displayValue === "true" || displayValue === "false" || displayValue === "0" || displayValue === "1";
                      const isNumeric = typeof cellValue === "number";

                      return (
                        <td
                          key={col.name}
                          onDoubleClick={() => {
                            if (!pkColumn) return;
                            setEditing({
                              rowIdx,
                              column: col.name,
                              value: isNull ? "" : String(cellValue),
                            });
                          }}
                          style={{
                            padding: "7px 14px",
                            maxWidth: 320,
                            overflow: "hidden",
                            textOverflow: "ellipsis",
                            whiteSpace: "nowrap",
                            color: isNull ? "#3f3f46" : isNumeric ? "#60a5fa" : "#d4d4d8",
                            fontStyle: isNull ? "italic" : "normal",
                            cursor: pkColumn ? "cell" : "default",
                            fontSize: 12,
                            position: "relative",
                          }}
                        >
                          {isEditing ? (
                            <div style={{ display: "flex", gap: 4, alignItems: "center" }}>
                              <input
                                autoFocus
                                value={editing.value}
                                onChange={(e) =>
                                  setEditing({ ...editing, value: e.target.value })
                                }
                                onKeyDown={(e) => {
                                  if (e.key === "Enter") handleSave();
                                  if (e.key === "Escape") setEditing(null);
                                }}
                                style={{
                                  flex: 1,
                                  padding: "4px 8px",
                                  background: "#09090b",
                                  color: "#e4e4e7",
                                  border: "1px solid #3b82f6",
                                  borderRadius: 6,
                                  fontSize: 12,
                                  fontFamily: "'JetBrains Mono', monospace",
                                  outline: "none",
                                  boxShadow: "0 0 0 3px rgba(59,130,246,0.1)",
                                }}
                              />
                              <button
                                onClick={handleSave}
                                disabled={saving}
                                style={{
                                  padding: "4px 10px",
                                  background: "rgba(34,197,94,0.1)",
                                  color: "#4ade80",
                                  border: "1px solid rgba(34,197,94,0.2)",
                                  borderRadius: 5,
                                  cursor: saving ? "wait" : "pointer",
                                  fontSize: 10,
                                  fontWeight: 600,
                                  fontFamily: "'Inter', sans-serif",
                                }}
                              >
                                {saving ? "..." : "Save"}
                              </button>
                              <button
                                onClick={() => setEditing(null)}
                                style={{
                                  padding: "4px 10px",
                                  background: "rgba(239,68,68,0.1)",
                                  color: "#fca5a5",
                                  border: "1px solid rgba(239,68,68,0.2)",
                                  borderRadius: 5,
                                  cursor: "pointer",
                                  fontSize: 10,
                                  fontWeight: 600,
                                  fontFamily: "'Inter', sans-serif",
                                }}
                              >
                                Esc
                              </button>
                            </div>
                          ) : (
                            <span>
                              {isNull ? (
                                <span
                                  style={{
                                    padding: "1px 5px",
                                    borderRadius: 3,
                                    background: "rgba(113,113,122,0.06)",
                                    border: "1px solid #1c1c22",
                                    fontSize: 10,
                                    fontWeight: 500,
                                    letterSpacing: "0.03em",
                                  }}
                                >
                                  NULL
                                </span>
                              ) : isBool && (displayValue === "true" || displayValue === "1") ? (
                                <span style={{ color: "#4ade80" }}>{displayValue}</span>
                              ) : isBool && (displayValue === "false" || displayValue === "0") ? (
                                <span style={{ color: "#f87171" }}>{displayValue}</span>
                              ) : (
                                displayValue
                              )}
                            </span>
                          )}
                        </td>
                      );
                    })}
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>

      {/* Status bar / Pagination */}
      <div
        style={{
          padding: "8px 20px",
          borderTop: "1px solid #1c1c22",
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          fontSize: 11,
          background: "#0c0c10",
          color: "#52525b",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
          <span>
            Showing {startRow}-{endRow} of {data.total.toLocaleString()}
          </span>
          {searchFilter && (
            <span style={{ color: "#3b82f6" }}>
              ({filteredRows.length} match{filteredRows.length !== 1 ? "es" : ""})
            </span>
          )}
          {pkColumn && (
            <span style={{ color: "#3f3f46" }}>
              Double-click to edit
            </span>
          )}
        </div>

        {totalPages > 1 && (
          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <button
              onClick={() => setPage(1)}
              disabled={page <= 1}
              style={{
                padding: "4px 8px",
                background: page <= 1 ? "#111114" : "#18181b",
                color: page <= 1 ? "#27272a" : "#71717a",
                border: `1px solid ${page <= 1 ? "#1c1c22" : "#27272a"}`,
                borderRadius: 5,
                cursor: page <= 1 ? "not-allowed" : "pointer",
                fontSize: 11,
              }}
            >
              {"<<"}
            </button>
            <button
              onClick={() => setPage((p) => Math.max(1, p - 1))}
              disabled={page <= 1}
              style={{
                padding: "4px 10px",
                background: page <= 1 ? "#111114" : "#18181b",
                color: page <= 1 ? "#27272a" : "#71717a",
                border: `1px solid ${page <= 1 ? "#1c1c22" : "#27272a"}`,
                borderRadius: 5,
                cursor: page <= 1 ? "not-allowed" : "pointer",
                fontSize: 11,
                fontWeight: 500,
              }}
            >
              Prev
            </button>

            {/* Page numbers */}
            {(() => {
              const pages: number[] = [];
              const range = 2;
              for (let i = Math.max(1, page - range); i <= Math.min(totalPages, page + range); i++) {
                pages.push(i);
              }
              return pages.map((p) => (
                <button
                  key={p}
                  onClick={() => setPage(p)}
                  style={{
                    padding: "4px 9px",
                    background: p === page
                      ? "linear-gradient(135deg, rgba(59,130,246,0.15), rgba(139,92,246,0.1))"
                      : "#111114",
                    color: p === page ? "#93c5fd" : "#52525b",
                    border: `1px solid ${p === page ? "rgba(59,130,246,0.3)" : "#1c1c22"}`,
                    borderRadius: 5,
                    cursor: "pointer",
                    fontSize: 11,
                    fontWeight: p === page ? 600 : 400,
                    minWidth: 28,
                  }}
                >
                  {p}
                </button>
              ));
            })()}

            <button
              onClick={() => setPage((p) => Math.min(totalPages, p + 1))}
              disabled={page >= totalPages}
              style={{
                padding: "4px 10px",
                background: page >= totalPages ? "#111114" : "#18181b",
                color: page >= totalPages ? "#27272a" : "#71717a",
                border: `1px solid ${page >= totalPages ? "#1c1c22" : "#27272a"}`,
                borderRadius: 5,
                cursor: page >= totalPages ? "not-allowed" : "pointer",
                fontSize: 11,
                fontWeight: 500,
              }}
            >
              Next
            </button>
            <button
              onClick={() => setPage(totalPages)}
              disabled={page >= totalPages}
              style={{
                padding: "4px 8px",
                background: page >= totalPages ? "#111114" : "#18181b",
                color: page >= totalPages ? "#27272a" : "#71717a",
                border: `1px solid ${page >= totalPages ? "#1c1c22" : "#27272a"}`,
                borderRadius: 5,
                cursor: page >= totalPages ? "not-allowed" : "pointer",
                fontSize: 11,
              }}
            >
              {">>"}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
