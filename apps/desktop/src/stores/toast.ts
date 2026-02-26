import { defineStore } from "pinia";
import { ref } from "vue";

export type ToastTone = "info" | "success" | "warning" | "danger";

export interface ToastItem {
  id: number;
  tone: ToastTone;
  message: string;
  durationMs: number;
}

export interface ToastPayload {
  tone?: ToastTone;
  message: string;
  durationMs?: number;
}

const DEFAULT_DURATION_MS = 3200;

export const useToastStore = defineStore("toast", () => {
  const toasts = ref<ToastItem[]>([]);
  const nextId = ref(1);

  function dismiss(id: number) {
    toasts.value = toasts.value.filter((item) => item.id !== id);
  }

  function push(payload: ToastPayload) {
    const id = nextId.value++;
    const durationMs = Math.max(800, payload.durationMs ?? DEFAULT_DURATION_MS);

    toasts.value.push({
      id,
      tone: payload.tone ?? "info",
      message: payload.message,
      durationMs,
    });

    window.setTimeout(() => {
      dismiss(id);
    }, durationMs);

    return id;
  }

  function info(message: string, durationMs?: number) {
    return push({ tone: "info", message, durationMs });
  }

  function success(message: string, durationMs?: number) {
    return push({ tone: "success", message, durationMs });
  }

  function warning(message: string, durationMs?: number) {
    return push({ tone: "warning", message, durationMs });
  }

  function danger(message: string, durationMs?: number) {
    return push({ tone: "danger", message, durationMs });
  }

  return {
    toasts,
    dismiss,
    push,
    info,
    success,
    warning,
    danger,
  };
});
