# Doss Recruiter

AI辅助招聘桌面工具（Tauri2 + Vue3 + 本地SQLite）。

## 功能概览

- 桌面端：Mac / Windows（Tauri2）
- 本地优先：候选人与流程数据写入本地 SQLite
- 数据采集：内置采集任务管理，支持 Boss/智联/58 真实适配器（Node sidecar + Playwright）
- AI分析：对候选人生成结构化评分卡（匹配度/风险/亮点/建议 + 证据）
  - 云模型支持：千问（Qwen）、豆包（Doubao）、MiniMax、GLM（含失败降级到本地启发式）
- 简历文件：支持 PDF / DOCX / TXT 上传解析，可选 OCR（依赖本机 `tesseract`），并可导入后自动触发分析
- 招聘管理：职位池、候选人池、阶段流转、流程事件追踪
- 全文检索：基于 SQLite FTS 检索候选人
- 安全审计：手机号/邮箱加密存储 + 审计日志

## 项目结构

- `apps/desktop`：Tauri2 + Vue3 桌面端
- `apps/desktop/src-tauri`：Rust 命令层、SQLite 模型、审计与加密
- `packages/shared`：跨端类型与评分规则
- `packages/crawler-sidecar`：Node sidecar（Express + 任务队列 + 三平台真实适配器）

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

- sidecar 采用 Playwright 持久化会话目录执行真实抓取，不再返回示例 Mock 数据；首次使用请先在持久化 profile 中完成各平台登录。
- 桌面端会在启动时自动确保 sidecar 可用；若端口冲突会尝试后续端口（默认从 `3791` 向后探测），异常退出后会在下次请求时自动重连拉起。
- 可通过 `DOSS_LOCAL_KEY` 显式指定本地敏感字段加密密钥；若未设置，应用会在本机自动生成并持久化本地密钥。
- AI分析默认可在设置页切换供应商；未配置密钥或调用失败时自动降级为本地启发式分析。
- 可通过环境变量覆盖运行时配置：`DOSS_AI_PROVIDER`、`DOSS_AI_MODEL`、`DOSS_AI_BASE_URL`、`DOSS_AI_API_KEY`、`DOSS_AI_TEMPERATURE`、`DOSS_AI_MAX_TOKENS`、`DOSS_AI_TIMEOUT_SECS`、`DOSS_AI_RETRY_COUNT`。也支持 `DOSS_QWEN_API_KEY` / `DOSS_DOUBAO_API_KEY` / `DOSS_DEEPSEEK_API_KEY` / `DOSS_MINIMAX_API_KEY` / `DOSS_GLM_API_KEY` / `DOSS_OPENAPI_API_KEY`。
- sidecar 运行参数可覆盖：`DOSS_SIDECAR_CMD`、`DOSS_SIDECAR_CWD`、`DOSS_SIDECAR_AUTOSTART`、`CRAWLER_PORT`。
