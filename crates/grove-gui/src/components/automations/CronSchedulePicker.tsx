import { useState, useEffect, useMemo } from "react";
import { C, lbl } from "@/lib/theme";

/* ── 7-field cron: sec min hour dom month dow year ──────────────────────── */

type Frequency = "hourly" | "daily" | "weekly" | "monthly";

const DAYS_OF_WEEK = [
  { short: "Mon", full: "Monday", cron: "MON" },
  { short: "Tue", full: "Tuesday", cron: "TUE" },
  { short: "Wed", full: "Wednesday", cron: "WED" },
  { short: "Thu", full: "Thursday", cron: "THU" },
  { short: "Fri", full: "Friday", cron: "FRI" },
  { short: "Sat", full: "Saturday", cron: "SAT" },
  { short: "Sun", full: "Sunday", cron: "SUN" },
];

const HOURS = Array.from({ length: 24 }, (_, i) => i);
const MINUTES = [0, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55];
const MONTH_DAYS = Array.from({ length: 31 }, (_, i) => i + 1);

function pad2(n: number): string {
  return n.toString().padStart(2, "0");
}

function formatHour(h: number): string {
  if (h === 0) return "12 AM";
  if (h < 12) return `${h} AM`;
  if (h === 12) return "12 PM";
  return `${h - 12} PM`;
}

function buildCron(freq: Frequency, hour: number, minute: number, daysOfWeek: string[], dayOfMonth: number): string {
  const sec = "0";
  const min = minute.toString();
  const hr = hour.toString();
  const year = "*";

  switch (freq) {
    case "hourly":
      return `${sec} ${min} * * * * ${year}`;
    case "daily":
      return `${sec} ${min} ${hr} * * * ${year}`;
    case "weekly": {
      const dow = daysOfWeek.length > 0 ? daysOfWeek.join(",") : "*";
      return `${sec} ${min} ${hr} * * ${dow} ${year}`;
    }
    case "monthly":
      return `${sec} ${min} ${hr} ${dayOfMonth} * * ${year}`;
  }
}

function describeSchedule(freq: Frequency, hour: number, minute: number, daysOfWeek: string[], dayOfMonth: number): string {
  const time = `${pad2(hour % 12 || 12)}:${pad2(minute)} ${hour < 12 ? "AM" : "PM"}`;

  switch (freq) {
    case "hourly":
      return minute === 0
        ? "Every hour on the hour"
        : `Every hour at :${pad2(minute)}`;
    case "daily":
      return `Every day at ${time}`;
    case "weekly": {
      if (daysOfWeek.length === 0) return `Every week (no days selected) at ${time}`;
      const names = daysOfWeek.map(d => DAYS_OF_WEEK.find(dw => dw.cron === d)?.full ?? d);
      if (names.length === 1) return `Every ${names[0]} at ${time}`;
      if (names.length === 7) return `Every day at ${time}`;
      const last = names.pop()!;
      return `Every ${names.join(", ")} and ${last} at ${time}`;
    }
    case "monthly": {
      const suffix = dayOfMonth === 1 ? "st" : dayOfMonth === 2 ? "nd" : dayOfMonth === 3 ? "rd" : dayOfMonth >= 21 && dayOfMonth <= 23
        ? dayOfMonth === 21 ? "st" : dayOfMonth === 22 ? "nd" : "rd"
        : dayOfMonth === 31 ? "st" : "th";
      return `Monthly on the ${dayOfMonth}${suffix} at ${time}`;
    }
  }
}

/** Try to parse an existing cron expression into picker state. */
function parseCron(expr: string): { freq: Frequency; hour: number; minute: number; daysOfWeek: string[]; dayOfMonth: number } | null {
  const parts = expr.trim().split(/\s+/);
  if (parts.length !== 7) return null;
  const [, min, hr, dom, , dow] = parts;

  const minute = parseInt(min, 10);
  if (isNaN(minute)) return null;

  // hourly: * in hour position
  if (hr === "*") {
    return { freq: "hourly", hour: 9, minute, daysOfWeek: [], dayOfMonth: 1 };
  }

  const hour = parseInt(hr, 10);
  if (isNaN(hour)) return null;

  // monthly: specific day of month
  if (dom !== "*") {
    const d = parseInt(dom, 10);
    if (!isNaN(d) && d >= 1 && d <= 31) {
      return { freq: "monthly", hour, minute, daysOfWeek: [], dayOfMonth: d };
    }
  }

  // weekly: specific day(s) of week
  if (dow !== "*") {
    const days = dow.split(",").filter(d => DAYS_OF_WEEK.some(dw => dw.cron === d));
    if (days.length > 0) {
      return { freq: "weekly", hour, minute, daysOfWeek: days, dayOfMonth: 1 };
    }
  }

  // daily
  return { freq: "daily", hour, minute, daysOfWeek: [], dayOfMonth: 1 };
}

