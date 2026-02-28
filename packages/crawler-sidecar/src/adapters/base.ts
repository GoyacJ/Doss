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
  /身份验证/u,
  /安全验证/u,
  /异常访问行为/u,
  /点击按钮进行验证/u,
  /captcha/i,
  /forbidden/i,
  /访问过于频繁/u,
];

const blockedUrlPatterns = [
  /\/web\/user\/safe\/verify/i,
  /verify-slider/i,
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
  const currentUrl = page.url();
  let pageTitle = "";
  try {
    pageTitle = await page.title();
  } catch {
    pageTitle = "";
  }

  const bodyText = await page.evaluate(() => {
    const doc = (globalThis as { document?: unknown }).document as
      | { body?: { innerText?: string | null } }
      | undefined;
    return doc?.body?.innerText?.slice(0, 4000) ?? "";
  });
  const visiblePageText = `${pageTitle}\n${bodyText}`;

  if (!bodyText.trim()) {
    throw new Error(`${source}_empty_page_content`);
  }

  if (
    blockedPatterns.some((pattern) => pattern.test(visiblePageText))
    || blockedUrlPatterns.some((pattern) => pattern.test(currentUrl))
  ) {
    throw new Error(`${source}_captcha_or_blocked`);
  }

  if (sessionInvalidPatterns.some((pattern) => pattern.test(visiblePageText))) {
    throw new Error(`${source}_session_invalid_or_login_required`);
  }
}

export async function extractJobCards(page: Page, selectors: PageFieldSelectors): Promise<unknown[]> {
  return page.evaluate((schema) => {
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
      let title = "";
      if (schema.title) {
        for (const selector of schema.title) {
          const text = node.querySelector(selector)?.textContent?.trim();
          if (text) {
            title = text;
            break;
          }
        }
      }

      let company = "";
      if (schema.company) {
        for (const selector of schema.company) {
          const text = node.querySelector(selector)?.textContent?.trim();
          if (text) {
            company = text;
            break;
          }
        }
      }

      let city = "";
      if (schema.city) {
        for (const selector of schema.city) {
          const text = node.querySelector(selector)?.textContent?.trim();
          if (text) {
            city = text;
            break;
          }
        }
      }

      let salaryK = "";
      if (schema.salary) {
        for (const selector of schema.salary) {
          const text = node.querySelector(selector)?.textContent?.trim();
          if (text) {
            salaryK = text;
            break;
          }
        }
      }

      let description = "";
      if (schema.description) {
        for (const selector of schema.description) {
          const text = node.querySelector(selector)?.textContent?.trim();
          if (text) {
            description = text;
            break;
          }
        }
      }

      let jobUrl = "";
      if (schema.link) {
        for (const selector of schema.link) {
          const linkNode = node.querySelector(selector);
          const href = linkNode?.href?.trim();
          if (href) {
            jobUrl = href;
            break;
          }
          const attrHref = linkNode?.getAttribute?.("href")?.trim();
          if (attrHref) {
            jobUrl = attrHref;
            break;
          }
        }
      }

      return {
        title,
        company,
        city,
        salaryK,
        description,
        jobUrl,
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
      let name = "";
      if (schema.name) {
        for (const selector of schema.name) {
          const text = node.querySelector(selector)?.textContent?.trim();
          if (text) {
            name = text;
            break;
          }
        }
      }

      let currentCompany = "";
      if (schema.company) {
        for (const selector of schema.company) {
          const text = node.querySelector(selector)?.textContent?.trim();
          if (text) {
            currentCompany = text;
            break;
          }
        }
      }

      let years = "";
      if (schema.years) {
        for (const selector of schema.years) {
          const text = node.querySelector(selector)?.textContent?.trim();
          if (text) {
            years = text;
            break;
          }
        }
      }

      let tag = "";
      if (schema.tag) {
        for (const selector of schema.tag) {
          const text = node.querySelector(selector)?.textContent?.trim();
          if (text) {
            tag = text;
            break;
          }
        }
      }

      let phone = "";
      if (schema.phone) {
        for (const selector of schema.phone) {
          const text = node.querySelector(selector)?.textContent?.trim();
          if (text) {
            phone = text;
            break;
          }
        }
      }

      let email = "";
      if (schema.email) {
        for (const selector of schema.email) {
          const text = node.querySelector(selector)?.textContent?.trim();
          if (text) {
            email = text;
            break;
          }
        }
      }

      let link = "";
      if (schema.link) {
        for (const selector of schema.link) {
          const linkNode = node.querySelector(selector);
          const href = linkNode?.href?.trim();
          if (href) {
            link = href;
            break;
          }
          const attrHref = linkNode?.getAttribute?.("href")?.trim();
          if (attrHref) {
            link = attrHref;
            break;
          }
        }
      }

      return {
        name,
        currentCompany,
        years,
        tag,
        phone,
        email,
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
