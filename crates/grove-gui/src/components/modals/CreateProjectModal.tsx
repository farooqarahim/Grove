import { useEffect, useState, type CSSProperties } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Folder, XIcon } from "@/components/ui/icons";
import { createProjectFromSource, type ProjectCreateRequest } from "@/lib/api";
import { C, lbl } from "@/lib/theme";

type CreateMode =
  | "open_folder"
  | "clone_git_repo"
  | "create_repo"
  | "fork_repo_to_remote"
  | "fork_folder_to_folder"
  | "ssh";

const MODE_LABELS: Record<CreateMode, { title: string; subtitle: string; action: string }> = {
  open_folder: {
    title: "Open Folder",
    subtitle: "Register an existing local directory.",
    action: "Open",
  },
  clone_git_repo: {
    title: "Clone Repo",
    subtitle: "Clone any GitHub, GitLab, or Bitbucket repo into a local path.",
    action: "Clone",
  },
  create_repo: {
    title: "Create Repo",
    subtitle: "Create a new remote repo and local checkout.",
    action: "Create",
  },
  fork_repo_to_remote: {
    title: "Fork Repo",
    subtitle: "Copy a local git repo into a new folder and new remote repo.",
    action: "Fork",
  },
  fork_folder_to_folder: {
    title: "Fork Folder",
    subtitle: "Copy a local folder into a new project folder.",
    action: "Fork",
  },
  ssh: {
    title: "SSH Machine",
    subtitle: "Register a remote machine and path for shell access.",
    action: "Create",
  },
};

const GITIGNORE_TEMPLATES = [
  { value: "", label: "None" },
  { value: "node", label: "Node / TypeScript" },
  { value: "python", label: "Python" },
  { value: "rust", label: "Rust" },
  { value: "go", label: "Go" },
  { value: "java", label: "Java" },
];

const REPO_PROVIDERS = [
  { value: "github", label: "GitHub" },
  { value: "gitlab", label: "GitLab" },
  { value: "bitbucket", label: "Bitbucket" },
];

interface CreateProjectModalProps {
  open: boolean;
  onClose: () => void;
  onCreated: () => void;
}

function parseRepoName(input: string): string {
  const cleaned = input.trim().replace(/\/+$/, "");
  if (!cleaned) return "";
  const last = cleaned.split("/").pop() ?? "";
  return last.replace(/\.git$/i, "");
}

function basename(input: string): string {
  const cleaned = input.trim().replace(/\/+$/, "");
  if (!cleaned) return "";
  const parts = cleaned.split("/");
  return parts[parts.length - 1] ?? "";
}

