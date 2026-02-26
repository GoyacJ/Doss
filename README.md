# Doss Recruiter

AI辅助招聘桌面工具（Tauri2 + Vue3 + 本地SQLite）。

## 功能概览

- 桌面端：Mac / Windows（Tauri2）
- 本地优先：候选人与流程数据写入本地 SQLite
- 数据采集：内置采集任务管理，支持 Boss/智联/58 适配器骨架（Node sidecar）
- AI分析：对候选人生成结构化评分卡（匹配度/风险/亮点/建议 + 证据）
- 招聘管理：职位池、候选人池、阶段流转、流程事件追踪
- 全文检索：基于 SQLite FTS 检索候选人
- 安全审计：手机号/邮箱加密存储 + 审计日志

## 项目结构

- `apps/desktop`：Tauri2 + Vue3 桌面端
- `apps/desktop/src-tauri`：Rust 命令层、SQLite 模型、审计与加密
- `packages/shared`：跨端类型与评分规则
- `packages/crawler-sidecar`：Node sidecar（Express + 任务队列 + 三平台适配器骨架）

## 开发

```bash
pnpm install
pnpm -r test
pnpm -r typecheck
pnpm -r build
```

启动 sidecar（可选，用于演示采集接口）：

```bash
pnpm --filter @doss/crawler-sidecar dev
```

启动桌面应用：

```bash
pnpm --filter @doss/desktop tauri dev
```

仅启动 Web 前端：

```bash
pnpm --filter @doss/desktop dev
```

## 说明

- sidecar 目前提供可运行的适配器骨架与任务队列逻辑，默认返回示例数据，便于后续对接真实页面抓取流程。
- AI分析默认使用本地启发式 + `cloud-mock` 元信息，已预留多模型抽象层的数据结构。
