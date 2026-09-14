/// <reference types="vite/client" />

// 让 TS 认识 `import App from './App.vue'`。
// Vue 3.5 + vue-tsc 3.x 已能通过插件解析 SFC，但显式声明能让
// 编辑器的"跳转到定义"在没跑 vue-tsc 时也正常工作。
declare module '*.vue' {
  import type { DefineComponent } from 'vue'

  const component: DefineComponent<Record<string, unknown>, Record<string, unknown>, unknown>
  export default component
}
