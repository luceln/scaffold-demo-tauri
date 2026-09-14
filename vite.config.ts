// Vite + Vitest 共用一份配置。
//
// 从 `vitest/config` 导入 defineConfig（而不是从 `vite`）：这样 `test` 段
// 有类型，无需额外引 @types/node，也不需要 /// <reference>。

import vue from '@vitejs/plugin-vue'
import { defineConfig } from 'vitest/config'

export default defineConfig({
  plugins: [vue()],

  // Tauri 会自己打印大量构建信息，Vite 再清屏会把它们冲掉。
  clearScreen: false,

  server: {
    // 必须与 src-tauri/tauri.conf.json 的 build.devUrl 一致。
    port: 1420,
    // ★ 必须显式钉 IPv4。不写 host 时 Vite 跟着 `localhost` 的 DNS 解析走，
    //   Windows/Node 17+ 上会只绑到 IPv6 的 [::1] —— 于是 IPv4 的 127.0.0.1:1420
    //   连不上（实测挂死不拒连），WebView2 白屏、整个应用点不动。
    host: '127.0.0.1',
    // 端口被占就直接失败，不要"顺手换个端口" ——
    // 否则 Tauri 会去连 1420 而拿到空白窗口，排查半天。
    strictPort: true,
    watch: {
      // src-tauri 的变更由 cargo 自己监听；Vite 再去 watch 会触发无意义的重启。
      ignored: ['**/src-tauri/**'],
    },
  },

  build: {
    target: 'es2022',
    // 生产包不给 sourcemap：桌面应用的分发包里带着源码映射是信息泄露。
    sourcemap: false,
  },

  test: {
    environment: 'happy-dom',
    include: ['src/**/*.test.ts'],
    // 显式 import { describe, it, expect } —— 不开 globals，
    // 少一处需要写进 tsconfig types 的隐式依赖。
    globals: false,
  },
})
