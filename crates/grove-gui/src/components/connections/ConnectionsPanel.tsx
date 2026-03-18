import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { qk } from "@/lib/queryKeys";
import { checkConnections, connectProvider, disconnectProvider } from "@/lib/api";
import { C } from "@/lib/theme";
import { Check } from "@/components/ui/icons";
import type { ConnectionStatus } from "@/types";

// ── Provider definitions ──────────────────────────────────────────────────────

interface FieldDef {
  key: string;
  label: string;
  type: "text" | "email" | "password";
  placeholder?: string;
  hint?: string;
}

interface ProviderDef {
  id: string;
  name: string;
  abbr: string;
  color: string;
  description: string;
  helpUrl: string;
  helpLabel: string;
  fields: FieldDef[];
  supportsCustomStorage?: boolean;
}

type CredentialStorage = "keychain" | "file";

const PROVIDERS: ProviderDef[] = [
  {
    id: "github",
    name: "GitHub",
    abbr: "GH",
    color: "#24292e",
    description: "Sync issues from GitHub repositories.",
    helpUrl: "https://github.com/settings/tokens/new",
    helpLabel: "Create a token at github.com/settings/tokens",
    fields: [
      {
        key: "token",
        label: "Personal Access Token",
        type: "password",
        placeholder: "ghp_xxxxxxxxxxxx",
        hint: "Requires repo and read:org scopes",
      },
    ],
  },
  {
    id: "jira",
    name: "Jira",
    abbr: "JR",
    color: "#0052CC",
    description: "Sync issues from Jira Cloud projects.",
    helpUrl: "https://id.atlassian.com/manage-profile/security/api-tokens",
    helpLabel: "Create an API token at id.atlassian.com",
    supportsCustomStorage: true,
    fields: [
      {
        key: "site",
        label: "Site URL",
        type: "text",
        placeholder: "https://your-company.atlassian.net",
        hint: "Your Jira Cloud instance URL",
      },
      {
        key: "email",
        label: "Atlassian Email",
        type: "email",
        placeholder: "you@company.com",
        hint: "The email address of your Atlassian account",
      },
      {
        key: "token",
        label: "API Token",
        type: "password",
        placeholder: "ATATT3x...",
        hint: "From id.atlassian.com/manage-profile/security/api-tokens",
      },
    ],
  },
  {
    id: "linear",
    name: "Linear",
    abbr: "LN",
    color: "#5E6AD2",
    description: "Sync issues and manage Linear teams.",
    helpUrl: "https://linear.app/settings/api",
    helpLabel: "Create a Personal API key at linear.app/settings/api",
    supportsCustomStorage: true,
    fields: [
      {
        key: "token",
        label: "Personal API Key",
        type: "password",
        placeholder: "lin_api_xxxxxxxxxxxx",
        hint: "From linear.app/settings/api — use a Personal key, not an OAuth app",
      },
    ],
  },
];

// ── Component ─────────────────────────────────────────────────────────────────

