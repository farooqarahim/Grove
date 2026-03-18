import type { CanonicalStatus, Issue, ProjectSettings } from "@/types";
import {
  CANONICAL_SEQUENCE,
  COLUMN_CONFIGS,
  PROVIDER_STATUS_ORDER,
  type DisplayColumn,
} from "./constants";

export function compositeId(issue: Issue): string {
  return issue.id ?? `${issue.provider}:${issue.external_id}`;
}

export function formatRelative(ts: string): string {
  const d = new Date(ts);
  if (isNaN(d.getTime())) return ts;
  const diff = Date.now() - d.getTime();
  const m = Math.floor(diff / 60000);
  if (m < 1) return "just now";
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const days = Math.floor(h / 24);
  if (days < 7) return `${days}d ago`;
  if (days < 30) return `${Math.floor(days / 7)}w ago`;
  return d.toLocaleDateString();
}

export function normalizePriority(p: string | null | undefined): string {
  if (!p) return "None";
  const map: Record<string, string> = { urgent: "Critical", high: "High", medium: "Medium", low: "Low" };
  return map[p] ?? "None";
}

export function displayProvider(provider: string): string {
  return provider.charAt(0).toUpperCase() + provider.slice(1);
}

export function emptyProjectSettings(): ProjectSettings {
  return {
    default_provider: null,
    default_project_key: null,
    max_parallel_agents: null,
    default_pipeline: null,
    default_permission_mode: null,
    issue_board: null,
  };
}

export function metadataPretty(value: unknown): string | null {
  if (value == null) return null;
  if (typeof value === "object" && !Array.isArray(value) && Object.keys(value as Record<string, unknown>).length === 0) {
    return null;
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return null;
  }
}

export function parseCsvValue(value: string): string[] {
  return value
    .split(",")
    .map((entry) => entry.trim())
    .filter(Boolean);
}

export function canonicalForIssue(issue: Issue): CanonicalStatus {
  const value = issue.canonical_status;
  if (value && CANONICAL_SEQUENCE.includes(value as CanonicalStatus)) {
    return value as CanonicalStatus;
  }
  const normalized = issue.status.trim().toLowerCase();
  if (normalized.includes("review") || normalized.includes("qa") || normalized.includes("test")) return "in_review";
  if (normalized.includes("progress") || normalized.includes("started") || normalized.includes("active")) return "in_progress";
  if (normalized.includes("block") || normalized.includes("hold") || normalized.includes("wait")) return "blocked";
  if (normalized.includes("done") || normalized.includes("closed") || normalized.includes("resolved") || normalized.includes("fixed")) return "done";
  if (normalized.includes("cancel") || normalized.includes("duplicate") || normalized.includes("wont")) return "cancelled";
  return "open";
}

export function providerFromSource(source: string): string | null {
  switch (source) {
    case "GitHub": return "github";
    case "Jira": return "jira";
    case "Linear": return "linear";
    case "Grove": return "grove";
    case "Linter": return "linter";
    default: return null;
  }
}

function prettifyStatus(status: string): string {
  return status
    .replace(/[_-]+/g, " ")
    .replace(/\s+/g, " ")
    .trim()
    .replace(/\b\w/g, (m) => m.toUpperCase());
}

function compareProviderStatuses(a: string, b: string, provider: string): number {
  const ordered = PROVIDER_STATUS_ORDER[provider] ?? [];
  const ai = ordered.indexOf(a);
  const bi = ordered.indexOf(b);
  if (ai !== -1 || bi !== -1) {
    if (ai === -1) return 1;
    if (bi === -1) return -1;
    return ai - bi;
  }
  return a.localeCompare(b);
}

export function buildConfiguredColumns(
  allColumns: { id: string; canonical_status: CanonicalStatus; label: string; issues: Issue[]; count: number }[],
  filterIssue: (issue: Issue) => boolean,
): DisplayColumn[] {
  return allColumns.map((column) => {
    const cfg = COLUMN_CONFIGS[column.canonical_status] ?? { label: column.label, dot: "#6b7280" };
    const issues = column.issues.filter(filterIssue);
    return {
      id: column.id,
      title: column.label,
      accent: cfg.dot,
      issues,
      count: issues.length,
      canonicalStatus: column.canonical_status,
    };
  });
}

export function buildProviderColumns(
  allIssues: Issue[],
  provider: string,
  filterIssue: (issue: Issue) => boolean,
): DisplayColumn[] {
  const groups = new Map<string, { title: string; issues: Issue[]; canonical: CanonicalStatus }>();

  for (const issue of allIssues) {
    const rawKey = issue.status.trim().toLowerCase() || "open";
    const canonical = canonicalForIssue(issue);
    const existing = groups.get(rawKey);
    if (existing) {
      existing.issues.push(issue);
      continue;
    }
    groups.set(rawKey, {
      title: issue.status.trim() || prettifyStatus(rawKey),
      issues: [issue],
      canonical,
    });
  }

  if (groups.size === 0) {
    return CANONICAL_SEQUENCE.map((status) => {
      const cfg = COLUMN_CONFIGS[status];
      return {
        id: `${provider}-${status}`,
        title: cfg.label,
        subtitle: "Observed statuses will appear here",
        accent: cfg.dot,
        issues: [],
        count: 0,
        canonicalStatus: status,
        provider,
      };
    });
  }

  return Array.from(groups.entries())
    .sort(([a], [b]) => compareProviderStatuses(a, b, provider))
    .map(([rawStatus, value]) => {
      const cfg = COLUMN_CONFIGS[value.canonical] ?? { dot: "#6b7280", label: value.canonical };
      const issues = value.issues.filter(filterIssue);
      return {
        id: `${provider}-${rawStatus}`,
        title: prettifyStatus(value.title),
        subtitle: cfg.label,
        accent: cfg.dot,
        issues,
        count: issues.length,
        canonicalStatus: value.canonical,
        provider,
      };
    });
}
