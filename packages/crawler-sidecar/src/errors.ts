import type { CrawlMode, SourceType } from "@doss/shared";

export type CrawlTaskType = "jobs" | "candidates" | "resume";

export type CrawlErrorCode =
  | "TIMEOUT"
  | "CAPTCHA_REQUIRED"
  | "ACCESS_BLOCKED"
  | "SESSION_INVALID"
  | "NETWORK_ERROR"
  | "PARSING_ERROR"
  | "UNKNOWN_ERROR";

export interface CrawlErrorContext {
  source: SourceType;
  taskType: CrawlTaskType;
  mode: CrawlMode;
  payload: Record<string, unknown>;
  url?: string;
}

export interface ClassifiedCrawlError {
  errorCode: CrawlErrorCode;
  errorMessage: string;
  snapshot: Record<string, unknown>;
}

function resolveMessage(error: unknown): string {
  if (error instanceof Error && error.message.trim()) {
    return error.message.trim();
  }

  if (typeof error === "string" && error.trim()) {
    return error.trim();
  }

  return "Unknown crawler error";
}

function resolveCode(message: string): CrawlErrorCode {
  const normalized = message.toLowerCase();

  if (
    normalized.includes("timeout") ||
    normalized.includes("timed out") ||
    normalized.includes("navigation timeout")
  ) {
    return "TIMEOUT";
  }

  if (
    normalized.includes("captcha") ||
    normalized.includes("验证码") ||
    normalized.includes("人机") ||
    normalized.includes("verify")
  ) {
    return "CAPTCHA_REQUIRED";
  }

  if (
    normalized.includes("403") ||
    normalized.includes("forbidden") ||
    normalized.includes("429") ||
    normalized.includes("blocked") ||
    normalized.includes("封禁")
  ) {
    return "ACCESS_BLOCKED";
  }

  if (
    normalized.includes("session") ||
    normalized.includes("cookie") ||
    normalized.includes("login") ||
    normalized.includes("未登录")
  ) {
    return "SESSION_INVALID";
  }

  if (
    normalized.includes("net::") ||
    normalized.includes("network") ||
    normalized.includes("econnreset") ||
    normalized.includes("econnrefused") ||
    normalized.includes("enotfound")
  ) {
    return "NETWORK_ERROR";
  }

  if (
    normalized.includes("selector") ||
    normalized.includes("parse") ||
    normalized.includes("schema")
  ) {
    return "PARSING_ERROR";
  }

  return "UNKNOWN_ERROR";
}

export function classifyCrawlError(error: unknown, context: CrawlErrorContext): ClassifiedCrawlError {
  const message = resolveMessage(error);
  const errorCode = resolveCode(message);

  return {
    errorCode,
    errorMessage: message,
    snapshot: {
      source: context.source,
      taskType: context.taskType,
      mode: context.mode,
      payload: context.payload,
      url: context.url,
      message,
      timestamp: new Date().toISOString(),
    },
  };
}
