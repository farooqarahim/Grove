import { useRef, useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { relativeTime } from "@/lib/hooks";
import { qk } from "@/lib/queryKeys";
import { listMessages } from "@/lib/api";
import { C } from "@/lib/theme";
import type { MessageRow } from "@/types";

interface MessageListProps {
  conversationId: string;
}

export function MessageList({ conversationId }: MessageListProps) {
  const [msgLimit, setMsgLimit] = useState(500);
  const { data: messages } = useQuery({
    queryKey: qk.messages(conversationId, msgLimit),
    queryFn: () => listMessages(conversationId, msgLimit),
    refetchInterval: 60000,
    staleTime: 30000,
  });
  const [copied, setCopied] = useState(false);

  const bottomRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages?.length]);

  const copyAll = () => {
    if (!messages) return;
    const text = messages.map(m => {
      const prefix = m.agent_type ? `[${m.role}/${m.agent_type}]` : `[${m.role}]`;
      return `${prefix} ${m.content}`;
    }).join("\n\n");
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }).catch(() => {});
  };

  if (!messages || messages.length === 0) {
    return (
      <div style={{ padding: "16px 0", textAlign: "center", color: C.text4, fontSize: 11 }}>
        No messages
      </div>
    );
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 0, padding: "8px 0" }}>
      <div className="flex justify-between items-center">
        {messages.length >= msgLimit ? (
          <button
            onClick={() => setMsgLimit(prev => prev + 500)}
            className="action-btn text-2xs cursor-pointer"
            style={{
              padding: "3px 8px", borderRadius: 4,
              background: "transparent", border: "none",
              color: C.text4,
            }}
          >
            Load earlier...
          </button>
        ) : <span />}
        <button
          onClick={copyAll}
          className="action-btn text-2xs cursor-pointer"
          style={{
            padding: "3px 8px", borderRadius: 4,
            background: "transparent", border: "none",
            color: copied ? C.accent : C.text4,
          }}
        >
          {copied ? "Copied!" : "Copy all"}
        </button>
      </div>
      {messages.map(msg => (
        <MessageBubble key={msg.id} message={msg} />
      ))}
      <div ref={bottomRef} />
    </div>
  );
}

function MessageBubble({ message }: { message: MessageRow }) {
  const isUser = message.role === "user";
  const isSystem = message.role === "system";

  if (isSystem) {
    return (
      <div style={{
        padding: "8px 0",
        display: "flex", alignItems: "center", gap: 10,
      }}>
        <div style={{ flex: 1, height: 1, background: "rgba(255,255,255,0.04)" }} />
        <span style={{ fontSize: 10, color: C.text4, fontStyle: "italic", flexShrink: 0 }}>
          {message.content.length > 80 ? message.content.slice(0, 80) + "…" : message.content}
        </span>
        <div style={{ flex: 1, height: 1, background: "rgba(255,255,255,0.04)" }} />
      </div>
    );
  }

  const accentColor = isUser ? C.accent : C.blue;
  const borderColor = isUser ? "rgba(49,185,123,0.2)" : "rgba(59,130,246,0.12)";
  const roleLabel = message.agent_type
    ? `${message.role} · ${message.agent_type}`
    : message.role;

  return (
    <div style={{
      padding: "8px 0 8px 12px",
      borderLeft: `2px solid ${borderColor}`,
      marginBottom: 8,
    }}>
      <div style={{ display: "flex", alignItems: "baseline", gap: 6, marginBottom: 4 }}>
        <span style={{
          fontSize: 10, fontWeight: 600, color: accentColor,
          textTransform: "capitalize", fontFamily: C.mono,
        }}>
          {roleLabel}
        </span>
        <span style={{ fontSize: 9, color: C.text4 }}>{relativeTime(message.created_at)}</span>
      </div>
      <div style={{
        fontSize: 11, color: C.text2,
        whiteSpace: "pre-wrap", wordBreak: "break-word",
        lineHeight: 1.6,
      }}>
        {message.content}
      </div>
    </div>
  );
}
