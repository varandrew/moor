import { useTranslation } from "@/contexts/LocaleContext";
import { NavLink } from "react-router-dom";
import { cn } from "@/lib/utils";
import { MoorLogo } from "@/components/icons/MoorLogo";
import {
  LayoutDashboard,
  Server,
  FolderOpen,
  BarChart3,
  Braces,
  HelpCircle,
  Cog,
} from "lucide-react";

const navItems = [
  { to: "/", label: "Dashboard", icon: LayoutDashboard },
  { to: "/servers", label: "Servers", icon: Server },
  { to: "/profiles", label: "Profiles", icon: FolderOpen },
  { to: "/logs", label: "Insights", icon: BarChart3 },
  { to: "/config", label: "Client Config", icon: Braces },
];

export function Sidebar() {
  const { t, text } = useTranslation();
  return (
    <aside className="w-[220px] shrink-0 border-r border-[var(--fg-10)] bg-surface-300 flex flex-col">
      <div className="px-5 py-4 flex items-center gap-2.5">
        <div className="h-9 w-9 rounded-xl bg-cursor-dark flex items-center justify-center">
          <MoorLogo className="h-7 w-7 text-surface-200" />
        </div>
        <div className="-space-y-0.5">
          <span className="font-headline text-lg font-semibold tracking-tight text-cursor-dark leading-tight block">
            Moor
          </span>
          <span className="font-mono text-[10px] text-[var(--fg-40)] tracking-wider uppercase leading-none">
            {t("MCP Manager")}
          </span>
        </div>
      </div>

      <nav className="flex-1 px-3 py-2 space-y-0.5">
        {navItems.map(({ to, label, icon: Icon }) => (
          <NavLink
            key={to}
            to={to}
            className={({ isActive }) =>
              cn(
                "flex items-center gap-3 px-3 py-2.5 rounded-xl font-headline text-sm transition-all duration-200 relative group",
                isActive
                  ? "bg-surface-400 text-cursor-dark font-medium"
                  : "text-[var(--fg-55)] hover:bg-[var(--fg-06)] hover:text-cursor-dark",
              )
            }
          >
            {({ isActive }) => (
              <>
                <span
                  className={cn(
                    "absolute left-0 top-1/2 -translate-y-1/2 w-[3px] h-5 rounded-r-full bg-cursor-orange transition-all duration-200",
                    isActive ? "opacity-100" : "opacity-0",
                  )}
                />
                <Icon
                  className={cn("h-4 w-4 transition-colors", isActive && "text-cursor-orange")}
                />
                {text(label)}
              </>
            )}
          </NavLink>
        ))}
      </nav>

      <div className="p-3 mt-auto">
        <a
          href="https://github.com/modelcontextprotocol"
          target="_blank"
          rel="noopener noreferrer"
          className="flex items-center gap-3 px-3 py-2.5 rounded-xl font-headline text-sm text-[var(--fg-45)] hover:bg-[var(--fg-06)] hover:text-cursor-dark transition-all duration-200"
        >
          <HelpCircle className="h-4 w-4" />
          {t("Documentation")}
        </a>

        <div className="my-2 border-t border-[var(--fg-08)]" />

        <NavLink
          to="/settings"
          className={({ isActive }) =>
            cn(
              "flex items-center gap-3 px-3 py-2.5 rounded-xl font-headline text-sm transition-all duration-200 relative group",
              isActive
                ? "bg-surface-400 text-cursor-dark font-medium"
                : "text-[var(--fg-45)] hover:bg-[var(--fg-06)] hover:text-cursor-dark",
            )
          }
        >
          {({ isActive }) => (
            <>
              <span
                className={cn(
                  "absolute left-0 top-1/2 -translate-y-1/2 w-[3px] h-5 rounded-r-full bg-cursor-orange transition-all duration-200",
                  isActive ? "opacity-100" : "opacity-0",
                )}
              />
              <Cog className={cn("h-4 w-4 transition-colors", isActive && "text-cursor-orange")} />
              {t("Settings")}
            </>
          )}
        </NavLink>
      </div>

      <div className="px-5 py-3 border-t border-[var(--fg-08)]">
        <p className="font-mono text-[10px] text-[var(--fg-30)]">{`Moor v${__APP_VERSION__}`}</p>
      </div>
    </aside>
  );
}
