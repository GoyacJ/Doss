# Frontend Tokens & UnoCSS 统一规范

更新时间: 2026-02-26

## 目标

- 全局视觉 token 统一（颜色、字号、圆角、间距）
- 使用 UnoCSS 作为唯一样式生成引擎
- 组件模板统一为纯 Uno 原子类写法（无语义样式类）

## 核心文件

- `apps/desktop/uno.config.ts`
- `apps/desktop/src/main.ts`
- `apps/desktop/src/components/UiButton.vue`
- `apps/desktop/src/components/UiPanel.vue`
- `apps/desktop/src/components/UiField.vue`
- `apps/desktop/src/components/UiCheckbox.vue`
- `apps/desktop/src/components/UiTable.vue`
- `apps/desktop/src/components/UiTh.vue`
- `apps/desktop/src/components/UiTd.vue`
- `apps/desktop/src/components/UiMetricCard.vue`
- `apps/desktop/src/components/UiInfoRow.vue`
- `apps/desktop/src/components/UiBadge.vue`
- `apps/desktop/src/components/GlobalToastViewport.vue`
- `apps/desktop/src/lib/status.ts`
- `apps/desktop/src/stores/toast.ts`

## 全局 Token（示例）

- `bg`: `#f2efe8`
- `bg2`: `#ebe5d8`
- `text`: `#17202a`
- `muted`: `#556170`
- `brand`: `#0a5f54`
- `accent`: `#ec8a2f`
- `danger`: `#b43f2a`
- `line`: `#17202a1f`
- `card`: `#ffffffd1`

补充尺寸 token:

- `--font-size-body`: `15px`
- `--font-size-control`: `16px`
- `--radius-panel`: `16px`
- `--radius-control`: `10px`
- `--space-control-x`: `12px`
- `--space-control-y`: `10px`

## 结构约束

- 不再定义组件级语义 `shortcuts`
- 页面结构、容器、按钮、表格全部在模板内用原子类直接声明
- 允许在 `preflight` 中保留全局基础样式（body / input / select / textarea）
- 页面中重复交互元素优先使用组件封装（例如 `UiButton`、`UiPanel`）
- 表单控件统一用 `UiField` / `UiCheckbox` 封装标签、间距和可读性
- 表格统一用 `UiTable` + `UiTh` + `UiTd` 管理边框/对齐/可滚动
- 交互消息统一使用全局浮层 `Toast`（`GlobalToastViewport` + `toast store`）
- 统计数字卡统一使用 `UiMetricCard`
- KV信息行统一使用 `UiInfoRow`
- 业务状态统一使用 `UiBadge` + `status.ts` 映射（候选人阶段/任务状态/Sidecar状态）

## 输入控件统一规范

通过 preflight 全局统一:

- `input/select/textarea` 字号 `16px`
- 一致边框/圆角/背景
- 统一 `focus` 高亮和阴影
