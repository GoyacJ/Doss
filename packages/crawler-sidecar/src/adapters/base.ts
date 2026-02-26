import fs from "node:fs/promises";
import path from "node:path";
import { chromium, type BrowserContext, type Page } from "playwright";
import type { CrawlMode } from "@doss/shared";

export interface AdapterSessionOptions {
  sessionDir: string;
  headless: boolean;
}

export interface PageFieldSelectors {
  cards: string[];
  name?: string[];
  title?: string[];
  company?: string[];
  city?: string[];
  salary?: string[];
  description?: string[];
  years?: string[];
  tag?: string[];
  phone?: string[];
  email?: string[];
  link?: string[];
}

export interface ResumeSelectors {
  containers: string[];
}

const blockedPatterns = [
  /验证码/u,
  /人机/u,
  /请完成验证/u,
  /captcha/i,
  /forbidden/i,
  /访问过于频繁/u,
];

const sessionInvalidPatterns = [
  /登录/u,
  /扫码/u,
  /sign\s?in/i,
  /log\s?in/i,
];

export async function withPersistentContext<T>(
  options: AdapterSessionOptions,
  callback: (context: BrowserContext, page: Page) => Promise<T>,
): Promise<T> {
  await fs.mkdir(options.sessionDir, { recursive: true });

  const context = await chromium.launchPersistentContext(path.resolve(options.sessionDir), {
    headless: options.headless,
    viewport: { width: 1440, height: 900 },
    ignoreHTTPSErrors: true,
  });

  try {
    const page = context.pages()[0] ?? (await context.newPage());
    return await callback(context, page);
  } finally {
    await context.close();
  }
}

export async function navigateAndStabilize(page: Page, url: string, mode: CrawlMode): Promise<void> {
  await page.goto(url, {
    waitUntil: "domcontentloaded",
    timeout: 30_000,
  });
  if (mode === "compliant") {
    await page.waitForTimeout(1_500);
  } else {
    await page.waitForTimeout(450);
  }
}

export async function assertPageAvailable(page: Page, source: string): Promise<void> {
  const bodyText = await page.evaluate(() => {
    const doc = (globalThis as { document?: unknown }).document as
      | { body?: { innerText?: string | null } }
      | undefined;
    return doc?.body?.innerText?.slice(0, 4000) ?? "";
  });

  if (!bodyText.trim()) {
    throw new Error(`${source}_empty_page_content`);
  }

  if (blockedPatterns.some((pattern) => pattern.test(bodyText))) {
    throw new Error(`${source}_captcha_or_blocked`);
  }

  if (sessionInvalidPatterns.some((pattern) => pattern.test(bodyText))) {
    throw new Error(`${source}_session_invalid_or_login_required`);
  }
}

export async function extractJobCards(page: Page, selectors: PageFieldSelectors): Promise<unknown[]> {
  return page.evaluate((schema) => {
    const firstMatchText = (
      root: { querySelector: (selector: string) => { textContent?: string | null } | null },
      targets: string[],
    ) => {
      for (const selector of targets) {
        const node = root.querySelector(selector);
        const text = node?.textContent?.trim();
        if (text) {
          return text;
        }
      }
      return "";
    };

    const firstMatchHref = (
      root: {
        querySelector: (selector: string) => { href?: string; getAttribute?: (name: string) => string | null } | null;
      },
      targets: string[],
    ) => {
      for (const selector of targets) {
        const node = root.querySelector(selector);
        const href = node?.href?.trim();
        if (!href) {
          const attrHref = node?.getAttribute?.("href")?.trim();
          if (attrHref) {
            return attrHref;
          }
        }
        if (href) {
          return href;
        }
      }
      return "";
    };

    const doc = (globalThis as { document?: unknown }).document as
      | { querySelectorAll: (selectors: string) => unknown[] }
      | undefined;
    if (!doc) {
      return [];
    }

    const cards = schema.cards
      .flatMap((selector) => Array.from(doc.querySelectorAll(selector)));
    const unique = Array.from(new Set(cards)).slice(0, 80);

    return unique.map((card) => {
      const node = card as {
        querySelector: (selector: string) => { textContent?: string | null; href?: string; getAttribute?: (name: string) => string | null } | null;
        getAttribute: (name: string) => string | null;
      };

      return {
        title: schema.title ? firstMatchText(node, schema.title) : "",
        company: schema.company ? firstMatchText(node, schema.company) : "",
        city: schema.city ? firstMatchText(node, schema.city) : "",
        salaryK: schema.salary ? firstMatchText(node, schema.salary) : "",
        description: schema.description ? firstMatchText(node, schema.description) : "",
        jobUrl: schema.link ? firstMatchHref(node, schema.link) : "",
        externalId:
          node.getAttribute("data-job-id") ??
          node.getAttribute("data-id") ??
          node.getAttribute("data-positionid") ??
          "",
      };
    });
  }, selectors);
}

