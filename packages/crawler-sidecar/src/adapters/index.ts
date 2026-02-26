import type { SourceType } from "@doss/shared";
import { MockAdapter } from "./base";
import type { SourceAdapter } from "./types";

const bossJobs = [
  { title: "前端开发工程师", company: "示例科技", salaryK: "30-45", city: "上海" },
  { title: "高级前端工程师", company: "创新软件", salaryK: "40-60", city: "深圳" },
];

const zhilianJobs = [
  { title: "全栈工程师", company: "成长互联", salaryK: "25-40", city: "北京" },
  { title: "Vue技术负责人", company: "云端产品", salaryK: "45-65", city: "杭州" },
];

const wubaJobs = [
  { title: "招聘专员", company: "城市服务集团", salaryK: "12-18", city: "广州" },
  { title: "HRBP", company: "商业服务中心", salaryK: "20-35", city: "成都" },
];

const adapterMap: Record<SourceType, SourceAdapter> = {
  boss: new MockAdapter("boss", bossJobs),
  zhilian: new MockAdapter("zhilian", zhilianJobs),
  wuba: new MockAdapter("wuba", wubaJobs),
  manual: new MockAdapter("boss", bossJobs),
};

export function getAdapter(source: SourceType): SourceAdapter {
  return adapterMap[source];
}
