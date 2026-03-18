export function formatRunPipelineLabel(pipeline?: string | null): string {
  switch (pipeline) {
    case "build_validate_judge":
    case "build":
      return "Builder + Validator -> Judge";
    case "bugfix":
    case "fix_validate_judge":
      return "Fixer + Validator -> Judge";
    case "review_only":
      return "Validator + Judge";
    case "autonomous":
      return "Legacy full pipeline";
    case "plan":
      return "Legacy planning pipeline";
    default:
      return humanizeToken(pipeline ?? "Run");
  }
}

export function formatRunBundleLabel(pipeline?: string | null): string {
  switch (pipeline) {
    case "bugfix":
    case "fix_validate_judge":
      return "Fixer + Validator";
    case "review_only":
      return "Validator";
    default:
      return "Builder + Validator";
  }
}

export function formatRunAgentLabel(agentType?: string | null, pipeline?: string | null): string {
  switch (agentType) {
    case "builder":
      return pipeline === "bugfix" || pipeline === "fix_validate_judge"
        ? "Fixer + Validator"
        : "Builder + Validator";
    case "reviewer":
      return "Validator";
    case "judge":
      return "Judge";
    case "build_prd":
      return "PRD";
    case "plan_system_design":
      return "Design";
    default:
      return humanizeToken(agentType ?? "Agent");
  }
}

function humanizeToken(value: string): string {
  return value
    .split(/[_\s-]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}
