//! # scaffold-demo-tauri
//!
//! 应用装配层：把日志、数据库、命令注册表拼成一个可运行的 Tauri 应用。
//!
//! ## ⚠️ 本文件有一条不能破的规矩
//!
//! **绝不能引用 `tauri::test`。**
//!
//! 它被 `#[cfg(any(test, feature = "test"))]` 门住，只在本 crate 自己的
//! 单元测试、或开了 `test` feature 时存在。而 `test` feature 只写在
//! `[dev-dependencies]` 里 —— 也就是说**生产构建里这个模块根本不存在**，
//! 引用它会得到一堆 `E0432 unresolved import`。
//!
//! 无 GUI 的 IPC 纵切测试放在 `tests/golden_slice.rs`（集成测试是独立 crate，
//! 能拿到 dev-dependencies 的 `test` feature）。

// 把「裸打印」变成编译错误（生产级六项第 6 项 / 宪法日志规范）。
//
// 这三条 lint 是**机械闸门**：靠人自觉"我记得用 tracing"迟早会漏，
// 而写成 deny 之后，想裸打印的人会直接编译不过。
// CLI 工具的正常结果输出不受此限，但本仓库是 GUI 应用，没有那种需求。
#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// 未使用的 Result 是典型的"错误被静默吞掉"，在启动路径上代价最大。
#![deny(unused_must_use)]

pub mod db;
pub mod demo_item;
pub mod error;
pub mod logging;

use tauri::Manager as _;

/// 启动桌面应用。
///
/// `main.rs` 只调用这一个函数 —— 这样 `main.rs` 里除了三行胶水没有别的逻辑，
/// 而所有"真正的装配"都能被测试与阅读。
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // ① 日志必须最先初始化：后面每一行 tracing 都依赖它。
            logging::init(app.handle())?;

            // ② 数据库放在**应用数据目录**，而不是当前工作目录。
            //
            // 用 CWD 是个经典坑：从 IDE 启动和在终端启动的 CWD 不同，
            // 于是同一个应用会各自产生一个数据库，表现为"数据莫名不见了"。
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("app.db");

            let conn = db::open(&db_path)?;
            tracing::info!(db = %db_path.display(), "db.ready");

            // ③ 交给 Tauri 托管：命令通过 `tauri::State<DemoItemRepo>` 取它。
            app.manage(demo_item::DemoItemRepo::new(conn));
            Ok(())
        })
        // ④ 命令注册表。新增命令时改 `demo_item::register_commands`，
        //    这里不需要动 —— 注册表归模块所有，是"私有命令"约束的直接后果。
        .invoke_handler(demo_item::register_commands())
        .run(tauri::generate_context!())
        .expect("Tauri 应用启动失败");
}
