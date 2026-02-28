import { describe, expect, it } from "vitest";
import { assertPageAvailable } from "../src/adapters/base";

function createPageStub(input: {
  bodyText: string;
  url?: string;
  title?: string;
}) {
  return {
    evaluate: async () => input.bodyText,
    url: () => input.url ?? "https://www.zhipin.com/web/geek/job?query=前端",
    title: async () => input.title ?? "职位搜索 - BOSS直聘",
  };
}

describe("assertPageAvailable", () => {
  it("throws captcha_or_blocked for boss verify-slider page", async () => {
    const page = createPageStub({
      url: "https://www.zhipin.com/web/user/safe/verify-slider?callbackUrl=https%3A%2F%2Fwww.zhipin.com%2Fweb%2Fgeek%2Fjob%3Fquery%3D%25E5%2589%258D%25E7%25AB%25AF",
      title: "网站访客身份验证 - BOSS直聘",
      bodyText: "点击按钮进行验证 当前 IP 地址可能存在异常访问行为，完成验证后即可正常使用。",
    });

    await expect(assertPageAvailable(page as never, "boss")).rejects.toThrow(
      "boss_captcha_or_blocked",
    );
  });

  it("throws session_invalid_or_login_required for login page content", async () => {
    const page = createPageStub({
      bodyText: "请先登录后继续访问，支持扫码登录",
      title: "登录 - BOSS直聘",
    });

    await expect(assertPageAvailable(page as never, "boss")).rejects.toThrow(
      "boss_session_invalid_or_login_required",
    );
  });
});
