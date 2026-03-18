import { FileTree } from "@/components/ui/FileTree";
import { Folder } from "@/components/ui/icons";
import { C } from "@/lib/theme";
import type { FileDiffEntry } from "@/types";
import type { TreeNode } from "@/components/ui/FileTree";
import { EmptyState } from "./EmptyState";

interface ChangedFilesPanelProps {
  changedTree: TreeNode[];
  changedFileCount: number;
  fileMetaByPath: Map<string, FileDiffEntry>;
  gitSource: string;
}

export function ChangedFilesPanel({
  changedTree,
  changedFileCount,
  fileMetaByPath,
  gitSource,
}: ChangedFilesPanelProps) {
  return (
    <>
      <div style={{ flex: 1, minHeight: 0, overflowY: "auto", padding: "0 14px 14px" }}>
        {changedFileCount > 0 ? (
          <div style={{ padding: 0 }}>
            <FileTree
              tree={changedTree}
              selected=""
              interactive={false}
              fileColorMode="plain"
              renderRight={(path) => {
                const file = fileMetaByPath.get(path);
                if (!file) return null;
                const areaLabel = file.area === "committed"
                  ? "committed"
                  : file.area === "staged"
                    ? "staged"
                    : file.area === "untracked"
                      ? "untracked"
                      : "unstaged";
                return (
                  <>
                    <span style={{
                      fontSize: 9,
                      color: C.text1,
                      background: "rgba(255,255,255,0.06)",
                      padding: "2px 5px",
                      borderRadius: 2,
                    }}>
                      {file.status.charAt(0) || "M"}
                    </span>
                    <span style={{
                      fontSize: 9,
                      color: file.area === "committed" ? C.warn : C.text2,
                      background: file.area === "committed" ? C.warnDim : "rgba(255,255,255,0.04)",
                      padding: "2px 5px",
                      borderRadius: 2,
                    }}>
                      {areaLabel}
                    </span>
                  </>
                );
              }}
            />
          </div>
        ) : (
          <EmptyState
            icon={<Folder size={18} />}
            title="No changed files"
            description={gitSource === "project" ? "This workspace is clean." : "There are no pending file changes in this session."}
          />
        )}
      </div>
    </>
  );
}
