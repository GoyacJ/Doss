import { createRouter, createWebHashHistory } from "vue-router";

export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    {
      path: "/",
      redirect: "/dashboard",
    },
    {
      path: "/dashboard",
      name: "dashboard",
      component: () => import("./views/DashboardView.vue"),
    },
    {
      path: "/jobs",
      name: "jobs",
      component: () => import("./views/JobsView.vue"),
    },
    {
      path: "/candidates",
      name: "candidates",
      component: () => import("./views/CandidatesView.vue"),
    },
    {
      path: "/crawl",
      name: "crawl",
      component: () => import("./views/CrawlView.vue"),
    },
    {
      path: "/interview",
      name: "interview",
      component: () => import("./views/InterviewView.vue"),
    },
    {
      path: "/decision",
      name: "decision",
      component: () => import("./views/DecisionView.vue"),
    },
    {
      path: "/settings",
      name: "settings",
      component: () => import("./views/SettingsView.vue"),
    },
  ],
});
