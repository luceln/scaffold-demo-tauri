# 更新日志

本文件格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [未发布]

### 新增

- 首个可运行版本：Tauri 2 + Vue 3 + SQLite 的应用外壳
- 一条打通的 CRUD 纵切（`demo_item`）：新建 / 分页列表 / 按主键查 / 部分更新 / 删除
- 前后端各自的统一日志门面（`src-tauri/src/logging.rs`、`src/lib/logger.ts`），
  裸打印由 clippy 与 ESLint 机械拦住
- Rust 侧走真实 IPC 派发层的纵切测试（`src-tauri/tests/golden_slice.rs`）

[未发布]: https://example.com/compare
