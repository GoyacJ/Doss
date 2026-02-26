<script setup lang="ts">
import { useToastStore } from "../stores/toast";

const toast = useToastStore();

function toneClass(tone: "info" | "success" | "warning" | "danger") {
  if (tone === "success") {
    return "text-brand border-brand/32 bg-brand/10";
  }
  if (tone === "warning") {
    return "text-accent border-accent/35 bg-accent/12";
  }
  if (tone === "danger") {
    return "text-danger border-danger/35 bg-danger/12";
  }
  return "text-brand border-brand/24 bg-brand/8";
}
</script>

<template>
  <Teleport to="body">
    <div class="pointer-events-none fixed top-4 right-4 z-[70] flex w-[360px] max-w-[calc(100vw-2rem)] flex-col gap-2">
      <div
        v-for="item in toast.toasts"
        :key="item.id"
        class="pointer-events-auto border rounded-xl px-3 py-2 shadow-sm backdrop-blur-sm flex items-start justify-between gap-2"
        :class="toneClass(item.tone)"
      >
        <p class="m-0 text-[0.92rem] leading-[1.45] break-all">{{ item.message }}</p>
        <button
          class="cursor-pointer border border-current/25 rounded-md px-1.5 py-0.5 text-[0.72rem] bg-transparent"
          @click="toast.dismiss(item.id)"
        >
          关闭
        </button>
      </div>
    </div>
  </Teleport>
</template>
