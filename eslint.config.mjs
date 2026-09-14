// ESLint flat config（ESLint 9+ 的唯一形态）。
//
// 这份配置里最重要的不是"开了哪些规则"，而是 `no-console: 'error'` ——
// 它是宪法「日志规范」的**机械闸门**：裸打印在 lint 阶段就失败，
// 只能走 src/lib/logger.ts 这一个出口。

import js from '@eslint/js'
import eslintConfigPrettier from 'eslint-config-prettier'
import pluginVue from 'eslint-plugin-vue'
import globals from 'globals'
import tseslint from 'typescript-eslint'

export default tseslint.config(
  {
    // 构建产物、依赖、Rust 侧都不属于前端 lint 的范围。
    ignores: ['dist/**', 'node_modules/**', 'src-tauri/**', 'coverage/**'],
  },

  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...pluginVue.configs['flat/recommended'],

  {
    // .vue 里的 <script lang="ts"> 用 TS 解析器；
    // 外层仍由 vue-eslint-parser 负责拆 SFC。
    files: ['**/*.vue'],
    languageOptions: {
      parserOptions: { parser: tseslint.parser },
    },
  },

  {
    languageOptions: {
      ecmaVersion: 'latest',
      sourceType: 'module',
      globals: { ...globals.browser },
    },
    rules: {
      // ★ 裸打印闸门（对应 Rust 侧的 #![deny(clippy::print_stdout)]）。
      'no-console': 'error',

      // 未使用变量报错，但允许用下划线显式表达"我知道它没用"。
      '@typescript-eslint/no-unused-vars': [
        'error',
        { argsIgnorePattern: '^_', varsIgnorePattern: '^_' },
      ],

      // 允许 `interface` 与 `type` 混用；团队偏好不该被 lint 强制。
      '@typescript-eslint/consistent-type-definitions': 'off',
    },
  },

  {
    // 唯一的豁免：日志门面**就是**那个"往 console 写"的实现。
    // 豁免范围精确到一个文件，而不是整目录 —— 不然这个闸门就漏水了。
    files: ['src/lib/logger.ts'],
    rules: { 'no-console': 'off' },
  },

  {
    // 测试文件跑在 Node 侧（vitest），需要 node 全局。
    files: ['**/*.test.ts', 'vite.config.ts', 'eslint.config.mjs'],
    languageOptions: { globals: { ...globals.node } },
  },

  // 必须放最后：关掉所有与 Prettier 冲突的格式类规则，
  // 让 lint 只管"对不对"，格式全交给 prettier。
  eslintConfigPrettier,
)
