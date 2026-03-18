import React, { useState } from "react";
import { ChevronR } from "@/components/ui/icons";
import { C } from "@/lib/theme";

const FILE_STATUS_COLORS: Record<string, string> = {
  A: "#31B97B",
  M: "#3B82F6",
  D: "#EF4444",
  R: "#F59E0B",
  "?": "#A78BFA",
};

export interface TreeNode {
  n: string;       // display name (filename or folder name)
  t: "d" | "f";   // directory or file
  s?: string;      // status char (A/M/D/R/?) — files only
  p?: string;      // full repo-relative path — files only, used as selection key
  a?: string;      // area: "staged"|"unstaged"|"untracked"|"committed"
  committed?: boolean;
  c?: TreeNode[];  // children — dirs only
}

interface TNodeProps {
  node: TreeNode;
  depth?: number;
  selected: string;
  onSelect?: (path: string) => void;
  interactive?: boolean;
  fileColorMode?: "status" | "plain";
  renderRight?: (path: string) => React.ReactNode;
}

function TNode({
  node,
  depth = 0,
  selected,
  onSelect,
  interactive = true,
  fileColorMode = "status",
  renderRight,
}: TNodeProps) {
  const [open, setOpen] = useState(true);
  const isDir = node.t === "d";
  const key = node.p ?? node.n;
  const isActive = interactive && !isDir && selected === key;
  const isClickable = isDir || (!!onSelect && interactive);
  const statusColor = node.s ? FILE_STATUS_COLORS[node.s] ?? C.text2 : C.text2;
  const fileColor = fileColorMode === "plain" ? C.text1 : statusColor;

  return (
    <div>
      <div
        onClick={() => {
          if (isDir) {
            setOpen(!open);
            return;
          }
          if (interactive) onSelect?.(key);
        }}
        className={isActive || !isClickable ? "" : "hover-row"}
        style={{
          display: "flex",
          alignItems: "center",
          gap: 4,
          padding: "3px 6px",
          paddingLeft: depth * 14 + 6,
          borderRadius: 4,
          cursor: isClickable ? "pointer" : "default",
          background: isActive ? C.surfaceActive : "transparent",
          fontSize: 11,
          fontFamily: C.mono,
          color: isActive ? C.text1 : isDir ? C.text2 : fileColor,
          opacity: node.committed ? 0.7 : 1,
        }}
      >
        {isDir ? (
          <span
            style={{
              color: C.text4,
              transform: open ? "rotate(90deg)" : "",
              transition: "transform 0.1s",
              display: "flex",
            }}
          >
            <ChevronR size={9} />
          </span>
        ) : (
          <span style={{ width: 9 }} />
        )}
        <span
          style={{
            flex: 1,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {node.n}
        </span>
        {node.s && !isDir && !renderRight && (
          <span
            style={{
              fontSize: 8,
              fontWeight: 700,
              color: statusColor,
              opacity: 0.8,
              flexShrink: 0,
            }}
          >
            {node.s === "?" ? "U" : node.s}
          </span>
        )}
        {node.committed && !renderRight && (
          <span style={{ fontSize: 8, color: "rgba(49,185,123,0.7)", flexShrink: 0 }} title="Committed">
            ✓
          </span>
        )}
        {!isDir && renderRight && node.p && (
          <span onClick={e => e.stopPropagation()} style={{ display: "flex", alignItems: "center", gap: 4 }}>
            {renderRight(node.p)}
          </span>
        )}
      </div>
      {isDir && open && node.c?.map((child, i) => (
        <TNode
          key={child.p ?? `${child.n}-${i}`}
          node={child}
          depth={depth + 1}
          selected={selected}
          onSelect={onSelect}
          interactive={interactive}
          fileColorMode={fileColorMode}
          renderRight={renderRight}
        />
      ))}
    </div>
  );
}

interface FileTreeProps {
  tree: TreeNode[];
  selected: string;
  onSelect?: (path: string) => void;
  interactive?: boolean;
  fileColorMode?: "status" | "plain";
  /** Optional per-file slot rendered on the right of each file row (checkboxes, stats, etc.) */
  renderRight?: (path: string) => React.ReactNode;
}

export function FileTree({
  tree,
  selected,
  onSelect,
  interactive = true,
  fileColorMode = "status",
  renderRight,
}: FileTreeProps) {
  return (
    <div style={{ padding: "4px 4px" }}>
      {tree.map((node, i) => (
        <TNode
          key={node.p ?? `${node.n}-${i}`}
          node={node}
          selected={selected}
          onSelect={onSelect}
          interactive={interactive}
          fileColorMode={fileColorMode}
          renderRight={renderRight}
        />
      ))}
    </div>
  );
}

/// Build a nested folder tree from a flat list of changed files.
export function buildFileTree(
  files: ReadonlyArray<{ path: string; status: string; committed?: boolean; area?: string }>,
): TreeNode[] {
  const root: TreeNode = { n: "", t: "d", c: [] };

  for (const file of files) {
    const parts = file.path.split("/");
    let node = root;

    // Walk/create intermediate directory nodes.
    for (let i = 0; i < parts.length - 1; i++) {
      let child = node.c?.find(c => c.t === "d" && c.n === parts[i]);
      if (!child) {
        child = { n: parts[i], t: "d", c: [] };
        (node.c ??= []).push(child);
      }
      node = child;
    }

    // Add the file node.
    (node.c ??= []).push({
      n: parts[parts.length - 1],
      t: "f",
      s: file.status.charAt(0),
      p: file.path,
      a: (file as { area?: string }).area,
      committed: (file as { committed?: boolean }).committed,
    });
  }

  const sortTree = (nodes: TreeNode[]) => {
    nodes.sort((a, b) => {
      if (a.t !== b.t) return a.t === "d" ? -1 : 1;
      return a.n.localeCompare(b.n);
    });
    nodes.forEach((node) => {
      if (node.c?.length) sortTree(node.c);
    });
  };

  sortTree(root.c ?? []);

  return root.c ?? [];
}
