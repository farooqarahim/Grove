import { useState } from "react";
import { updateConversationTitle, archiveConversation, deleteConversation } from "@/lib/api";
import { C } from "@/lib/theme";
import { XIcon } from "@/components/ui/icons";
import type { ConversationRow } from "@/types";

interface SessionSettingsModalProps {
  conversation: ConversationRow;
  onClose: () => void;
  onUpdated: () => void;
  onDeleted: () => void;
}

export function SessionSettingsModal({ conversation, onClose, onUpdated, onDeleted }: SessionSettingsModalProps) {
  const [titleInput, setTitleInput] = useState(conversation.title ?? "");
  const [saving, setSaving] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [deleting, setDeleting] = useState(false);

  const handleSaveTitle = async () => {
    const trimmed = titleInput.trim();
    if (!trimmed || trimmed === (conversation.title ?? "")) { onClose(); return; }
    setSaving(true);
    try {
      await updateConversationTitle(conversation.id, trimmed);
      onUpdated();
      onClose();
    } finally {
      setSaving(false);
    }
  };

  const handleArchive = async () => {
    await archiveConversation(conversation.id);
    onUpdated();
    onClose();
  };

  const handleDelete = async () => {
    if (!confirmDelete) { setConfirmDelete(true); return; }
    setDeleting(true);
    try {
      await deleteConversation(conversation.id);
      onDeleted();
      onClose();
    } finally {
      setDeleting(false);
    }
  };

  return (
    <div
      style={{
        position: "fixed", inset: 0, zIndex: 200,
        display: "flex", alignItems: "center", justifyContent: "center",
        background: "rgba(0,0,0,0.55)",
      }}
      onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      <div style={{
        width: 380, borderRadius: 10,
        background: C.surface, border: `1px solid ${C.border}`,
        padding: "20px 22px",
      }}>
        {/* Header */}
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 18 }}>
          <span style={{ fontSize: 13, fontWeight: 600, color: C.text1 }}>Session Settings</span>
          <button
            onClick={onClose}
            className="btn-ghost"
            style={{ background: "transparent", color: C.text4, cursor: "pointer", padding: "3px 6px", borderRadius: 6 }}
          >
            <XIcon size={12} />
          </button>
        </div>

        {/* Title rename */}
        <div style={{ marginBottom: 16 }}>
          <div style={{
            fontSize: 10, fontWeight: 600, color: C.text4,
            textTransform: "uppercase", letterSpacing: "0.06em", marginBottom: 6,
          }}>
            Title
          </div>
          <input
            value={titleInput}
            onChange={e => setTitleInput(e.target.value)}
            onKeyDown={e => { if (e.key === "Enter") handleSaveTitle(); if (e.key === "Escape") onClose(); }}
            autoFocus
            style={{
              width: "100%", padding: "8px 10px", borderRadius: 6,
              background: C.surfaceHover, border: `1px solid ${C.border}`,
              color: C.text1, fontSize: 12, outline: "none",
              boxSizing: "border-box",
            }}
          />
        </div>

        <div style={{ display: "flex", gap: 12, marginBottom: 16 }}>
          <div style={{ flex: 1 }}>
            <div style={{
              fontSize: 10, fontWeight: 600, color: C.text4,
              textTransform: "uppercase", letterSpacing: "0.06em", marginBottom: 6,
            }}>
              Type
            </div>
            <div style={{ fontSize: 12, color: C.text2 }}>
              {conversation.conversation_kind === "cli" ? "CLI-based" : "Run-based"}
            </div>
          </div>
          {conversation.conversation_kind === "cli" && (
            <div style={{ flex: 1 }}>
              <div style={{
                fontSize: 10, fontWeight: 600, color: C.text4,
                textTransform: "uppercase", letterSpacing: "0.06em", marginBottom: 6,
              }}>
                CLI
              </div>
              <div style={{ fontSize: 12, color: C.text2 }}>
                {conversation.cli_provider ?? "Unknown"}
                {conversation.cli_model ? ` · ${conversation.cli_model}` : ""}
              </div>
            </div>
          )}
        </div>

        <button
          onClick={handleSaveTitle}
          disabled={saving}
          style={{
            width: "100%", padding: "8px 0", borderRadius: 6,
            background: C.accent, border: "none", color: "#000",
            fontSize: 12, fontWeight: 600, cursor: "pointer",
            marginBottom: 16, opacity: saving ? 0.6 : 1,
          }}
        >
          {saving ? "Saving…" : "Save"}
        </button>

        {/* Divider */}
        <div style={{ height: 1, background: C.border, marginBottom: 12 }} />

        {/* Archive / Delete */}
        <div style={{ display: "flex", gap: 6 }}>
          {conversation.state === "active" && (
            <button
              onClick={handleArchive}
              className="btn-ghost"
              style={{
                flex: 1, padding: "7px 0", borderRadius: 6,
                background: C.surfaceHover, border: "none",
                color: C.text3, fontSize: 11, cursor: "pointer",
              }}
            >
              Archive
            </button>
          )}
          <button
            onClick={handleDelete}
            disabled={deleting}
            style={{
              flex: 1, padding: "7px 0", borderRadius: 6,
              background: confirmDelete ? "#EF4444" : "rgba(239,68,68,0.08)",
              border: "none",
              color: confirmDelete ? "#fff" : "#EF4444",
              fontSize: 11, cursor: "pointer",
              fontWeight: confirmDelete ? 600 : 400,
              transition: "all 0.12s",
              opacity: deleting ? 0.5 : 1,
            }}
          >
            {deleting ? "Deleting…" : confirmDelete ? "Confirm Delete" : "Delete Session"}
          </button>
        </div>
      </div>
    </div>
  );
}
