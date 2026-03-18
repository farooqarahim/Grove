import { C } from "@/lib/theme";
import { Home, Layers, Gear, KanbanIcon, Zap } from "@/components/ui/icons";
import type { NavScreen } from "@/types";

interface NavRailProps {
  screen: NavScreen;
  onNavigate: (screen: NavScreen) => void;
  openIssueCount?: number;
  automationCount?: number;
}

const NAV_ITEMS: { screen: NavScreen; icon: typeof Home; label: string }[] = [
  { screen: "dashboard", icon: Home, label: "Home" },
  { screen: "sessions", icon: Layers, label: "Sessions" },
  { screen: "issues", icon: KanbanIcon, label: "Issues" },
  { screen: "automations", icon: Zap, label: "Automations" },
];

export function NavRail({ screen, onNavigate, openIssueCount, automationCount }: NavRailProps) {
  return (
    <div
      className="flex flex-col items-center shrink-0 pt-3"
      style={{ width: 52, background: C.sidebar }}
    >
      {NAV_ITEMS.map(({ screen: s, icon: Icon, label }) => {
        let badge: number | undefined;
        if (s === "issues" && openIssueCount) badge = openIssueCount;
        if (s === "automations" && automationCount) badge = automationCount;
        return (
          <NavButton
            key={s}
            active={screen === s}
            onClick={() => onNavigate(s)}
            label={label}
            badge={badge}
          >
            <Icon size={18} />
          </NavButton>
        );
      })}

      <div className="flex-1" />

      <NavButton
        active={screen === "settings"}
        onClick={() => onNavigate("settings")}
        label="Settings"
      >
        <Gear size={18} />
      </NavButton>

      <div className="h-3" />
    </div>
  );
}

function NavButton({
  active,
  onClick,
  children,
  label,
  badge,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
  label: string;
  badge?: number;
}) {
  return (
    <button
      onClick={onClick}
      title={label}
      aria-label={label}
      aria-current={active ? "page" : undefined}
      className={`nav-btn ${active ? "nav-active" : ""} relative flex items-center justify-center cursor-pointer`}
      style={{
        width: 38, height: 38, margin: "2px 0",
        borderRadius: 6,
        border: "none",
        background: active ? C.accentDim : "transparent",
        color: active ? C.accent : C.text4,
      }}
    >
      {active && (
        <div
          className="absolute rounded-sm"
          style={{
            left: -6, top: 10, bottom: 10,
            width: 3, background: C.accent,
          }}
        />
      )}
      {children}
      {badge != null && badge > 0 && (
        <span style={{
          position: "absolute", top: 4, right: 4,
          minWidth: 14, height: 14, borderRadius: 7,
          background: C.accent, color: "#000",
          fontSize: 8, fontWeight: 800,
          display: "flex", alignItems: "center", justifyContent: "center",
          padding: "0 3px",
        }}>
          {badge > 99 ? "99+" : badge}
        </span>
      )}
    </button>
  );
}
