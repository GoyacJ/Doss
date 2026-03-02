# Resume Parsing + Model Invocation Hardening Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Improve candidate rerun scoring reliability by adding parse-quality gating, stronger model-call observability, and end-user-visible runtime diagnostics.

**Architecture:** Keep the existing rerun pipeline (`CandidatesView -> run_candidate_ai_analysis`) and incrementally harden it in three layers: resume parsing quality, model invocation robustness, and UI/runtime surfacing. Persist new diagnostics into `structured_result.summary.runtime` first (no schema migration) to remain backward compatible.

**Tech Stack:** Vue 3 + Pinia + Vitest, Tauri (Rust), rusqlite, serde_json.

---

### Task 1: Add Parse-Quality Scoring in Resume Materialization

**Files:**
- Modify: `apps/desktop/src-tauri/src/domains/resume_materializer.rs`
- Test: `apps/desktop/src-tauri/src/domains/resume_materializer.rs` (existing `#[cfg(test)]` module)

**Step 1: Write the failing test**

Add tests asserting parse-quality behavior for short vs sufficiently rich resume text:

```rust
#[test]
fn parse_quality_should_be_low_for_tiny_text() {
    let score = compute_parse_quality("Vue");
    assert!(score < 35);
}

#[test]
fn parse_quality_should_be_acceptable_for_rich_resume() {
    let score = compute_parse_quality("工作经历 ... 项目经历 ... 技能 ...");
    assert!(score >= 55);
}
```

**Step 2: Run test to verify it fails**

Run: `cd apps/desktop/src-tauri && cargo test parse_quality_should_be_low_for_tiny_text -- --nocapture`
Expected: FAIL with missing function or assertion mismatch.

**Step 3: Write minimal implementation**

Implement `compute_parse_quality(raw_text: &str) -> i32` and add parse warnings generation helper.

```rust
fn compute_parse_quality(raw_text: &str) -> i32 {
    let len = raw_text.trim().chars().count();
    if len < 300 { return 20; }
    if len < 1500 { return 50; }
    75
}
```

**Step 4: Run tests to verify pass**

Run: `cd apps/desktop/src-tauri && cargo test parse_quality_should -- --nocapture`
Expected: PASS.

**Step 5: Commit**

```bash
git add apps/desktop/src-tauri/src/domains/resume_materializer.rs
git commit -m "test: add parse-quality scoring for materialized resume"
```

---

### Task 2: Enforce Parse-Quality Gate Before AI Invocation

**Files:**
- Modify: `apps/desktop/src-tauri/src/domains/scoring.rs`
- Modify: `apps/desktop/src-tauri/src/domains/resume_materializer.rs`
- Test: `apps/desktop/src-tauri/src/domains/scoring.rs` (unit tests)

**Step 1: Write the failing test**

Add unit test for low-quality materialized resume path returning `resume_parse_quality_too_low`.

```rust
#[test]
fn run_scoring_should_fail_when_parse_quality_too_low() {
    let err = run_scoring_with_fixture_low_quality().expect_err("should fail");
    assert_eq!(err, "resume_parse_quality_too_low");
}
```

**Step 2: Run test to verify it fails**

Run: `cd apps/desktop/src-tauri && cargo test run_scoring_should_fail_when_parse_quality_too_low -- --nocapture`
Expected: FAIL.

**Step 3: Write minimal implementation**

In scoring flow, immediately after materialization:

```rust
if materialized_resume.parse_quality < 35 {
    return Err("resume_parse_quality_too_low".to_string());
}
```

Emit a `prepare/failed/end` progress update with parse-quality metadata.

**Step 4: Run tests to verify pass**

Run: `cd apps/desktop/src-tauri && cargo test run_scoring_should_fail_when_parse_quality_too_low -- --nocapture`
Expected: PASS.

**Step 5: Commit**

```bash
git add apps/desktop/src-tauri/src/domains/scoring.rs apps/desktop/src-tauri/src/domains/resume_materializer.rs
git commit -m "feat: gate scoring on resume parse quality"
```

---

### Task 3: Persist Invocation Runtime Metadata in Structured Result

**Files:**
- Modify: `apps/desktop/src-tauri/src/domains/scoring.rs`
- Test: `apps/desktop/src-tauri/src/domains/scoring.rs` (unit tests)

**Step 1: Write the failing test**

Add assertion that `structured_result.summary.runtime` exists and contains mode + parse quality.

```rust
assert_eq!(runtime.get("input_mode").and_then(Value::as_str), Some("parsed_text"));
assert!(runtime.get("parse_quality").and_then(Value::as_i64).is_some());
```

**Step 2: Run test to verify it fails**

