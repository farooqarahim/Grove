export interface DatabaseEntry {
  id: string;
  name: string;
  path: string;
}

export interface ColumnInfo {
  cid: number;
  name: string;
  col_type: string;
  notnull: boolean;
  default_value: string | null;
  pk: boolean;
}

export interface TableRows {
  columns: ColumnInfo[];
  rows: Record<string, unknown>[];
  total: number;
  page: number;
  page_size: number;
}

const BASE = "/api";

export async function fetchDatabases(): Promise<DatabaseEntry[]> {
  const res = await fetch(`${BASE}/databases`);
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function fetchTables(db: string): Promise<string[]> {
  const res = await fetch(`${BASE}/tables?db=${encodeURIComponent(db)}`);
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function fetchSchema(
  db: string,
  table: string
): Promise<ColumnInfo[]> {
  const res = await fetch(
    `${BASE}/schema/${encodeURIComponent(table)}?db=${encodeURIComponent(db)}`
  );
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function fetchRows(
  db: string,
  table: string,
  page: number = 1,
  size: number = 50,
  sort?: string,
  order?: string
): Promise<TableRows> {
  let url = `${BASE}/rows/${encodeURIComponent(table)}?db=${encodeURIComponent(db)}&page=${page}&size=${size}`;
  if (sort) url += `&sort=${encodeURIComponent(sort)}`;
  if (order) url += `&order=${encodeURIComponent(order)}`;
  const res = await fetch(url);
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function updateRow(
  db: string,
  table: string,
  pkValue: string,
  pkColumn: string,
  updates: Record<string, unknown>
): Promise<{ affected: number }> {
  const res = await fetch(
    `${BASE}/rows/${encodeURIComponent(table)}/${encodeURIComponent(pkValue)}`,
    {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ db, pk_column: pkColumn, updates }),
    }
  );
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}
