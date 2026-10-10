import { useTranslation } from "@/contexts/LocaleContext";
import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-shell";
import { FolderOpen } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { getServerLogPath } from "@/lib/tauri";
import { getNoticeErrorMessage } from "@/lib/utils";

interface OpenLogButtonProps {
  serverId: string;
}

export function OpenLogButton({ serverId }: OpenLogButtonProps) {
  const { t } = useTranslation();
  const [logPath, setLogPath] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    getServerLogPath(serverId)
      .then((path) => {
        if (!cancelled) setLogPath(path);
      })
      .catch(() => {
        // 非 Tauri 运行时(纯浏览器 dev)时无法取得路径,保持 null 由点击兜底。
      });
    return () => {
      cancelled = true;
    };
  }, [serverId]);

  const handleOpen = async () => {
    if (!logPath) {
      toast.error(t("Logs unavailable"), {
        description: t("Log files are only accessible in the Moor desktop app."),
      });
      return;
    }
    try {
      await open(logPath);
    } catch (err) {
      toast.error(t("Failed to open log file"), { description: getNoticeErrorMessage(err) });
    }
  };

  return (
    <Button
      variant="ghost"
      size="sm"
      onClick={handleOpen}
      title={logPath ?? t("Open log file")}
      className="h-7 px-2 text-error-warm hover:bg-error-warm/10 hover:text-error-warm"
    >
      <FolderOpen className="h-3.5 w-3.5 mr-1" />
      {t("Open Logs")}
    </Button>
  );
}