export function ConnectionsPanel() {
  const queryClient = useQueryClient();
  const { data: statuses } = useQuery({
    queryKey: qk.connections(),
    queryFn: checkConnections,
    refetchInterval: 30000,
    staleTime: 15000,
  });

  // Which provider form is expanded for connection.
  const [expanded, setExpanded] = useState<string | null>(null);
  // Per-provider form data — keyed by provider id so switching providers
  // doesn't mix fields from different providers.
  const [formData, setFormData] = useState<Record<string, Record<string, string>>>({});
  const [storageByProvider, setStorageByProvider] = useState<
    Record<string, CredentialStorage>
  >({});
  // Async operation states.
  const [saving, setSaving] = useState<string | null>(null);
  const [disconnecting, setDisconnecting] = useState<string | null>(null);
  const [confirmDisconnect, setConfirmDisconnect] = useState<string | null>(null);
  // Per-provider error and success feedback.
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [successFor, setSuccessFor] = useState<string | null>(null);

  const getStatus = (id: string): ConnectionStatus | undefined =>
    statuses?.find((s) => s.provider === id);

  const getField = (providerId: string, key: string): string =>
    formData[providerId]?.[key] ?? "";

  const setField = (providerId: string, key: string, value: string) => {
    setFormData((prev) => ({
      ...prev,
      [providerId]: { ...prev[providerId], [key]: value },
    }));
  };

  const getStorage = (providerId: string): CredentialStorage =>
    storageByProvider[providerId] ?? "keychain";

  const setStorage = (providerId: string, storage: CredentialStorage) => {
    setStorageByProvider((prev) => ({ ...prev, [providerId]: storage }));
  };

  const clearError = (providerId: string) =>
    setErrors((prev) => ({ ...prev, [providerId]: "" }));

  const isFormComplete = (prov: ProviderDef): boolean =>
    prov.fields.every((f) => getField(prov.id, f.key).trim().length > 0);

  const handleConnect = async (prov: ProviderDef) => {
    clearError(prov.id);
    setSaving(prov.id);
    try {
      const credentials: Record<string, string> = {};
      for (const f of prov.fields) {
        credentials[f.key] = getField(prov.id, f.key).trim();
      }
      await connectProvider(
        prov.id,
        credentials,
        prov.supportsCustomStorage ? getStorage(prov.id) : "keychain",
      );
      // Clear form and collapse on success.
      setFormData((prev) => ({ ...prev, [prov.id]: {} }));
      setExpanded(null);
      setSuccessFor(prov.id);
      setTimeout(() => setSuccessFor(null), 3000);
      void queryClient.invalidateQueries({ queryKey: qk.connections() });
    } catch (e) {
      setErrors((prev) => ({
        ...prev,
        [prov.id]: e instanceof Error ? e.message : String(e),
      }));
    } finally {
      setSaving(null);
    }
  };

  const handleDisconnect = async (providerId: string) => {
    if (confirmDisconnect !== providerId) {
      setConfirmDisconnect(providerId);
      return;
    }
    setConfirmDisconnect(null);
    setDisconnecting(providerId);
    try {
      await disconnectProvider(providerId);
      void queryClient.invalidateQueries({ queryKey: qk.connections() });
    } catch (e) {
      setErrors((prev) => ({
        ...prev,
        [providerId]: e instanceof Error ? e.message : String(e),
      }));
    } finally {
      setDisconnecting(null);
    }
  };

  const toggleExpand = (providerId: string) => {
    setExpanded((prev) => (prev === providerId ? null : providerId));
    setConfirmDisconnect(null);
    clearError(providerId);
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      {PROVIDERS.map((prov) => {
        const status = getStatus(prov.id);
        const connected = status?.connected ?? false;
        const hasError = !connected && !!status?.error;
        const isExpanded = expanded === prov.id;
        const isSaving = saving === prov.id;
        const isDisconnecting = disconnecting === prov.id;
        const isConfirming = confirmDisconnect === prov.id;
        const didSucceed = successFor === prov.id;
        const formError = errors[prov.id];

        return (
          <div
            key={prov.id}
            style={{
              borderRadius: 8,
              border: `1px solid ${connected ? "rgba(49,185,123,0.25)" : C.border}`,
              background: C.surfaceHover,
              overflow: "hidden",
            }}
          >
            {/* Header row */}
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: 10,
                padding: "10px 14px",
              }}
            >
              {/* Provider badge */}
              <div
                style={{
                  width: 28,
                  height: 28,
                  borderRadius: 6,
                  background: prov.color,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  flexShrink: 0,
                  fontSize: 9,
                  fontWeight: 800,
                  color: "#fff",
                  letterSpacing: "0.02em",
                }}
              >
                {prov.abbr}
              </div>

              {/* Name + status */}
              <div style={{ flex: 1, minWidth: 0 }}>
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 8,
                  }}
                >
                  <span
                    style={{
                      fontSize: 12,
                      fontWeight: 600,
                      color: C.text1,
                    }}
                  >
                    {prov.name}
                  </span>
                  {/* Connection status dot */}
                  <span
                    style={{
                      width: 6,
                      height: 6,
                      borderRadius: "50%",
                      background: connected
                        ? "#31B97B"
                        : hasError
                          ? "#EF4444"
                          : C.text4,
                      flexShrink: 0,
                    }}
                  />
                  {/* Connected user or status badge */}
                  {connected && status?.user_display ? (
                    <span style={{ fontSize: 10, color: C.text3 }}>
                      {status.user_display}
                    </span>
                  ) : (
                    <span
                      style={{
                        fontSize: 9,
                        padding: "1px 6px",
                        borderRadius: 4,
                        background: connected
                          ? "rgba(49,185,123,0.12)"
                          : "rgba(156,163,175,0.12)",
                        color: connected ? "#31B97B" : C.text4,
                        fontWeight: 600,
                      }}
                    >
                      {connected ? "Connected" : "Not connected"}
                    </span>
                  )}
                  {didSucceed && (
                    <span
                      style={{
                        fontSize: 10,
                        color: "#31B97B",
                        display: "flex",
                        alignItems: "center",
                        gap: 3,
                      }}
                    >
                      <Check size={10} /> Connected!
                    </span>
                  )}
                </div>
                {/* Error — shown for both connected (expired token) and disconnected (keychain failure) */}
                {status?.error && (
                  <div
                    style={{
                      fontSize: 10,
                      color: "#EF4444",
                      marginTop: 2,
                    }}
                  >
                    {status.error}
                  </div>
                )}
              </div>

              {/* Actions */}
              {connected ? (
                <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
                  {isConfirming && (
                    <span
                      style={{ fontSize: 10, color: C.text3 }}
                    >
                      Sure?
                    </span>
                  )}
                  <button
                    onClick={() => handleDisconnect(prov.id)}
                    disabled={isDisconnecting}
                    style={{
                      padding: "3px 9px",
                      borderRadius: 6,
                      fontSize: 9,
                      fontWeight: 600,
                      border: "none",
                      cursor: isDisconnecting ? "default" : "pointer",
                      background: isConfirming ? "#EF4444" : C.surfaceActive,
                      color: isConfirming ? "#fff" : C.text3,
                      opacity: isDisconnecting ? 0.5 : 1,
                    }}
                  >
                    {isDisconnecting
                      ? "Disconnecting…"
                      : isConfirming
                        ? "Confirm"
                        : "Disconnect"}
                  </button>
                  {isConfirming && (
                    <button
                      onClick={() => setConfirmDisconnect(null)}
                      style={{
                        padding: "3px 9px",
                        borderRadius: 6,
                        fontSize: 9,
                        border: "none",
                        cursor: "pointer",
                        background: "transparent",
                        color: C.text4,
                      }}
                    >
                      Cancel
                    </button>
                  )}
                </div>
              ) : (
                <button
                  onClick={() => toggleExpand(prov.id)}
                  style={{
                    padding: "3px 10px",
                    borderRadius: 6,
                    fontSize: 9,
                    fontWeight: 600,
                    border: "none",
                    cursor: "pointer",
                    background: isExpanded ? C.surfaceActive : C.accent,
                    color: isExpanded ? C.text3 : "#fff",
                  }}
                >
                  {isExpanded ? "Cancel" : "Connect"}
                </button>
              )}
            </div>

            {/* Expandable form */}
            {isExpanded && !connected && (
              <div
                style={{
                  padding: "0 14px 14px",
                  display: "flex",
                  flexDirection: "column",
                  gap: 10,
                  borderTop: `1px solid ${C.border}`,
                  paddingTop: 12,
                }}
              >
                {/* Description + help link */}
                <div
                  style={{
                    fontSize: 11,
                    color: C.text3,
                    lineHeight: 1.5,
                  }}
                >
                  {prov.description}{" "}
                  <a
                    href={prov.helpUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    style={{
                      color: C.accent,
                      textDecoration: "none",
                      fontSize: 10,
                    }}
                  >
                    {prov.helpLabel} ↗
                  </a>
                </div>

                {prov.supportsCustomStorage && (
                  <div
                    style={{
                      display: "flex",
                      flexDirection: "column",
                      gap: 6,
                      padding: "8px 10px",
                      borderRadius: 6,
                      border: `1px solid ${C.border}`,
                      background: C.base,
                    }}
                  >
                    <div
                      style={{
                        fontSize: 10,
                        fontWeight: 600,
                        color: C.text3,
                      }}
                    >
                      Credential storage
                    </div>
                    <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
                      {([
                        {
                          value: "keychain" as const,
                          label: "OS Keychain",
                        },
                        {
                          value: "file" as const,
                          label: "Grove App File",
                        },
                      ]).map((option) => {
                        const selected = getStorage(prov.id) === option.value;
                        return (
                          <button
                            key={option.value}
                            type="button"
                            onClick={() => setStorage(prov.id, option.value)}
                            style={{
                              padding: "6px 10px",
                              borderRadius: 6,
                              border: `1px solid ${selected ? C.accent : C.border}`,
                              background: selected ? "rgba(59,130,246,0.12)" : C.surfaceHover,
                              color: selected ? C.text1 : C.text3,
                              fontSize: 10,
                              fontWeight: 600,
                              cursor: "pointer",
                            }}
                          >
                            {option.label}
                          </button>
                        );
                      })}
                    </div>
                    <div style={{ fontSize: 9, color: C.text4, lineHeight: 1.5 }}>
                      {getStorage(prov.id) === "keychain"
                        ? "Use the operating system credential store."
                        : "Store outside the project in ~/.grove/workspaces/_global/tracker_credentials.json."}
                    </div>
                  </div>
                )}

                {/* Fields */}
                {prov.fields.map((field) => (
                  <div key={field.key}>
                    <div
                      style={{
                        display: "flex",
                        alignItems: "baseline",
                        gap: 6,
                        marginBottom: 4,
                      }}
                    >
                      <span
                        style={{
                          fontSize: 10,
                          fontWeight: 600,
                          color: C.text3,
                        }}
                      >
                        {field.label}
                      </span>
                      {field.hint && (
                        <span style={{ fontSize: 9, color: C.text4 }}>
                          — {field.hint}
                        </span>
                      )}
                    </div>
                    <input
                      type={field.type}
                      value={getField(prov.id, field.key)}
                      onChange={(e) =>
                        setField(prov.id, field.key, e.target.value)
                      }
                      onKeyDown={(e) => {
                        if (
                          e.key === "Enter" &&
                          isFormComplete(prov) &&
                          !isSaving
                        ) {
                          void handleConnect(prov);
                        }
                      }}
                      placeholder={field.placeholder}
                      autoComplete="off"
                      spellCheck={false}
                      style={{
                        width: "100%",
                        padding: "6px 10px",
                        borderRadius: 6,
                        background: C.base,
                        border: `1px solid ${C.border}`,
                        color: C.text1,
                        fontSize: 11,
                        outline: "none",
                        fontFamily:
                          field.type === "password" ? C.mono : undefined,
                        boxSizing: "border-box",
                      }}
                    />
                  </div>
                ))}

                {/* Error message */}
                {formError && (
                  <div
                    style={{
                      fontSize: 10,
                      color: "#EF4444",
                      padding: "6px 10px",
                      background: "rgba(239,68,68,0.08)",
                      borderRadius: 6,
                      border: "1px solid rgba(239,68,68,0.2)",
                      lineHeight: 1.5,
                    }}
                  >
                    {formError}
                  </div>
                )}

                {/* Save button */}
                <button
                  onClick={() => void handleConnect(prov)}
                  disabled={!isFormComplete(prov) || isSaving}
                  style={{
                    padding: "6px 14px",
                    borderRadius: 6,
                    fontSize: 11,
                    fontWeight: 600,
                    border: "none",
                    cursor:
                      !isFormComplete(prov) || isSaving
                        ? "default"
                        : "pointer",
                    background: C.accent,
                    color: "#fff",
                    alignSelf: "flex-start",
                    opacity: !isFormComplete(prov) || isSaving ? 0.5 : 1,
                  }}
                >
                  {isSaving ? "Connecting…" : "Save & Connect"}
                </button>
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
