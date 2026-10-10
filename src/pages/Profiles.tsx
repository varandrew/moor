import { useTranslation } from "@/contexts/LocaleContext";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent } from "@/components/ui/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { PageHeader } from "@/components/shared/PageHeader";
import { useProfiles } from "@/hooks/useProfiles";
import { Plus, Trash2, Check, Code, FlaskConical, User, Home } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { cn } from "@/lib/utils";
import type { Profile } from "@moor/types";

interface DisplayProfile {
  profile: Profile;
  originalIndex: number;
}

function getProfilesForDisplay(profiles: Profile[]): DisplayProfile[] {
  return profiles
    .map((profile, originalIndex) => ({ profile, originalIndex }))
    .sort((a, b) => {
      if (a.profile.isActive === b.profile.isActive) return a.originalIndex - b.originalIndex;
      return a.profile.isActive ? -1 : 1;
    });
}

const profileIcons = [Code, FlaskConical, User, Home, Code, FlaskConical, User, Home];
const profileAccents = [
  "bg-cursor-orange/10 text-cursor-orange border-cursor-orange/20",
  "bg-grep/15 text-grep border-grep/20",
  "bg-read/15 text-read border-read/20",
  "bg-edit/15 text-edit border-edit/20",
];

export function Profiles() {
  const { t } = useTranslation();
  const { profiles, createProfile, activateProfile, deleteProfile, cloneProfile } = useProfiles();
  const [newName, setNewName] = useState("");
  const [templateId, setTemplateId] = useState("");
  const [creating, setCreating] = useState(false);
  const navigate = useNavigate();
  const displayProfiles = getProfilesForDisplay(profiles);

  const resetForm = () => {
    setNewName("");
    setTemplateId("");
    setCreating(false);
  };

  const handleCreate = async () => {
    const name = newName.trim();
    if (!name) return;
    try {
      if (templateId && templateId !== "none") {
        await cloneProfile({ sourceId: templateId, name });
      } else {
        await createProfile(name);
      }
    } catch {
      // 失败提示由全局 MutationCache onError 兜底；保留表单输入便于重试
      return;
    }
    resetForm();
  };

  return (
    <div className="space-y-8 animate-fade-in-up">
      <PageHeader
        title={t("Profiles")}
        subtitle={t("Manage server groupings and tool visibility")}
        action={
          !creating && (
            <Button onClick={() => setCreating(true)}>
              <Plus className="h-4 w-4 mr-2" />
              {t("New Profile")}
            </Button>
          )
        }
      />

      {/* Create Form */}
      {creating && (
        <Card className="animate-scale-in border-cursor-orange/20">
          <CardContent className="p-4 flex items-center gap-3 flex-wrap">
            <Input
              placeholder={t("Profile name")}
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && handleCreate()}
              autoFocus
              className="max-w-sm"
            />
            <Select value={templateId} onValueChange={setTemplateId}>
              <SelectTrigger className="w-[220px]">
                <SelectValue placeholder={t("Start from scratch")} />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="none">{t("Start from scratch")}</SelectItem>
                {profiles.map((profile) => (
                  <SelectItem key={profile.id} value={profile.id}>
                    {t("Copy of {name}", { name: profile.name })}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Button onClick={handleCreate} disabled={!newName.trim()}>
              {t("Create")}
            </Button>
            <Button variant="outline" onClick={resetForm}>
              {t("Cancel")}
            </Button>
          </CardContent>
        </Card>
      )}

      {/* Profiles Grid */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
        {displayProfiles.map(({ profile, originalIndex }) => {
          const Icon = profileIcons[originalIndex % profileIcons.length];
          const accent = profileAccents[originalIndex % profileAccents.length];
          return (
            <Card
              key={profile.id}
              className={cn(
                "group cursor-pointer transition-all duration-200 hover:shadow-[0_12px_40px_-12px_rgba(0,0,0,0.06)]",
                profile.isActive
                  ? "ring-2 ring-cursor-orange/20 border-cursor-orange/30"
                  : "hover:border-[var(--fg-15)]",
              )}
              onClick={() => navigate(`/profiles/${profile.id}`)}
            >
              <CardContent className="p-5 relative">
                {profile.isActive && (
                  <div className="absolute top-4 right-4 text-cursor-orange">
                    <Check className="h-5 w-5" />
                  </div>
                )}
                <div
                  className={cn(
                    "h-10 w-10 rounded-xl flex items-center justify-center border mb-4 transition-colors",
                    accent,
                  )}
                >
                  <Icon className="h-[18px] w-[18px]" />
                </div>
                <h3 className="font-headline text-base text-cursor-dark mb-1">{profile.name}</h3>
                <p className="font-body text-xs text-[var(--fg-45)] mb-5">
                  {t(profile.isActive ? "Currently active" : "Click to manage")}
                </p>
                <div className="flex items-center gap-2 pt-4 border-t border-[var(--fg-06)]">
                  <span
                    className={cn(
                      "h-2 w-2 rounded-full",
                      profile.isActive ? "bg-success-muted" : "bg-[var(--fg-20)]",
                    )}
                  />
                  <span className="font-body text-xs text-[var(--fg-40)]">
                    {t("{count} servers", { count: profile.serverCount ?? 0 })}
                  </span>
                </div>
                {/* Actions overlay */}
                <div
                  className="absolute top-4 right-4 flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity"
                  style={{ opacity: profile.isActive ? undefined : undefined }}
                  onClick={(e) => e.stopPropagation()}
                >
                  {!profile.isActive && (
                    <>
                      <Button
                        variant="ghost"
                        size="sm"
                        className="text-xs px-2"
                        onClick={(e) => {
                          e.stopPropagation();
                          activateProfile(profile.id);
                        }}
                      >
                        {t("Activate")}
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="text-[var(--fg-35)] hover:text-error-warm"
                        onClick={(e) => {
                          e.stopPropagation();
                          deleteProfile(profile.id);
                        }}
                      >
                        <Trash2 className="h-4 w-4" />
                      </Button>
                    </>
                  )}
                </div>
              </CardContent>
            </Card>
          );
        })}

        {/* New Profile CTA */}
        <button
          onClick={() => setCreating(true)}
          className={cn(
            "rounded-xl border-2 border-dashed border-[var(--fg-12)]",
            "flex flex-col items-center justify-center text-center min-h-[180px] p-5",
            "text-[var(--fg-35)] hover:text-cursor-orange hover:border-cursor-orange/30 hover:bg-cursor-orange/[0.02]",
            "transition-all duration-200 cursor-pointer group",
          )}
        >
          <div className="h-11 w-11 rounded-full bg-surface-300 group-hover:bg-cursor-orange/10 flex items-center justify-center mb-3 transition-colors">
            <Plus className="h-5 w-5" />
          </div>
          <h3 className="font-headline text-sm font-medium text-cursor-dark group-hover:text-cursor-orange transition-colors">
            {t("New Profile")}
          </h3>
          <p className="font-body text-xs text-[var(--fg-40)] mt-1">
            {t("Create a server grouping")}
          </p>
        </button>
      </div>
    </div>
  );
}
