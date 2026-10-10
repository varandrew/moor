import { useTranslation } from "@/contexts/LocaleContext";
import { useProfiles } from "@/hooks/useProfiles";
import { ChevronDown, Check } from "lucide-react";
import { useState, useRef, useEffect } from "react";
import { cn } from "@/lib/utils";

export function Header() {
  const { t } = useTranslation();
  const { profiles, activateProfile } = useProfiles();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  const activeProfile = profiles.find((p) => p.isActive);

  useEffect(() => {
    function handleClick(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, []);

  return (
    <header className="h-14 border-b border-[var(--fg-08)] bg-surface-200/80 backdrop-blur-md flex items-center justify-between px-6 sticky top-0 z-40">
      <div className="flex items-center gap-2">
        <span className="font-body text-sm text-[var(--fg-40)]">{t("Active Profile")}</span>
        <div className="relative" ref={ref}>
          <button
            onClick={() => setOpen(!open)}
            className={cn(
              "flex items-center gap-2 px-3 py-1.5 rounded-xl transition-all duration-200 font-headline text-sm",
              open
                ? "bg-surface-400 text-cursor-dark shadow-sm"
                : "bg-surface-300 hover:bg-surface-400 text-cursor-dark",
            )}
          >
            <span className="relative flex h-2 w-2">
              <span className="animate-ping-slow absolute inline-flex h-full w-full rounded-full bg-success-muted opacity-60" />
              <span className="relative inline-flex rounded-full h-2 w-2 bg-success-muted" />
            </span>
            {activeProfile?.name || t("None")}
            <ChevronDown
              className={cn(
                "h-3.5 w-3.5 text-[var(--fg-35)] transition-transform duration-200",
                open && "rotate-180",
              )}
            />
          </button>
          {open && (
            <div className="absolute top-full mt-1.5 left-0 z-50 w-52 rounded-xl border border-[var(--fg-10)] bg-surface-200 shadow-[rgba(0,0,0,0.14)_0px_28px_70px,rgba(0,0,0,0.1)_0px_14px_32px,oklab(0.263084_-0.00230259_0.0124794_/_0.1)_0px_0px_0px_1px] py-1.5 animate-scale-in origin-top-left">
              {profiles.map((profile) => (
                <button
                  key={profile.id}
                  onClick={() => {
                    activateProfile(profile.id);
                    setOpen(false);
                  }}
                  className={cn(
                    "w-full text-left px-3 py-2 text-sm font-headline transition-colors duration-150 flex items-center gap-2.5 rounded-lg mx-1",
                    profile.isActive
                      ? "text-cursor-dark font-medium bg-surface-300"
                      : "text-[var(--fg-55)] hover:bg-surface-300/60 hover:text-cursor-dark",
                  )}
                >
                  {profile.isActive ? (
                    <Check className="h-3.5 w-3.5 text-success-muted" />
                  ) : (
                    <span className="h-3.5 w-3.5" />
                  )}
                  {profile.name}
                </button>
              ))}
            </div>
          )}
        </div>
      </div>
    </header>
  );
}