Run: `cd apps/desktop/src-tauri && cargo test structured_result_should_include_runtime_metadata -- --nocapture`
Expected: FAIL.

**Step 3: Write minimal implementation**

During structured result assembly, append:

```rust
"runtime": {
  "provider": ai_settings.provider,
  "model": ai_settings.model,
  "input_mode": if use_direct_file { "direct_file" } else { "parsed_text" },
  "parse_quality": materialized_resume.parse_quality,
  "latency_ms": elapsed_ms
}
```

**Step 4: Run tests to verify pass**

Run: `cd apps/desktop/src-tauri && cargo test structured_result_should_include_runtime_metadata -- --nocapture`
Expected: PASS.

**Step 5: Commit**

```bash
git add apps/desktop/src-tauri/src/domains/scoring.rs
git commit -m "feat: include runtime metadata in scoring structured result"
```

---

### Task 4: Normalize New Error Codes to User-Facing Messages

**Files:**
- Modify: `apps/desktop/src/lib/scoring-rerun-feedback.ts`
- Test: `apps/desktop/src/lib/scoring-rerun-feedback.test.ts`

**Step 1: Write the failing test**

```ts
it("maps resume_parse_quality_too_low to warning", () => {
  expect(resolveScoringRerunFeedback("resume_parse_quality_too_low")).toEqual({
    tone: "warning",
    message: "简历解析质量较低，请补充更完整简历后重试",
  });
});
```

**Step 2: Run test to verify it fails**

Run: `cd apps/desktop && pnpm test scoring-rerun-feedback.test.ts`
Expected: FAIL.

**Step 3: Write minimal implementation**

Add new mapping branch in `resolveScoringRerunFeedback`.

**Step 4: Run tests to verify pass**

Run: `cd apps/desktop && pnpm test scoring-rerun-feedback.test.ts`
Expected: PASS.

**Step 5: Commit**

```bash
git add apps/desktop/src/lib/scoring-rerun-feedback.ts apps/desktop/src/lib/scoring-rerun-feedback.test.ts
git commit -m "feat: map parse-quality and invocation errors for rerun feedback"
```

---

### Task 5: Surface Runtime Diagnostics in Candidate AI Panel

**Files:**
- Modify: `apps/desktop/src/lib/scoring-structured.ts`
- Modify: `apps/desktop/src/views/CandidatesView.vue`
- Test: `apps/desktop/src/lib/scoring-structured.test.ts`

**Step 1: Write the failing test**

Add test for runtime extraction from `structured_result.summary.runtime`.

```ts
expect(model?.runtime.inputMode).toBe("direct_file");
expect(model?.runtime.parseQuality).toBe(78);
```

**Step 2: Run test to verify it fails**

Run: `cd apps/desktop && pnpm test scoring-structured.test.ts`
Expected: FAIL.

**Step 3: Write minimal implementation**

- Extend `StructuredScoringViewModel` with runtime fields.
- Render compact runtime line in AI panel:

```vue
<p class="m-0 text-xs text-muted">模式：{{ runtime.inputMode }} · 解析质量：{{ runtime.parseQuality }} · 模型：{{ runtime.model }}</p>
```

**Step 4: Run tests to verify pass**

Run: `cd apps/desktop && pnpm test scoring-structured.test.ts`
Expected: PASS.

**Step 5: Commit**

```bash
git add apps/desktop/src/lib/scoring-structured.ts apps/desktop/src/lib/scoring-structured.test.ts apps/desktop/src/views/CandidatesView.vue
git commit -m "feat: show scoring runtime diagnostics in candidate AI panel"
```

---

### Task 6: End-to-End Regression + Typecheck Sweep

**Files:**
- Verify only (no required source changes)

**Step 1: Run targeted Rust tests**

Run: `cd apps/desktop/src-tauri && cargo test materialize_resume_from_file_full_text -- --nocapture`
Expected: PASS.

**Step 2: Run targeted TS tests**

Run: `cd apps/desktop && pnpm test scoring-rerun-feedback.test.ts scoring-structured.test.ts analysis-progress.test.ts`
Expected: PASS.

**Step 3: Run desktop full test suite**

Run: `cd apps/desktop && pnpm test`
Expected: PASS.

**Step 4: Run type checks**

Run: `cd /Users/goya/Repo/Git/Doss && pnpm typecheck`
Expected: PASS.

**Step 5: Commit verification note**

```bash
git add -A
git commit -m "chore: verify resume parsing and model invocation hardening"
```

---

## Execution Notes

1. Keep each task commit-scoped and independently revertible.
2. If Task 2 reveals widespread flaky fixtures, stop and split fixture stabilization into an explicit pre-task.
3. Prefer incremental rollout behind a lightweight runtime guard if production risk rises.
