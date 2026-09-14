# scaffold-demo-tauri

scaffold-demo-tauri 是一个 Tauri 2 桌面工具：**Vue 3 前端 + Rust 后端 + SQLite 本地持久层**，
自带一条打通到数据库的 CRUD 纵切（`demo_item`：新建 / 分页列表 / 按主键查 / 部分更新 / 删除）。

---

## 前置依赖

| 工具 | 版本 | 说明 |
|---|---|---|
| Node | ≥ 22 | 前端构建 |
| pnpm | 11.24.0 | **只能用 pnpm**（`packageManager` 字段已钉版本，混用 npm/yarn 会让锁文件打架） |
| Rust | ≥ 1.85.0 | 2024 edition 的下限。用 rustup 装：`rustup toolchain install stable` |
| 平台依赖 | — | Windows：MSVC 工具链 + WebView2（Win11 自带）。macOS / Linux：见 [Tauri Prerequisites](https://tauri.app/start/prerequisites/) |

SQLite **不需要**单独安装 —— `rusqlite` 用的是 `bundled` feature，源码一起编进来。

---

## 快速开始

```bash
pnpm install                     # 前端依赖
cargo fetch --manifest-path src-tauri/Cargo.toml   # 预热 Rust 依赖

pnpm tauri dev                   # 启动（Vite + Tauri 热重载）
```

### 生成项目后的第一步

把锁文件一起提交：

```bash
git add pnpm-lock.yaml src-tauri/Cargo.lock
```

这是一个**应用**而不是库，锁文件必须进版本库。CI 用的是
`pnpm install --frozen-lockfile`，锁文件没提交会直接在第一次 CI 上失败
（这是刻意设计的：早失败比"某天依赖悄悄变了导致构建挂掉"好得多）。

---

## 常用命令

```bash
make ci        # 本地完整 CI 检查面（与 .github/workflows/ci.yml 同一条命令）
make check     # 只跑静态检查与测试（不含构建）
make dev       # 开发模式
make build     # 构建发布包
make help      # 全部 target
```

> **本地预检用 `make ci`（或 Windows 上 `pnpm run ci`）**——它和 CI 跑
> 完全相同的命令。只跑子集（比如只 `make check`）会让「本地过了 CI 挂」
> 有机可乘：任务集合不同＝等价性是假的。

**不用 make 的等价命令**（Windows 上没有 make 时照这个抄）：

| 目的 | 命令 |
|---|---|
| **本地完整 CI 面** | `pnpm run ci` |
| 前端 lint | `pnpm run lint` |
| 前端类型检查 | `pnpm run typecheck` |
| 前端测试 | `pnpm run test` |
| 前端构建 | `pnpm run build` |
| Rust 格式检查 | `pnpm run rust:fmt` |
| Rust lint | `pnpm run rust:clippy` |
| Rust 测试 | `pnpm run rust:test` |
| Rust 构建 | `pnpm run rust:build` |
| 全部静态检查 | `pnpm run check`（只覆盖前端，不含构建） |

> `pnpm run build` **必须**先于 `cargo build`：`src-tauri/tauri.conf.json` 的
> `frontendDist` 指向 `../dist`，Rust 侧打包时要把它一起编进去。

---

## 目录结构

```
.
├── src/                            前端（Vue 3 + TS）
│   ├── lib/logger.ts               ★ 前端唯一日志出口（no-console 唯一豁免）
│   ├── lib/api.ts                  ★ 前端唯一 IPC 出口（invoke 封装）
│   ├── lib/api.test.ts             钉住命令名与参数形状
│   ├── lib/logger.test.ts          纯函数测试：级别过滤 + 脱敏
│   ├── types/demo_item.ts          与 Rust DTO 一一对应的类型
│   └── components/DemoItemPanel.vue  ★ 纵切 UI（Element Plus）
└── src-tauri/                      Rust 后端
    ├── build.rs                    Tauri 代码生成 + Windows 清单注入
    ├── app.manifest                comctl32 v6 清单（**不能删**，见下）
    ├── src/lib.rs                  应用装配（**不得引用 tauri::test**）
    ├── src/logging.rs              ★ Rust 唯一日志出口
    ├── src/error.rs                AppError + 稳定错误码
    ├── src/db.rs                   SQLite 连接 + 表结构（单一事实源）
    ├── src/demo_item.rs            ★ 纵切本体：DTO + 校验 + 仓储 + 5 条命令
    └── tests/golden_slice.rs       ★ 走真实 IPC 派发层的纵切测试
```

---

## 测试策略：为什么有两个"接口测试"

| 文件 | 跑在哪 | 守住什么 |
|---|---|---|
| `src-tauri/tests/golden_slice.rs` | 真 Rust、真 IPC 派发层（MockRuntime，**不要窗口**） | 命令存在、参数能解析、SQL 正确、错误码正确、**数据真的落盘** |
| `src/lib/api.test.ts` | 纯 Node，毫秒级 | 前端发出去的命令名与参数**形状**对不对 |

两者都不能省。后端测试永远发现不了"前端把 `input` 写成了 `item`"；
前端测试也永远发现不了"命令根本没注册"。缺一边就有一半的故障能溜过去。

---

## 日志

一条链路，两端各一个门面：

| 端 | 文件 | 落地 |
|---|---|---|
| Rust | `src-tauri/src/logging.rs` | **JSON 结构化 + 按天滚动**落文件；同时打一份到 stderr |
| 前端 | `src/lib/logger.ts` | webview 控制台（按级别用 console 的对应方法） |

**裸打印是被机械拦住的**，不靠自觉：

- Rust：`lib.rs` 顶部 `#![deny(clippy::print_stdout, print_stderr, dbg_macro)]`，
  加上 CI 里的 `cargo clippy -- -D warnings`。
- 前端：`eslint.config.mjs` 里 `no-console: 'error'`，**唯一豁免是 `src/lib/logger.ts`**。

日志文件位置（由 Tauri 的应用目录决定）：

- Windows：`%LOCALAPPDATA%\<bundle identifier>\logs\`
- macOS：`~/Library/Logs/<bundle identifier>/`
- Linux：`~/.local/share/<bundle identifier>/logs/`

用 `RUST_LOG` 调级别，例如 `RUST_LOG=debug pnpm tauri dev`。

> 前端日志**没有**转发到后端统一落盘。这是刻意的取舍：那需要引入
> `tauri-plugin-log` 并额外开 capabilities 权限，与"最小权限"基线冲突，
> 还会形成第二条日志链路与第二套轮转策略。真要统一，做法是在
> `src/lib/logger.ts` 的 sink 里换成调用一条 `log_frontend` 命令。

---

## 数据

SQLite 文件在**应用数据目录**（不是当前工作目录）：

- Windows：`%APPDATA%\<bundle identifier>\app.db`
- macOS：`~/Library/Application Support/<bundle identifier>/app.db`
- Linux：`~/.local/share/<bundle identifier>/app.db`

表结构只有一份事实源：`src-tauri/src/db.rs` 里的 `SCHEMA` 常量。
生产启动与测试都执行它，所以不会出现"测试库和真实库 schema 漂移"。

需要重置数据时，删掉上面那个 `app.db` 即可（连同 `-wal` / `-shm` 一起删）。

---

## 已知的坑（都是真踩过的）

### 1. Windows 上 `cargo test` 起不来（`0xc0000139`）

**症状**：编译全过，可执行文件一启动就退，退出码 `0xc0000139 STATUS_ENTRYPOINT_NOT_FOUND`，
没有任何 Rust 层报错。测试宿主与 example 崩得一模一样。

**根因**：缺 `comctl32.dll` 的 `TaskDialogIndirect` —— 这个导出只在 comctl32 **v6**，
而 v6 只有程序**自带清单**声明 `Microsoft.Windows.Common-Controls 6.0.0.0` 时才加载。

**为什么 `tauri-build` 救不了**：它会编这份清单，但底层 `embed-resource` 发出的是
`cargo:rustc-link-arg-bins` —— **只挂 bin 目标**，测试和 example 拿不到。

**本项目的处理**：`app.manifest` 保存清单，`build.rs` 里关掉 tauri-build 那半边，
再用**不限目标**的 `cargo:rustc-link-arg` 统一注入。

> 🔧 **不要删 `app.manifest`，也不要"简化" `build.rs`** ——
> 删掉的后果是 Windows 上所有测试静默崩溃，而错误信息完全不指向真正原因。

### 1b. `app.manifest` 的注释里不能出现 `--`（连续两个连字符）

XML 规范禁止注释内出现 `--`。link.exe 通过 `mt.exe` 处理清单时会因此失败，
报的却是完全指不到原因的错：

```
general error c1010070: Failed to load and parse the manifest.
LINK : fatal error LNK1327: 运行 mt.exe 期间出错
```

一个顺手写下的 `--`（比如 `cargo run --example`）就足够破坏整个链接。
**想给清单加注释就用英文，并且别用 `--` 当破折号。**

（实测过：UTF-8 无 BOM 的中文本身**不会**导致这个问题；`--` 才会。
本文件保持纯 ASCII 只是为了少一个变量。）

自查办法：拿任意 XML 解析器读一遍，良构性检查会直接给出行号列号。

### 2. 库名为什么叫 `tauri_app_lib` 而不是项目名

Rust 包名可以带连字符（`my-tool`），但 crate 标识符不行（`use my-tool::...` 非法）。
正常做法是做「连字符 → 下划线」变换，但项目模板的渲染层是**纯占位符替换**，
没有变换能力。所以把库名钉成常量，让整个骨架不依赖任何名称变换。
`src/main.rs` 里调的是 `tauri_app_lib::run()`，看起来和包名不一致 —— 这是有意的。

### 3. 时间戳是 UTC，且不带时区标记

SQLite 的 `datetime('now')` 返回 `"2026-09-10 12:00:00"`，是 UTC。
`new Date("2026-09-10 12:00:00")` 在不同 JS 引擎上解析结果不同（有的按本地时区），
同一个值会差好几个小时。`DemoItemPanel.vue` 的 `formatTime()` 显式补上 `T` 与 `Z`
把语义钉死。写新的时间显示逻辑时请复用这个做法。

### 4. `#[tauri::command]` 的函数必须是**完全私有**的

写成 `pub` / `pub(crate)` 会让宏生成的 `__cmd__xxx` 与模块内 `use` 撞名，报
`E0255: defined multiple times`。约定是：命令写在本模块里当私有 `fn`，
由模块导出 `register_commands()` 完成注册。加命令时**别忘了往那里加一行**。

### 5. `pnpm build` 会提示 chunk > 500 kB（预期内，非错误）

Element Plus 是整包引入的，产物里 JS 约 990 kB（gzip 约 318 kB），
所以 vite 会打一条 chunk 体积警告。**这条警告不影响退出码**（仍是 0）。

真要把体积压下去时，再引入 `unplugin-vue-components` 做按需引入，
或对 Element Plus 做 `manualChunks` 分包 —— 但那是优化项，不是模板的默认选择：
本模板优先保证"开箱即跑、少一层构建魔法"。改之前请先量一下收益。

### 6. `tauri dev` 白屏：窗口开了但里面一片空白（排查顺序很重要）

窗口出现 ≠ 页面加载成功。本项目的可见内容**全部**由 Vue 渲进 `#app`，
`index.html` 的 body 里只有 `<div id="app"></div>` —— 所以"JS 没执行"和
"页面没加载"看起来一模一样，都是白屏。按下面顺序排查（每步都有实测依据）：

1. **`tauri dev` 是否真的拉起了 Vite？** 终端里应出现
   `Running BeforeDevCommand (pnpm dev)` 与 `VITE ... ready`。
   这依赖 `tauri.conf.json` 的 `build.beforeDevCommand` / `beforeBuildCommand`。
   **这两个字段删掉任何一个，`tauri dev` / `tauri build` 都会残废**：
   dev 变成无限 `Waiting for your frontend dev server`，build 打出的包里嵌着空 `dist`。
2. **Vite 绑的是不是 IPv4？** 不写 `server.host` 时 Vite 跟着 `localhost` 的
   DNS 解析走，Windows + Node 17+ 上会只绑 `[::1]`（IPv6），实测 IPv4 的
   `127.0.0.1:1420` 连 TCP 都握不上手。所以本模板把 `vite.config.ts` 的
   `server.host` 钉成 `127.0.0.1`、`devUrl` 钉成 `http://127.0.0.1:1420`，
   **两处必须一起改**。验证：`netstat -ano | findstr 1420` 应看到 `127.0.0.1:1420`。
3. **webview 到底请求了哪些模块？** 在 `vite.config.ts` 里临时加一个
   `configureServer` 中间件打印 `req.url`，重启后看模块图走到哪一步断掉 ——
   断点就是第一个解析/执行失败的模块。
4. **WebView2 的用户数据目录坏了**（症状：模块图走到一半停住、出现
   `msedgewebview2 已停止工作 / CrashSender` 崩溃框，但同样的 URL 用系统
   Edge/Chrome 一切正常）。删掉 `%LOCALAPPDATA%\<bundle标识>\EBWebView`
   即可 —— 纯缓存，下次启动自动重建。

> 判定"是不是前端自身的问题"最快的隔离实验：
> `msedge --headless=new --dump-dom http://127.0.0.1:1420/` ——
> 无头 Edge 会执行 JS，DOM 里能找到 `条目管理` 就是前端好的，问题在 webview 层。

---

## 发布

```bash
pnpm tauri build
```

产物在 `src-tauri/target/release/bundle/`。

如果本项目带了 `.github/workflows/release.yml`，打一个 `v*` 标签就会自动出
三平台安装包（草稿 release）。签名与公证需要配置仓库 Secrets，详见该文件内的注释。