/* ── Styles ─────────────────────────────────────────────────────────────── */

const pillBase: React.CSSProperties = {
  padding: "6px 14px",
  borderRadius: 8,
  border: "none",
  cursor: "pointer",
  fontFamily: "inherit",
  fontSize: 12,
  fontWeight: 600,
  transition: "all .15s",
};

const selectStyle: React.CSSProperties = {
  height: 34,
  padding: "0 10px",
  borderRadius: 6,
  border: `1px solid ${C.border}`,
  background: C.surfaceHover,
  color: C.text1,
  fontSize: 13,
  fontFamily: "inherit",
  outline: "none",
  cursor: "pointer",
  transition: "border-color .15s",
  appearance: "none",
  WebkitAppearance: "none",
  backgroundImage: `url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='6' fill='none'%3E%3Cpath d='M1 1l4 4 4-4' stroke='%2364748b' stroke-width='1.5' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E")`,
  backgroundRepeat: "no-repeat",
  backgroundPosition: "right 10px center",
  paddingRight: 28,
};

const chipBase: React.CSSProperties = {
  width: 36,
  height: 36,
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  borderRadius: 8,
  border: `1px solid ${C.border}`,
  cursor: "pointer",
  fontFamily: "inherit",
  fontSize: 12,
  fontWeight: 600,
  transition: "all .12s",
  padding: 0,
};

/* ── Component ──────────────────────────────────────────────────────────── */

interface Props {
  value: string;
  onChange: (cron: string) => void;
}

