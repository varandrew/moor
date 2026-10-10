import { useTranslation } from "@/contexts/LocaleContext";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";

interface UnsavedChangesDialogProps {
  open: boolean;
  title?: string;
  description?: string;
  actionLabel?: string;
  onCancel: () => void;
  onConfirm: () => void;
}

export function UnsavedChangesDialog({
  open,
  title = "Discard unsaved changes?",
  description = "You have unsaved changes. They will be lost if you continue.",
  actionLabel = "Discard changes",
  onCancel,
  onConfirm,
}: UnsavedChangesDialogProps) {
  const { t, text } = useTranslation();
  return (
    <AlertDialog open={open}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{text(title)}</AlertDialogTitle>
          <AlertDialogDescription>{text(description)}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel onClick={onCancel}>{t("Cancel")}</AlertDialogCancel>
          <AlertDialogAction
            className="bg-error-warm text-surface-200 hover:bg-error-warm/90"
            onClick={onConfirm}
          >
            {text(actionLabel)}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