export async function extractCandidateCards(page: Page, selectors: PageFieldSelectors): Promise<unknown[]> {
  return page.evaluate((schema) => {
    const firstMatchText = (
      root: { querySelector: (selector: string) => { textContent?: string | null } | null },
      targets: string[],
    ) => {
      for (const selector of targets) {
        const node = root.querySelector(selector);
        const text = node?.textContent?.trim();
        if (text) {
          return text;
        }
      }
      return "";
    };

    const firstMatchHref = (
      root: {
        querySelector: (selector: string) => { href?: string; getAttribute?: (name: string) => string | null } | null;
      },
      targets: string[],
    ) => {
      for (const selector of targets) {
        const node = root.querySelector(selector);
        const href = node?.href?.trim();
        if (!href) {
          const attrHref = node?.getAttribute?.("href")?.trim();
          if (attrHref) {
            return attrHref;
          }
        }
        if (href) {
          return href;
        }
      }
      return "";
    };

    const doc = (globalThis as { document?: unknown }).document as
      | { querySelectorAll: (selectors: string) => unknown[] }
      | undefined;
    if (!doc) {
      return [];
    }

    const cards = schema.cards
      .flatMap((selector) => Array.from(doc.querySelectorAll(selector)));
    const unique = Array.from(new Set(cards)).slice(0, 100);

    return unique.map((card) => {
      const node = card as {
        querySelector: (selector: string) => { textContent?: string | null; href?: string; getAttribute?: (name: string) => string | null } | null;
        getAttribute: (name: string) => string | null;
      };
      const link = schema.link ? firstMatchHref(node, schema.link) : "";

      return {
        name: schema.name ? firstMatchText(node, schema.name) : "",
        currentCompany: schema.company ? firstMatchText(node, schema.company) : "",
        years: schema.years ? firstMatchText(node, schema.years) : "",
        tag: schema.tag ? firstMatchText(node, schema.tag) : "",
        phone: schema.phone ? firstMatchText(node, schema.phone) : "",
        email: schema.email ? firstMatchText(node, schema.email) : "",
        profileUrl: link,
        externalId:
          node.getAttribute("data-geek") ??
          node.getAttribute("data-resume-number") ??
          node.getAttribute("data-resumeid") ??
          node.getAttribute("data-id") ??
          link ??
          "",
      };
    });
  }, selectors);
}

function extractSkills(rawText: string): string[] {
  const dictionary = [
    "Vue",
    "Vue3",
    "React",
    "TypeScript",
    "JavaScript",
    "Node.js",
    "Python",
    "Java",
    "Go",
    "Rust",
    "SQL",
    "MySQL",
    "PostgreSQL",
    "Redis",
    "Docker",
    "Kubernetes",
    "Playwright",
    "Prompt",
    "LLM",
  ];

  return dictionary.filter((item) => {
    const escaped = item.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    return new RegExp(`\\b${escaped}\\b`, "i").test(rawText);
  });
}

export async function extractResumePayload(
  page: Page,
  source: string,
  candidateId: string,
  selectors: ResumeSelectors,
): Promise<{ rawText: string; parsed: Record<string, unknown> }> {
  const rawText = await page.evaluate((schema) => {
    const doc = (globalThis as { document?: unknown }).document as
      | {
        querySelectorAll: (selectors: string) => unknown[];
        body?: { innerText?: string | null };
      }
      | undefined;
    if (!doc) {
      return "";
    }

    const chunks = schema.containers
      .flatMap((selector) => Array.from(doc.querySelectorAll(selector)))
      .map((node) => {
        const target = node as { textContent?: string | null };
        return target.textContent?.trim() ?? "";
      })
      .filter((text) => text.length > 20);

    if (chunks.length > 0) {
      return chunks.join("\n");
    }

    return doc.body?.innerText?.trim() ?? "";
  }, selectors);

  if (!rawText || rawText.length < 20) {
    throw new Error(`${source}_resume_content_empty`);
  }

  return {
    rawText,
    parsed: {
      candidateId,
      source,
      skills: extractSkills(rawText),
      crawledAt: new Date().toISOString(),
      summary: rawText.slice(0, 280),
    },
  };
}

export function resolveDetailUrl(input: string, buildFromId: (id: string) => string): string {
  const text = input.trim();
  if (text.startsWith("http://") || text.startsWith("https://")) {
    return text;
  }
  return buildFromId(text);
}
