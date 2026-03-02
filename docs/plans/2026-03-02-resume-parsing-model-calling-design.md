# 简历解析与模型调用详细设计（候选人重新分析）

## 1. 背景与目标

当前“候选人详情 -> 重新分析”已可执行完整 AI 评分链路，但在解析质量治理、调用层可观测、错误分级与前端可解释性上仍有提升空间。

本设计目标是：

1. 将“简历解析 + 模型调用”拆分为清晰可演进的三层流水线（解析层、调用层、评分层）。
2. 明确输入契约、错误码契约、进度事件契约，避免跨层语义漂移。
3. 提升失败可诊断性与用户可操作性（失败原因明确、下一步建议明确）。
4. 在不破坏现有 API 与 UI 的前提下做增量演进。

非目标：

1. 不引入多模型投票/仲裁系统。
2. 不改动招聘业务评分维度（T0/T1/T2/T3）的业务语义。
3. 不调整历史评分数据结构的主索引与读取方式。

---

## 2. 当前实现概览（代码锚点）

前端入口与进度：

- `apps/desktop/src/views/CandidatesView.vue`
  - `rerunScoring()` 触发重新评分
  - 监听 `candidate-ai-analysis-progress` 事件并更新步骤条

store 与后端桥接：

- `apps/desktop/src/stores/recruiting/analysis-context.ts`
  - `runScoring(candidateId, jobId, runId)`
- `apps/desktop/src/services/backend.ts`
  - `runCandidateAiAnalysis()` -> invoke `run_candidate_ai_analysis`

后端核心：

- `apps/desktop/src-tauri/src/domains/scoring.rs`
  - `run_candidate_ai_analysis_blocking()`：主流程
  - `invoke_text_generation_json_strict()`：严格 JSON + repair + schema 校验
  - `list_scoring_results()`：按 `created_at DESC, id DESC` 返回

简历物化：

- `apps/desktop/src-tauri/src/domains/resume_materializer.rs`
  - `materialize_resume_from_file_full_text()`：从附件提取全文并回写 `resumes`

---

## 3. 目标架构

### 3.1 分层

1. 解析层（Resume Parsing Pipeline）
- 职责：从附件稳定提取可评分文本与结构化摘要，给出解析质量信号。

2. 调用层（Model Invocation Orchestrator）
- 职责：根据模型能力选择输入模式，执行严格结构化调用、重试与修复。

3. 评分层（Scoring Composer）
- 职责：将模型输出与模板对齐、计算最终分、生成 recommendation/risk、落库。

### 3.2 端到端时序

1. UI 触发 rerun，生成 `run_id`。
2. 解析层执行附件全文提取、结构化解析、质量评估。
3. 调用层构造 prompt，调用模型并做 JSON/schema 兜底。
4. 评分层按模板合成分数与结构化结果。
5. 持久化 `scoring_results`，发送完成事件。
6. 前端刷新 scoring + context，展示最新结果。

---

## 4. 详细设计

### 4.1 解析层设计

#### 4.1.1 输入

- `candidate_id`（必填）
- 从 `resume_files` 读取附件内容（必需）
- OCR 开关/扩展名识别沿用现有解析器能力

#### 4.1.2 输出结构（新增逻辑字段）

在现有 `MaterializedResume` 基础上扩展逻辑对象（先可只落在内存与 structured_result runtime）：

```json
{
  "raw_text": "...",
  "parsed_value": {"...": "..."},
  "attachment": {"file_name": "resume.pdf"},
  "parse_quality": 0,
  "warnings": ["..."],
  "source_extension": "pdf",
  "ocr_used": true
}
```

#### 4.1.3 质量评分（parse_quality）

建议采用 0-100 评分，初始规则：

1. 文本长度（40 分）
- `< 300` 字：0-10
- `300-1500`：10-30
- `>= 1500`：30-40

2. 结构完整性（30 分）
- 命中工作经历/项目/技能等 section 的数量映射得分

3. 关键信息可用性（30 分）
- skills、项目描述、时间线等字段可提取情况

门禁策略：

- `parse_quality < 35`：阻断模型调用，返回可操作错误码 `resume_parse_quality_too_low`
- `35 <= parse_quality < 55`：允许调用，但注入 warning 并在 UI 显示“结果置信度一般”
- `>= 55`：正常调用

#### 4.1.4 与现有代码衔接

- 继续保留并复用：
  - `resume_file_required_for_ai_analysis`
  - `resume_file_text_empty_after_parse`
- 在 `materialize_resume_from_file_full_text()` 上增量扩展，不另起分支函数。

---

### 4.2 调用层设计

#### 4.2.1 输入模式选择

沿用当前逻辑并标准化记录：

- `direct_file`：模型支持文件上传且附件可用
- `parsed_text`：不支持文件上传时回退为解析全文

建议将最终选择写入结果 runtime：

```json
{"input_mode": "direct_file"}
```

#### 4.2.2 prompt 规范

继续使用双 prompt：

1. `system_prompt`：强约束 JSON schema、证据引用、字段必须存在
2. `user_prompt`：模板配置 + 候选人上下文 + 岗位上下文 +（必要时）全文文本

关键约束保持：

- section item key 必须来自模板
- 每个 item 必须有 reason + evidence
- 必须返回完整结构，不允许自然语言散文

