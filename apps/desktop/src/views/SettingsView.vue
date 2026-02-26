<script setup lang="ts">
import { ref } from "vue";
import { useRecruitingStore } from "../stores/recruiting";

const store = useRecruitingStore();
const aiProvider = ref("Cloud Provider A");
const complianceMode = ref(true);
</script>

<template>
  <section class="page">
    <header class="page-header">
      <h2>设置</h2>
    </header>

    <article class="panel">
      <h3>AI配置</h3>
      <label class="setting-item">
        模型供应商
        <select v-model="aiProvider">
          <option>Cloud Provider A</option>
          <option>Cloud Provider B</option>
        </select>
      </label>
      <p>当前策略: 多模型抽象层 + 单供应商默认。</p>
    </article>

    <article class="panel">
      <h3>采集合规策略</h3>
      <label class="setting-item checkbox">
        <input v-model="complianceMode" type="checkbox" />
        默认使用合规模式（推荐）
      </label>
      <p>高级模式仍可在任务创建时显式选择。</p>
    </article>

    <article class="panel">
      <h3>系统状态</h3>
      <p>数据库路径: {{ store.health?.dbPath || "-" }}</p>
      <p>Sidecar: {{ store.sidecarHealthy ? "在线" : "离线" }}</p>
    </article>
  </section>
</template>
