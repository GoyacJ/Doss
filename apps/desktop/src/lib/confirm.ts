export interface SafeConfirmOptions {
  confirmFn?: ((message?: string) => boolean) | undefined;
  fallbackWhenUnavailable?: boolean;
}

export function safeConfirm(
  message: string,
  options?: SafeConfirmOptions,
): boolean {
  const fallbackWhenUnavailable = options?.fallbackWhenUnavailable ?? true;
  const confirmFn = options?.confirmFn ?? (
    typeof window !== "undefined" && typeof window.confirm === "function"
      ? window.confirm.bind(window)
      : undefined
  );

  if (!confirmFn) {
    return fallbackWhenUnavailable;
  }

  try {
    return confirmFn(message);
  } catch {
    return fallbackWhenUnavailable;
  }
}

