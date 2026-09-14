<script setup lang="ts">
/**
 * 应用外壳：标题栏 + 内容区。
 *
 * 刻意保持极薄 —— 页面逻辑都在 `components/` 里。
 * 想加第二个页面时，在这里换成路由视图即可，不需要动任何业务组件。
 *
 * 注意这里**只有** `<script setup>`：一个 SFC 里同时用 `<script setup>` 和
 * 带 `export default` 的普通 `<script>` 是极易踩的坑（编译报
 * "default export must be used with <script setup>"）。需要一个普通常量时，
 * 直接写在 `<script setup>` 里就够了。
 */

import { DemoItemPanel } from './components'

// 标题的唯一来源是 index.html 的 <title>，这里只是读它 ——
// 两处各写一遍名字，迟早有一处忘了改。
const appTitle = document.title
</script>

<template>
  <el-container class="app-shell">
    <el-header class="app-header">
      <!--
        用 v-text 而不是双大括号插值。

        原因：项目模板的渲染层用「双大括号 + 变量名」做替换，且必须**只替换已声明的变量名**。
        裸标识符插值（双大括号里只有单个单词）有被误判成模板变量的风险。
        v-text 表达的是同一件事，且永远安全。

        规则：本骨架里出现的「双大括号 + 单个标识符」**只允许是模板占位符**。
      -->
      <span class="app-title" v-text="appTitle" />
      <span class="app-subtitle">Tauri 2 + Vue 3 + SQLite</span>
    </el-header>

    <el-main>
      <DemoItemPanel />
    </el-main>
  </el-container>
</template>

<style scoped>
.app-shell {
  height: 100vh;
}

.app-header {
  display: flex;
  align-items: baseline;
  gap: 12px;
  border-bottom: 1px solid var(--el-border-color);
  background-color: var(--el-bg-color);
}

.app-title {
  font-size: 16px;
  font-weight: 600;
}

.app-subtitle {
  font-size: 12px;
  color: var(--el-text-color-secondary);
}
</style>
