<script setup lang="ts">
import { reactive } from "vue";
import { useRecruitingStore } from "../stores/recruiting";

const store = useRecruitingStore();

const form = reactive({
  title: "",
  company: "",
  city: "",
  salary_k: "",
  description: "",
});

async function submit() {
  if (!form.title || !form.company) {
    return;
  }

  await store.addJob({
    title: form.title,
    company: form.company,
    city: form.city || undefined,
    salary_k: form.salary_k || undefined,
    description: form.description || undefined,
  });

  form.title = "";
  form.company = "";
  form.city = "";
  form.salary_k = "";
  form.description = "";
}
</script>

<template>
  <section class="page">
    <header class="page-header">
      <h2>职位池</h2>
    </header>

    <article class="panel">
      <h3>创建职位</h3>
      <div class="form-grid">
        <input v-model="form.title" placeholder="职位名称" />
        <input v-model="form.company" placeholder="公司" />
        <input v-model="form.city" placeholder="城市" />
        <input v-model="form.salary_k" placeholder="薪资区间(k)" />
      </div>
      <textarea v-model="form.description" placeholder="岗位描述 / 技能要求"></textarea>
      <button class="button" @click="submit">保存职位</button>
    </article>

    <article class="panel">
      <h3>已创建职位</h3>
      <table class="table">
        <thead>
          <tr>
            <th>职位</th>
            <th>公司</th>
            <th>城市</th>
            <th>薪资</th>
            <th>更新时间</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="job in store.jobs" :key="job.id">
            <td>{{ job.title }}</td>
            <td>{{ job.company }}</td>
            <td>{{ job.city || "-" }}</td>
            <td>{{ job.salary_k || "-" }}</td>
            <td>{{ job.updated_at }}</td>
          </tr>
        </tbody>
      </table>
    </article>
  </section>
</template>
