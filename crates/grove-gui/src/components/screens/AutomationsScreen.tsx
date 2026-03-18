import { useState } from "react";
import { AutomationList } from "@/components/automations/AutomationList";
import { AutomationDetail } from "@/components/automations/AutomationDetail";
import type { ProjectRow } from "@/types";

interface Props {
  projectId: string | null;
  projects: ProjectRow[];
}

export function AutomationsScreen({ projectId, projects }: Props) {
  const [selectedAutomationId, setSelectedAutomationId] = useState<string | null>(null);

  if (selectedAutomationId) {
    return (
      <AutomationDetail
        automationId={selectedAutomationId}
        projectId={projectId}
        onBack={() => setSelectedAutomationId(null)}
      />
    );
  }

  return (
    <AutomationList
      projectId={projectId}
      projects={projects}
      onSelect={setSelectedAutomationId}
    />
  );
}