#### 4.2.3 严格输出协议

执行顺序：

1. 主调用获取原始文本
2. `parse_json_from_text` 尝试解析
3. 失败时触发一次 repair 调用
4. repair 后仍非 JSON -> `provider_response_not_json_after_repair`
5. JSON 合法但 schema 不匹配 -> `provider_response_schema_invalid`

#### 4.2.4 重试策略

- schema/格式类错误：最多 1 次 repair（已有）
- 网络/供应商瞬时错误：建议新增 1 次指数退避重试（例如 300ms）
- 上下文超限：不重试同请求，直接给出“切换长上下文模型”建议

---

### 4.3 评分层设计

#### 4.3.1 模板对齐

- 每个 section（t0~t3）必须与模板 item 一一对应
- 多 key、少 key、未知 key 一律 schema invalid

#### 4.3.2 分数计算

保持现有：

- 每 section 5 分制加权
- overall_100 = overall_5 * 20，clamp 到 0-100

#### 4.3.3 决策建议

保持现有 recommendation 规则（PASS/REVIEW/REJECT），后续可参数化。

#### 4.3.4 持久化扩展

在 `structured_result.summary.runtime` 增加：

```json
{
  "provider": "...",
  "model": "...",
  "input_mode": "direct_file|parsed_text",
  "parse_quality": 78,
  "latency_ms": 1234,
  "token_usage": {"prompt": 0, "completion": 0, "total": 0}
}
```

注：若供应商无 token usage 则字段可为空对象。

---

### 4.4 进度事件与错误契约

#### 4.4.1 进度相位

保持并统一：

- `prepare -> ai -> t0 -> t1 -> t2 -> t3 -> persist`

状态：

- `running | completed | failed`

#### 4.4.2 错误码目录

保留：

- `resume_file_required_for_ai_analysis`
- `resume_file_text_empty_after_parse`
- `provider_response_not_json_after_repair`
- `provider_response_schema_invalid`
- `ai_provider_api_key_missing`

新增建议：

- `resume_parse_quality_too_low`
- `model_invocation_timeout`
- `provider_unavailable`

前端 `scoring-rerun-feedback.ts` 统一映射用户文案，避免裸错误直出。

---

## 5. 数据与迁移策略

### 5.1 数据库

短期建议：不改表结构，把新增 runtime 信息写入 `structured_result_json`。

中期可选：若要做 SQL 级监控，再追加列：

- `input_mode TEXT`
- `parse_quality INTEGER`
- `latency_ms INTEGER`
- `provider TEXT`
- `model TEXT`

### 5.2 兼容性

- `resolveStructuredScoringViewModel()` 已具备字段缺省容错，新增 runtime 字段不影响旧记录读取。
- 历史数据无需回填，前端空字段按 `-` 展示。

---

## 6. 可观测性与审计

1. 审计事件
- 保留 `scoring.run`
- 增加 payload：`inputMode/parseQuality/latencyMs/errorCode`

2. 指标（后续可落本地聚合）
- 解析成功率
- schema 合法率
- repair 触发率
- 平均耗时（解析/调用/总耗时）
- 各错误码分布

---

## 7. 测试策略

### 7.1 Rust 单测

- `resume_materializer.rs`
  - 无附件 -> `resume_file_required_for_ai_analysis`
  - 附件可读但空文本 -> `resume_file_text_empty_after_parse`
  - parse_quality 低于阈值 -> `resume_parse_quality_too_low`

- `scoring.rs`
  - 非 JSON -> repair 成功
  - repair 后仍失败 -> `provider_response_not_json_after_repair`
  - schema 缺字段/错 key -> `provider_response_schema_invalid`
  - 模板 item 对齐严格性验证

### 7.2 前端单测

- `analysis-progress`：事件过滤与 step 演进
- `scoring-rerun-feedback`：新增错误码映射
- `scoring-structured`：runtime 字段存在/缺省均可展示

### 7.3 集成验证

- `direct_file` 与 `parsed_text` 两条路径都能完整落库并展示
- UI 进度与后端 phase 一致

---

## 8. 风险与缓解

1. OCR 质量波动导致不稳定
- 引入 parse_quality 门禁 + warning

2. 供应商输出不稳定
- 严格 schema + repair + 可观测错误码

3. 上下文长度超限
- 明确错误提示 + 引导模型切换

4. 现网兼容风险
- 先走“仅 structured_result 扩展”的零迁移方案

---

## 9. 分阶段落地建议

1. Phase A（低风险）
- 增加 parse_quality 计算与错误码
- 扩展 runtime 信息写入 structured_result
- 前端展示与错误提示补齐

2. Phase B（稳态优化）
- 增加供应商瞬时失败退避重试
- 增加耗时/token 采集（按供应商能力）

3. Phase C（数据化运营）
- 如有需要，将 runtime 拆为表列用于 SQL 统计

---

## 10. 交付标准（Definition of Done）

1. 重新分析失败不再出现“不可解释错误”文案。
2. 新增错误码全链路可观测（后端 -> 前端文案）。
3. 两种输入模式都可稳定产出结构化评分。
4. 关键单测与集成测试覆盖新增分支。
5. 历史数据与旧 UI 行为兼容。