export function CronSchedulePicker({ value, onChange }: Props) {
  // Parse existing value (if any) to seed state
  const parsed = useMemo(() => parseCron(value), [value]);

  const [freq, setFreq] = useState<Frequency>(parsed?.freq ?? "daily");
  const [hour, setHour] = useState(parsed?.hour ?? 9);
  const [minute, setMinute] = useState(parsed?.minute ?? 0);
  const [daysOfWeek, setDaysOfWeek] = useState<string[]>(parsed?.daysOfWeek ?? ["MON"]);
  const [dayOfMonth, setDayOfMonth] = useState(parsed?.dayOfMonth ?? 1);
  const [showAdvanced, setShowAdvanced] = useState(false);

  // Emit cron whenever state changes
  useEffect(() => {
    const cron = buildCron(freq, hour, minute, daysOfWeek, dayOfMonth);
    if (cron !== value) {
      onChange(cron);
    }
  }, [freq, hour, minute, daysOfWeek, dayOfMonth]);

  const description = describeSchedule(freq, hour, minute, daysOfWeek, dayOfMonth);

  function toggleDay(day: string) {
    setDaysOfWeek(prev =>
      prev.includes(day) ? prev.filter(d => d !== day) : [...prev, day],
    );
  }

  if (showAdvanced) {
    return (
      <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
          <div style={lbl}>Cron Expression (7-field)</div>
          <button
            onClick={() => setShowAdvanced(false)}
            style={{
              background: "none",
              border: "none",
              color: C.accent,
              fontSize: 11,
              fontWeight: 600,
              cursor: "pointer",
              fontFamily: "inherit",
              padding: 0,
            }}
          >
            Visual Editor
          </button>
        </div>
        <input
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder="0 0 9 * * MON *"
          style={{
            width: "100%",
            height: 36,
            padding: "0 12px",
            borderRadius: 6,
            border: `1px solid ${C.border}`,
            background: C.surfaceHover,
            color: C.text1,
            fontSize: 13,
            fontFamily: C.mono,
            outline: "none",
            boxSizing: "border-box",
            transition: "border-color .15s",
            letterSpacing: "0.04em",
          }}
          onFocus={(e) => { e.currentTarget.style.borderColor = C.accent; }}
          onBlur={(e) => { e.currentTarget.style.borderColor = C.border; }}
        />
        <div style={{ fontSize: 11, color: "#64748b", lineHeight: 1.4 }}>
          Format: sec min hour day-of-month month day-of-week year
        </div>
      </div>
    );
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
      {/* Header row */}
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
        <div style={{ ...lbl, marginBottom: 0 }}>Schedule</div>
        <button
          onClick={() => setShowAdvanced(true)}
          style={{
            background: "none",
            border: "none",
            color: "#64748b",
            fontSize: 11,
            fontWeight: 600,
            cursor: "pointer",
            fontFamily: "inherit",
            padding: 0,
            transition: "color .12s",
          }}
          onMouseEnter={(e) => { e.currentTarget.style.color = C.accent; }}
          onMouseLeave={(e) => { e.currentTarget.style.color = "#64748b"; }}
        >
          Advanced
        </button>
      </div>

      {/* Frequency selector */}
      <div
        style={{
          display: "flex",
          background: C.surfaceHover,
          borderRadius: 8,
          border: `1px solid ${C.border}`,
          padding: 3,
        }}
      >
        {(["hourly", "daily", "weekly", "monthly"] as Frequency[]).map((f) => {
          const active = freq === f;
          return (
            <button
              key={f}
              onClick={() => setFreq(f)}
              style={{
                ...pillBase,
                flex: 1,
                background: active ? C.accent : "transparent",
                color: active ? "#fff" : "#64748b",
              }}
            >
              {f.charAt(0).toUpperCase() + f.slice(1)}
            </button>
          );
        })}
      </div>

      {/* Time selectors */}
      {freq !== "hourly" && (
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{ fontSize: 12, color: "#64748b", fontWeight: 500, whiteSpace: "nowrap" }}>at</span>
          <select
            value={hour}
            onChange={(e) => setHour(parseInt(e.target.value, 10))}
            style={{ ...selectStyle, width: 100 }}
          >
            {HOURS.map(h => (
              <option key={h} value={h}>{formatHour(h)}</option>
            ))}
          </select>
          <span style={{ fontSize: 14, color: "#64748b", fontWeight: 600 }}>:</span>
          <select
            value={minute}
            onChange={(e) => setMinute(parseInt(e.target.value, 10))}
            style={{ ...selectStyle, width: 72 }}
          >
            {MINUTES.map(m => (
              <option key={m} value={m}>{pad2(m)}</option>
            ))}
          </select>
        </div>
      )}

      {/* Hourly — minute selector only */}
      {freq === "hourly" && (
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{ fontSize: 12, color: "#64748b", fontWeight: 500, whiteSpace: "nowrap" }}>at minute</span>
          <select
            value={minute}
            onChange={(e) => setMinute(parseInt(e.target.value, 10))}
            style={{ ...selectStyle, width: 72 }}
          >
            {MINUTES.map(m => (
              <option key={m} value={m}>{pad2(m)}</option>
            ))}
          </select>
          <span style={{ fontSize: 11, color: "#64748b" }}>past each hour</span>
        </div>
      )}

      {/* Day of week chips */}
      {freq === "weekly" && (
        <div>
          <div style={{ ...lbl, marginBottom: 8 }}>Days</div>
          <div style={{ display: "flex", gap: 6 }}>
            {DAYS_OF_WEEK.map(d => {
              const active = daysOfWeek.includes(d.cron);
              return (
                <button
                  key={d.cron}
                  onClick={() => toggleDay(d.cron)}
                  title={d.full}
                  style={{
                    ...chipBase,
                    background: active ? C.accentDim : C.surfaceHover,
                    color: active ? C.accent : "#64748b",
                    borderColor: active ? "rgba(49,185,123,0.35)" : C.border,
                  }}
                  onMouseEnter={(e) => {
                    if (!active) e.currentTarget.style.borderColor = C.borderHover;
                  }}
                  onMouseLeave={(e) => {
                    e.currentTarget.style.borderColor = active ? "rgba(49,185,123,0.35)" : C.border;
                  }}
                >
                  {d.short.charAt(0)}
                </button>
              );
            })}
          </div>
        </div>
      )}

      {/* Day of month grid */}
      {freq === "monthly" && (
        <div>
          <div style={{ ...lbl, marginBottom: 8 }}>Day of Month</div>
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(7, 1fr)",
              gap: 4,
            }}
          >
            {MONTH_DAYS.map(d => {
              const active = dayOfMonth === d;
              return (
                <button
                  key={d}
                  onClick={() => setDayOfMonth(d)}
                  style={{
                    ...chipBase,
                    width: "100%",
                    height: 32,
                    fontSize: 11,
                    borderRadius: 6,
                    background: active ? C.accentDim : C.surfaceHover,
                    color: active ? C.accent : "#64748b",
                    borderColor: active ? "rgba(49,185,123,0.35)" : C.border,
                  }}
                  onMouseEnter={(e) => {
                    if (!active) e.currentTarget.style.borderColor = C.borderHover;
                  }}
                  onMouseLeave={(e) => {
                    e.currentTarget.style.borderColor = active ? "rgba(49,185,123,0.35)" : C.border;
                  }}
                >
                  {d}
                </button>
              );
            })}
          </div>
        </div>
      )}

      {/* Human-readable description */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          padding: "8px 12px",
          borderRadius: 8,
          background: C.accentMuted,
          border: `1px solid rgba(49,185,123,0.15)`,
        }}
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke={C.accent} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <circle cx="12" cy="12" r="10" />
          <polyline points="12 6 12 12 16 14" />
        </svg>
        <span style={{ fontSize: 12, color: C.accent, fontWeight: 600 }}>
          {description}
        </span>
      </div>
    </div>
  );
}