export function CreateProjectModal({ open, onClose, onCreated }: CreateProjectModalProps) {
  const [mode, setMode] = useState<CreateMode>("open_folder");
  const [name, setName] = useState("");
  const [rootPath, setRootPath] = useState("");

  const [repoUrl, setRepoUrl] = useState("");
  const [targetPath, setTargetPath] = useState("");

  const [repoProvider, setRepoProvider] = useState("github");
  const [repoName, setRepoName] = useState("");
  const [owner, setOwner] = useState("");
  const [visibility, setVisibility] = useState("private");
  const [gitignoreTemplate, setGitignoreTemplate] = useState("");
  const [gitignoreEntries, setGitignoreEntries] = useState("");

  const [sourcePath, setSourcePath] = useState("");
  const [remoteName, setRemoteName] = useState("origin");
  const [preserveGit, setPreserveGit] = useState(false);

  const [sshHost, setSshHost] = useState("");
  const [sshUser, setSshUser] = useState("");
  const [sshPort, setSshPort] = useState("22");
  const [sshRemotePath, setSshRemotePath] = useState("");

  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    if (open) return;
    setMode("open_folder");
    setName("");
    setRootPath("");
    setRepoUrl("");
    setTargetPath("");
    setRepoProvider("github");
    setRepoName("");
    setOwner("");
    setVisibility("private");
    setGitignoreTemplate("");
    setGitignoreEntries("");
    setSourcePath("");
    setRemoteName("origin");
    setPreserveGit(false);
    setSshHost("");
    setSshUser("");
    setSshPort("22");
    setSshRemotePath("");
    setError(null);
    setSubmitting(false);
  }, [open]);

  if (!open) return null;

  const splitGitignoreEntries = (): string[] =>
    gitignoreEntries
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean);

  const buildRequest = (): ProjectCreateRequest | null => {
    if (mode === "open_folder") {
      if (!rootPath.trim()) {
        setError("Root path is required");
        return null;
      }
      return {
        kind: "open_folder",
        root_path: rootPath.trim(),
        name: name.trim() || null,
      };
    }

    if (mode === "clone_git_repo") {
      if (!repoUrl.trim()) {
        setError("Repository URL is required");
        return null;
      }
      if (!targetPath.trim()) {
        setError("Target path is required");
        return null;
      }
      return {
        kind: "clone_git_repo",
        repo_url: repoUrl.trim(),
        target_path: targetPath.trim(),
        name: name.trim() || null,
      };
    }

    if (mode === "create_repo") {
      if (!repoName.trim()) {
        setError("Repository name is required");
        return null;
      }
      if (!targetPath.trim()) {
        setError("Local checkout path is required");
        return null;
      }
      return {
        kind: "create_repo",
        provider: repoProvider,
        repo_name: repoName.trim(),
        target_path: targetPath.trim(),
        owner: owner.trim() || null,
        visibility,
        gitignore_template: gitignoreTemplate || null,
        gitignore_entries: splitGitignoreEntries(),
        name: name.trim() || null,
      };
    }

    if (mode === "fork_repo_to_remote") {
      if (!sourcePath.trim()) {
        setError("Source repo path is required");
        return null;
      }
      if (!targetPath.trim()) {
        setError("Target path is required");
        return null;
      }
      if (!repoName.trim()) {
        setError("Repository name is required");
        return null;
      }
      return {
        kind: "fork_repo_to_remote",
        provider: repoProvider,
        source_path: sourcePath.trim(),
        target_path: targetPath.trim(),
        repo_name: repoName.trim(),
        owner: owner.trim() || null,
        visibility,
        remote_name: remoteName.trim() || null,
        name: name.trim() || null,
      };
    }

    if (mode === "fork_folder_to_folder") {
      if (!sourcePath.trim()) {
        setError("Source folder path is required");
        return null;
      }
      if (!targetPath.trim()) {
        setError("Target path is required");
        return null;
      }
      return {
        kind: "fork_folder_to_folder",
        source_path: sourcePath.trim(),
        target_path: targetPath.trim(),
        preserve_git: preserveGit,
        name: name.trim() || null,
      };
    }

    if (!sshHost.trim()) {
      setError("SSH host is required");
      return null;
    }
    if (!sshRemotePath.trim()) {
      setError("SSH remote path is required");
      return null;
    }
    const parsedPort = sshPort.trim() ? Number.parseInt(sshPort.trim(), 10) : 22;
    if (Number.isNaN(parsedPort) || parsedPort <= 0) {
      setError("SSH port must be a positive number");
      return null;
    }
    return {
      kind: "ssh",
      host: sshHost.trim(),
      remote_path: sshRemotePath.trim(),
      user: sshUser.trim() || null,
      port: parsedPort,
      name: name.trim() || null,
    };
  };

  const handleSubmit = async () => {
    setError(null);
    const request = buildRequest();
    if (!request) return;

    setSubmitting(true);
    try {
      await createProjectFromSource(request);
      onCreated();
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSubmitting(false);
    }
  };

  const handleBrowseFolder = async (
    onSelect: (selected: string) => void,
    title: string,
  ) => {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title,
    });
    if (selected) onSelect(selected as string);
  };

  const fieldStyle: CSSProperties = {
    width: "100%",
    padding: "9px 12px",
    borderRadius: 6,
    background: C.base,
    color: C.text1,
    fontSize: 12,
    outline: "none",
    boxSizing: "border-box",
    border: "none",
  };

  const selectStyle: CSSProperties = {
    ...fieldStyle,
    appearance: "none",
    cursor: "pointer",
  };

  const requestReady = (() => {
    if (mode === "open_folder") return Boolean(rootPath.trim());
    if (mode === "clone_git_repo") return Boolean(repoUrl.trim() && targetPath.trim());
    if (mode === "create_repo") return Boolean(repoName.trim() && targetPath.trim());
    if (mode === "fork_repo_to_remote") {
      return Boolean(sourcePath.trim() && targetPath.trim() && repoName.trim());
    }
    if (mode === "fork_folder_to_folder") {
      return Boolean(sourcePath.trim() && targetPath.trim());
    }
    return Boolean(sshHost.trim() && sshRemotePath.trim());
  })();

  const subtitle = MODE_LABELS[mode].subtitle;
  const actionLabel = submitting ? `${MODE_LABELS[mode].action}ing...` : MODE_LABELS[mode].action;

  return (
    <div
      onClick={onClose}
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 100,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "rgba(0,0,0,0.5)",
        backdropFilter: "blur(8px)",
        WebkitBackdropFilter: "blur(8px)",
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          width: 720,
          maxWidth: "calc(100vw - 32px)",
          maxHeight: "calc(100vh - 40px)",
          background: C.surface,
          borderRadius: 10,
          overflow: "hidden",
        }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            padding: "16px 20px",
            background: C.surfaceHover,
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <div
              style={{
                width: 28,
                height: 28,
                borderRadius: 6,
                background: C.accentDim,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
              }}
            >
              <span style={{ color: C.accent }}>
                <Folder size={13} />
              </span>
            </div>
            <div>
              <div style={{ fontSize: 14, fontWeight: 700, color: C.text1 }}>New Project</div>
              <div style={{ fontSize: 10, color: C.text4 }}>{subtitle}</div>
            </div>
          </div>
          <button
            onClick={onClose}
            style={{ background: "none", color: C.text4, cursor: "pointer", padding: 4, border: "none" }}
          >
            <XIcon size={12} />
          </button>
        </div>

        <div style={{ padding: 20, display: "flex", flexDirection: "column", gap: 16, overflowY: "auto", maxHeight: "calc(100vh - 180px)" }}>
          <div style={{ display: "grid", gridTemplateColumns: "repeat(3, minmax(0, 1fr))", gap: 8 }}>
            {(Object.keys(MODE_LABELS) as CreateMode[]).map((key) => {
              const active = key === mode;
              return (
                <button
                  key={key}
                  onClick={() => {
                    setMode(key);
                    setError(null);
                  }}
                  style={{
                    padding: "9px 10px",
                    borderRadius: 6,
                    border: "none",
                    cursor: "pointer",
                    background: active ? C.accent : C.base,
                    color: active ? "#fff" : C.text2,
                    fontSize: 11,
                    fontWeight: 700,
                  }}
                >
                  {MODE_LABELS[key].title}
                </button>
              );
            })}
          </div>

          {mode === "open_folder" && (
            <>
              <div>
                <div style={lbl}>Root Path</div>
                <div style={{ display: "flex", gap: 6 }}>
                  <input
                    value={rootPath}
                    onChange={(e) => {
                      const value = e.target.value;
                      setRootPath(value);
                      if (!name.trim()) setName(basename(value));
                    }}
                    placeholder="/Users/you/projects/my-app"
                    autoFocus
                    onKeyDown={(e) => {
                      if (e.key === "Enter") handleSubmit();
                    }}
                    style={{ ...fieldStyle, fontFamily: C.mono, flex: 1 }}
                  />
                  <button
                    onClick={() =>
                      handleBrowseFolder((selected) => {
                        setRootPath(selected);
                        if (!name.trim()) setName(basename(selected));
                      }, "Select project root folder")
                    }
                    style={{
                      padding: "9px 14px",
                      borderRadius: 6,
                      background: C.surfaceHover,
                      color: C.text2,
                      fontSize: 11,
                      fontWeight: 600,
                      cursor: "pointer",
                      border: "none",
                    }}
                  >
                    Browse
                  </button>
                </div>
              </div>
              <div>
                <div style={lbl}>Project Name</div>
                <input
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="Defaults to folder name"
                  style={fieldStyle}
                />
              </div>
            </>
          )}

          {mode === "clone_git_repo" && (
            <>
              <div>
                <div style={lbl}>Repository URL</div>
                <input
                  value={repoUrl}
                  onChange={(e) => setRepoUrl(e.target.value)}
                  placeholder="git@gitlab.com:team/repo.git"
                  autoFocus
                  style={{ ...fieldStyle, fontFamily: C.mono }}
                />
              </div>
              <div>
                <div style={lbl}>Target Path</div>
                <div style={{ display: "flex", gap: 6 }}>
                  <input
                    value={targetPath}
                    onChange={(e) => {
                      const value = e.target.value;
                      setTargetPath(value);
                      if (!name.trim()) setName(basename(value));
                    }}
                    placeholder="/Users/you/projects/repo-name"
                    style={{ ...fieldStyle, fontFamily: C.mono, flex: 1 }}
                  />
                  <button
                    onClick={() =>
                      handleBrowseFolder((selected) => {
                        const repoLeaf = parseRepoName(repoUrl) || "repo";
                        const resolved = `${selected.replace(/\/+$/, "")}/${repoLeaf}`;
                        setTargetPath(resolved);
                        if (!name.trim()) setName(repoLeaf);
                      }, "Select parent folder")
                    }
                    style={{
                      padding: "9px 14px",
                      borderRadius: 6,
                      background: C.surfaceHover,
                      color: C.text2,
                      fontSize: 11,
                      fontWeight: 600,
                      cursor: "pointer",
                      border: "none",
                    }}
                  >
                    Choose Parent
                  </button>
                </div>
              </div>
              <div>
                <div style={lbl}>Project Name</div>
                <input
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="Optional display name"
                  style={fieldStyle}
                />
              </div>
            </>
          )}

          {mode === "create_repo" && (
            <>
              <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr", gap: 12 }}>
                <div>
                  <div style={lbl}>Provider</div>
                  <select value={repoProvider} onChange={(e) => setRepoProvider(e.target.value)} style={selectStyle}>
                    {REPO_PROVIDERS.map((provider) => (
                      <option key={provider.value} value={provider.value}>
                        {provider.label}
                      </option>
                    ))}
                  </select>
                </div>
                <div>
                  <div style={lbl}>Repository Name</div>
                  <input
                    value={repoName}
                    onChange={(e) => {
                      const value = e.target.value;
                      setRepoName(value);
                      if (!name.trim()) setName(value);
                    }}
                    placeholder="my-new-repo"
                    autoFocus
                    style={fieldStyle}
                  />
                </div>
                <div>
                  <div style={lbl}>Owner / Org</div>
                  <input
                    value={owner}
                    onChange={(e) => setOwner(e.target.value)}
                    placeholder="Optional"
                    style={fieldStyle}
                  />
                </div>
              </div>
              <div>
                <div style={lbl}>Local Checkout Path</div>
                <div style={{ display: "flex", gap: 6 }}>
                  <input
                    value={targetPath}
                    onChange={(e) => setTargetPath(e.target.value)}
                    placeholder="/Users/you/projects/my-new-repo"
                    style={{ ...fieldStyle, fontFamily: C.mono, flex: 1 }}
                  />
                  <button
                    onClick={() =>
                      handleBrowseFolder((selected) => {
                        const leaf = repoName.trim() || "new-repo";
                        setTargetPath(`${selected.replace(/\/+$/, "")}/${leaf}`);
                      }, "Select parent folder")
                    }
                    style={{
                      padding: "9px 14px",
                      borderRadius: 6,
                      background: C.surfaceHover,
                      color: C.text2,
                      fontSize: 11,
                      fontWeight: 600,
                      cursor: "pointer",
                      border: "none",
                    }}
                  >
                    Choose Parent
                  </button>
                </div>
              </div>
              <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>
                <div>
                  <div style={lbl}>Visibility</div>
                  <select value={visibility} onChange={(e) => setVisibility(e.target.value)} style={selectStyle}>
                    <option value="private">Private</option>
                    <option value="public">Public</option>
                  </select>
                </div>
                <div>
                  <div style={lbl}>Gitignore Template</div>
                  <select
                    value={gitignoreTemplate}
                    onChange={(e) => setGitignoreTemplate(e.target.value)}
                    style={selectStyle}
                  >
                    {GITIGNORE_TEMPLATES.map((template) => (
                      <option key={template.value || "none"} value={template.value}>
                        {template.label}
                      </option>
                    ))}
                  </select>
                </div>
              </div>
              <div>
                <div style={lbl}>Extra Gitignore Entries</div>
                <textarea
                  value={gitignoreEntries}
                  onChange={(e) => setGitignoreEntries(e.target.value)}
                  placeholder={"Examples:\n.env.local\n.tmp/\ncoverage-final.json"}
                  rows={4}
                  style={{ ...fieldStyle, resize: "vertical", lineHeight: 1.5, fontFamily: C.mono }}
                />
              </div>
              <div>
                <div style={lbl}>Project Name</div>
                <input
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="Optional display name"
                  style={fieldStyle}
                />
              </div>
            </>
          )}

          {mode === "fork_repo_to_remote" && (
            <>
              <div>
                <div style={lbl}>Source Repo Path</div>
                <div style={{ display: "flex", gap: 6 }}>
                  <input
                    value={sourcePath}
                    onChange={(e) => {
                      const value = e.target.value;
                      setSourcePath(value);
                      if (!repoName.trim()) setRepoName(`${basename(value) || "repo"}-fork`);
                      if (!name.trim()) setName(`${basename(value) || "repo"} fork`);
                    }}
                    placeholder="/Users/you/projects/existing-repo"
                    autoFocus
                    style={{ ...fieldStyle, fontFamily: C.mono, flex: 1 }}
                  />
                  <button
                    onClick={() =>
                      handleBrowseFolder((selected) => {
                        setSourcePath(selected);
                        if (!repoName.trim()) setRepoName(`${basename(selected) || "repo"}-fork`);
                        if (!name.trim()) setName(`${basename(selected) || "repo"} fork`);
                      }, "Select source repo")
                    }
                    style={{
                      padding: "9px 14px",
                      borderRadius: 6,
                      background: C.surfaceHover,
                      color: C.text2,
                      fontSize: 11,
                      fontWeight: 600,
                      cursor: "pointer",
                      border: "none",
                    }}
                  >
                    Browse
                  </button>
                </div>
              </div>
              <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr", gap: 12 }}>
                <div>
                  <div style={lbl}>Provider</div>
                  <select value={repoProvider} onChange={(e) => setRepoProvider(e.target.value)} style={selectStyle}>
                    {REPO_PROVIDERS.map((provider) => (
                      <option key={provider.value} value={provider.value}>
                        {provider.label}
                      </option>
                    ))}
                  </select>
                </div>
                <div>
                  <div style={lbl}>Repository Name</div>
                  <input
                    value={repoName}
                    onChange={(e) => setRepoName(e.target.value)}
                    placeholder="my-forked-repo"
                    style={fieldStyle}
                  />
                </div>
                <div>
                  <div style={lbl}>Owner / Org</div>
                  <input
                    value={owner}
                    onChange={(e) => setOwner(e.target.value)}
                    placeholder="Optional"
                    style={fieldStyle}
                  />
                </div>
              </div>
              <div>
                <div style={lbl}>Target Path</div>
                <div style={{ display: "flex", gap: 6 }}>
                  <input
                    value={targetPath}
                    onChange={(e) => setTargetPath(e.target.value)}
                    placeholder="/Users/you/projects/my-forked-repo"
                    style={{ ...fieldStyle, fontFamily: C.mono, flex: 1 }}
                  />
                  <button
                    onClick={() =>
                      handleBrowseFolder((selected) => {
                        const leaf = repoName.trim() || `${basename(sourcePath) || "repo"}-fork`;
                        setTargetPath(`${selected.replace(/\/+$/, "")}/${leaf}`);
                      }, "Select parent folder")
                    }
                    style={{
                      padding: "9px 14px",
                      borderRadius: 6,
                      background: C.surfaceHover,
                      color: C.text2,
                      fontSize: 11,
                      fontWeight: 600,
                      cursor: "pointer",
                      border: "none",
                    }}
                  >
                    Choose Parent
                  </button>
                </div>
              </div>
              <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>
                <div>
                  <div style={lbl}>Visibility</div>
                  <select value={visibility} onChange={(e) => setVisibility(e.target.value)} style={selectStyle}>
                    <option value="private">Private</option>
                    <option value="public">Public</option>
                  </select>
                </div>
                <div>
                  <div style={lbl}>Remote Name</div>
                  <input
                    value={remoteName}
                    onChange={(e) => setRemoteName(e.target.value)}
                    placeholder="origin"
                    style={fieldStyle}
                  />
                </div>
              </div>
              <div>
                <div style={lbl}>Project Name</div>
                <input
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="Optional display name"
                  style={fieldStyle}
                />
              </div>
            </>
          )}

          {mode === "fork_folder_to_folder" && (
            <>
              <div>
                <div style={lbl}>Source Folder Path</div>
                <div style={{ display: "flex", gap: 6 }}>
                  <input
                    value={sourcePath}
                    onChange={(e) => {
                      const value = e.target.value;
                      setSourcePath(value);
                      if (!name.trim()) setName(`${basename(value) || "folder"} copy`);
                    }}
                    placeholder="/Users/you/projects/source-folder"
                    autoFocus
                    style={{ ...fieldStyle, fontFamily: C.mono, flex: 1 }}
                  />
                  <button
                    onClick={() =>
                      handleBrowseFolder((selected) => {
                        setSourcePath(selected);
                        if (!name.trim()) setName(`${basename(selected) || "folder"} copy`);
                      }, "Select source folder")
                    }
                    style={{
                      padding: "9px 14px",
                      borderRadius: 6,
                      background: C.surfaceHover,
                      color: C.text2,
                      fontSize: 11,
                      fontWeight: 600,
                      cursor: "pointer",
                      border: "none",
                    }}
                  >
                    Browse
                  </button>
                </div>
              </div>
              <div>
                <div style={lbl}>Target Path</div>
                <div style={{ display: "flex", gap: 6 }}>
                  <input
                    value={targetPath}
                    onChange={(e) => setTargetPath(e.target.value)}
                    placeholder="/Users/you/projects/copied-folder"
                    style={{ ...fieldStyle, fontFamily: C.mono, flex: 1 }}
                  />
                  <button
                    onClick={() =>
                      handleBrowseFolder((selected) => {
                        const leaf = `${basename(sourcePath) || "folder"}-copy`;
                        setTargetPath(`${selected.replace(/\/+$/, "")}/${leaf}`);
                      }, "Select parent folder")
                    }
                    style={{
                      padding: "9px 14px",
                      borderRadius: 6,
                      background: C.surfaceHover,
                      color: C.text2,
                      fontSize: 11,
                      fontWeight: 600,
                      cursor: "pointer",
                      border: "none",
                    }}
                  >
                    Choose Parent
                  </button>
                </div>
              </div>
              <label
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 8,
                  fontSize: 12,
                  color: C.text2,
                }}
              >
                <input
                  type="checkbox"
                  checked={preserveGit}
                  onChange={(e) => setPreserveGit(e.target.checked)}
                />
                Preserve `.git` if the source folder is already a repository
              </label>
              <div>
                <div style={lbl}>Project Name</div>
                <input
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="Optional display name"
                  style={fieldStyle}
                />
              </div>
            </>
          )}

          {mode === "ssh" && (
            <>
              <div style={{ display: "grid", gridTemplateColumns: "1.2fr 1fr 120px", gap: 12 }}>
                <div>
                  <div style={lbl}>Host</div>
                  <input
                    value={sshHost}
                    onChange={(e) => setSshHost(e.target.value)}
                    placeholder="devbox.example.com"
                    autoFocus
                    style={fieldStyle}
                  />
                </div>
                <div>
                  <div style={lbl}>User</div>
                  <input
                    value={sshUser}
                    onChange={(e) => setSshUser(e.target.value)}
                    placeholder="ubuntu"
                    style={fieldStyle}
                  />
                </div>
                <div>
                  <div style={lbl}>Port</div>
                  <input
                    value={sshPort}
                    onChange={(e) => setSshPort(e.target.value)}
                    placeholder="22"
                    style={fieldStyle}
                  />
                </div>
              </div>
              <div>
                <div style={lbl}>Remote Path</div>
                <input
                  value={sshRemotePath}
                  onChange={(e) => {
                    const value = e.target.value;
                    setSshRemotePath(value);
                    if (!name.trim()) setName(basename(value) || sshHost.trim());
                  }}
                  placeholder="/srv/app"
                  style={{ ...fieldStyle, fontFamily: C.mono }}
                />
              </div>
              <div>
                <div style={lbl}>Project Name</div>
                <input
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="Optional display name"
                  style={fieldStyle}
                />
              </div>
            </>
          )}

          {error && <p style={{ fontSize: 11, color: "#EF4444", margin: 0 }}>{error}</p>}
        </div>

        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "flex-end",
            padding: "14px 20px",
            background: C.base,
            gap: 6,
          }}
        >
          <button
            onClick={onClose}
            style={{
              padding: "7px 16px",
              borderRadius: 6,
              background: "transparent",
              color: C.text3,
              fontSize: 11,
              fontWeight: 500,
              cursor: "pointer",
              border: "none",
            }}
          >
            Cancel
          </button>
          <button
            onClick={handleSubmit}
            disabled={!requestReady || submitting}
            style={{
              padding: "7px 16px",
              borderRadius: 6,
              background: C.accent,
              color: "#fff",
              fontSize: 11,
              fontWeight: 700,
              cursor: "pointer",
              opacity: !requestReady || submitting ? 0.5 : 1,
              border: "none",
            }}
          >
            {actionLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
