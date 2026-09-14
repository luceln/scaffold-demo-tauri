//! # 黄金纵切测试（DESIGN.md §7.2 的落点）
//!
//! ## 为什么这个文件存在
//!
//! 桌面应用没有 HTTP 入口，它的"对外入口"就是 **Tauri command**。
//!
//! 如果测试只是 `repo.create(...)`，那验证的是仓储、不是接口 ——
//! **命令名拼错、参数名对不上、`generate_handler!` 里漏注册**，
//! 这三类最容易犯的错，全都测不出来，而它们恰恰是用户会立刻撞上的故障。
//!
//! 所以这里用的是 `tauri::test` 的 MockRuntime + `get_ipc_response`：
//! 请求真的经过 `invoke_handler` 的派发层，和前端 `invoke()` 走同一条路。
//! **不需要窗口、不需要显示器、不需要 `cargo tauri dev`。**
//!
//! DESIGN.md §7.2 的原话：只有九件套的模板是空壳，测试全绿但没测过任何接口，
//! 是没意义的绿灯。
//!
//! ## 为什么不需要 async / tokio
//!
//! `get_ipc_response` 对 **async 命令会提前返回**（收不到结果），
//! 而本模板的命令全是同步 `fn`（本地 SQLite 亚毫秒，没有必要 async）。
//! 于是测试就是一个普通的 `#[test]`，没有运行时、没有超时陷阱。

use std::path::Path;

use tauri::WebviewWindow;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{
    INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
};
use tauri::webview::InvokeRequest;

/// 起一个"与真实运行同构"的应用。
///
/// 与 `lib.rs::run()` 的唯一差别是 Runtime（Mock 而非 Wry），
/// 数据库用的是真实文件而不是内存库 —— 这一点很关键，
/// 它让"数据到底有没有落盘"变成可断言的事实。
fn boot(db_path: &Path) -> (tauri::App<MockRuntime>, WebviewWindow<MockRuntime>) {
    let conn = tauri_app_lib::db::open(db_path).expect("建库并初始化表结构");
    let app = mock_builder()
        .manage(tauri_app_lib::demo_item::DemoItemRepo::new(conn))
        // 用的是生产同一份注册表 —— 这里如果换成手写的命令列表，
        // 测试就再也发现不了"漏注册"了。
        .invoke_handler(tauri_app_lib::demo_item::register_commands())
        .build(mock_context(noop_assets()))
        .expect("构建 mock 应用");

    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("构建 webview");

    (app, webview)
}

/// 通过真实 IPC 派发层调用一条命令。
///
/// 返回 `Err` 时拿到的是命令序列化出来的错误载荷（我们的 `AppError` → `{code, message}`）。
fn invoke(
    webview: &WebviewWindow<MockRuntime>,
    cmd: &str,
    args: serde_json::Value,
) -> Result<serde_json::Value, serde_json::Value> {
    get_ipc_response(
        webview,
        InvokeRequest {
            cmd: cmd.into(),
            // 回调 id 只是占位：MockRuntime 下没有真正的 JS 回调。
            callback: CallbackFn(0),
            error: CallbackFn(1),
            // ★ URL 必须与 runtime 的"本地源"判定同构：Windows/Android 的本地协议
            //   是 http://tauri.localhost，其余平台是 tauri://localhost。
            //   写死一个，在另一个平台上 is_local_url() 就会返回 false，
            //   ACL 会把所有命令拒之门外（报 "not allowed. Plugin not found"）。
            //   这是 tauri::test 官方文档给的写法，两个平台都算本地源。
            url: if cfg!(any(windows, target_os = "android")) {
                "http://tauri.localhost"
            } else {
                "tauri://localhost"
            }
            .parse()
            .expect("合法 URL"),
            body: InvokeBody::Json(args),
            headers: Default::default(),
            // 必须带上 invoke key，否则请求会在更早的一层就被拒 ——
            // 那会让测试"看起来很严格"，其实根本没走到命令派发。
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|body| body.deserialize().expect("响应应为 JSON"))
}

/// 造一条记录并返回它的 id。
fn create_one(webview: &WebviewWindow<MockRuntime>, name: &str, quantity: i64) -> i64 {
    let created = invoke(
        webview,
        "demo_item_create",
        serde_json::json!({ "input": { "name": name, "quantity": quantity, "note": "备注" } }),
    )
    .expect("create 应当成功");
    created["id"].as_i64().expect("返回值里应有 id")
}

// ---------------------------------------------------------------------------
// 1. 增 → 查 → 改 → 删 全链路
// ---------------------------------------------------------------------------

#[test]
fn crud_round_trip_via_ipc() {
    let dir = tempfile::tempdir().expect("临时目录");
    let (_app, webview) = boot(&dir.path().join("app.db"));

    // ---- 增 ----
    let created = invoke(
        &webview,
        "demo_item_create",
        serde_json::json!({ "input": { "name": "写周报", "quantity": 3, "note": "每周五" } }),
    )
    .expect("create 应当成功");
    assert_eq!(created["name"], "写周报");
    assert_eq!(created["quantity"], 3);
    assert_eq!(created["note"], "每周五");
    // createdAt 而不是 created_at —— camelCase 重命名确实生效了。
    assert!(
        created["createdAt"].is_string(),
        "时间戳应由数据库填充，实际：{created}"
    );
    let id = created["id"].as_i64().expect("应有 id");

    // ---- 列表 ----
    let page = invoke(
        &webview,
        "demo_item_list",
        serde_json::json!({ "limit": 10, "offset": 0 }),
    )
    .expect("list 应当成功");
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"].as_array().expect("items 是数组").len(), 1);

    // ---- 按主键查 ----
    let got =
        invoke(&webview, "demo_item_get", serde_json::json!({ "id": id })).expect("get 应当成功");
    assert_eq!(got["name"], "写周报");

    // ---- 改（部分更新）----
    let updated = invoke(
        &webview,
        "demo_item_update",
        serde_json::json!({ "id": id, "patch": { "quantity": 7 } }),
    )
    .expect("update 应当成功");
    assert_eq!(updated["quantity"], 7);
    // ★ 这一条是 SQL 里 COALESCE 的语义契约：**没传的字段不能被清空**。
    // 如果没有它，update 很容易写成"整体覆盖"，把 note 悄悄抹成空串。
    assert_eq!(updated["name"], "写周报", "未提供的字段不该被改动");
    assert_eq!(updated["note"], "每周五", "未提供的字段不该被改动");

    // ---- 删 ----
    invoke(
        &webview,
        "demo_item_delete",
        serde_json::json!({ "id": id }),
    )
    .expect("delete 应当成功");

    let after = invoke(
        &webview,
        "demo_item_list",
        serde_json::json!({ "limit": 10, "offset": 0 }),
    )
    .expect("删除后 list 应当成功");
    assert_eq!(after["total"], 0, "删除后总数应归零");
}

// ---------------------------------------------------------------------------
// 2. 错误码：校验失败与"不存在"必须是**稳定错误码**，不是文案
// ---------------------------------------------------------------------------

#[test]
fn errors_are_reported_as_stable_codes_over_ipc() {
    let dir = tempfile::tempdir().expect("临时目录");
    let (_app, webview) = boot(&dir.path().join("app.db"));

    // 校验失败：名字是纯空白。
    // 这条用例真正的价值是证明"校验确实挂在了 IPC 路径上" ——
    // 纯函数单测只能证明函数本身对，证明不了它被调用。
    let invalid = invoke(
        &webview,
        "demo_item_create",
        serde_json::json!({ "input": { "name": "   ", "quantity": 0, "note": "" } }),
    )
    .expect_err("空名字必须被拒");
    assert_eq!(invalid["code"], "E_VALIDATION");

    // 上述失败**不能**留下任何残留记录。
    let page = invoke(
        &webview,
        "demo_item_list",
        serde_json::json!({ "limit": 10, "offset": 0 }),
    )
    .expect("list");
    assert_eq!(page["total"], 0, "被拒的创建不该写库");

    // 不存在的主键：查 / 改 / 删三条路径都要给 E_NOT_FOUND。
    for (cmd, args) in [
        ("demo_item_get", serde_json::json!({ "id": 99999 })),
        (
            "demo_item_update",
            serde_json::json!({ "id": 99999, "patch": { "quantity": 1 } }),
        ),
        ("demo_item_delete", serde_json::json!({ "id": 99999 })),
    ] {
        // 这里的语义要看清：`invoke` 返回 `Result<成功载荷, 错误载荷>`，
        // 命令**按预期报错**时拿到的是 `Err`（错误载荷），不是 `Ok`。
        // 所以断言"它必须失败"要用 `expect_err` —— 若返回 `Ok`（说明命令居然成功了）才 panic。
        let err = invoke(&webview, cmd, args)
            .expect_err(&format!("{cmd} 对不存在的 id 应当报错，实际却成功了"));
        assert_eq!(err["code"], "E_NOT_FOUND", "命令 {cmd} 的错误码不对");
        assert!(
            err["message"].as_str().is_some_and(|m| m.contains("99999")),
            "错误信息应带上 id，便于排查；实际：{err}"
        );
    }
}

// ---------------------------------------------------------------------------
// 3. 真的落盘了（而不是内存里假装成功）
// ---------------------------------------------------------------------------

#[test]
fn data_survives_a_second_connection_to_the_same_file() {
    let dir = tempfile::tempdir().expect("临时目录");
    let db_path = dir.path().join("app.db");

    // 第一个应用 + 第一条连接：写。
    // 绑定名带下划线前缀只是为了让"它必须活到作用域结束"这件事显式可见。
    let (_app1, webview1) = boot(&db_path);
    create_one(&webview1, "落盘验证", 1);

    // 第二个应用 + **第二条独立的 SQLite 连接**：读同一个文件。
    // 如果前面的 create 只是内存操作，这里必然读不到 —— 这就是本用例的意义。
    let (_app2, webview2) = boot(&db_path);
    let page = invoke(
        &webview2,
        "demo_item_list",
        serde_json::json!({ "limit": 50, "offset": 0 }),
    )
    .expect("list 应当成功");

    assert_eq!(page["total"], 1, "数据必须真的写进了磁盘文件");
    assert_eq!(page["items"][0]["name"], "落盘验证");
}

// ---------------------------------------------------------------------------
// 4. 分页上限真的在 IPC 层被夹紧（性能底线的可执行证据）
// ---------------------------------------------------------------------------

#[test]
fn page_limit_is_clamped_over_ipc() {
    let dir = tempfile::tempdir().expect("临时目录");
    let (_app, webview) = boot(&dir.path().join("app.db"));

    // 前端要一个天文数字的页大小 —— 必须被夹到 PAGE_MAX。
    // 没有这道闸，一句 invoke 就能把整表拉进内存。
    let page = invoke(
        &webview,
        "demo_item_list",
        serde_json::json!({ "limit": i64::MAX, "offset": -100 }),
    )
    .expect("list 应当成功");

    assert_eq!(page["limit"], tauri_app_lib::demo_item::PAGE_MAX);
    assert_eq!(page["offset"], 0, "负偏移应被归零");
}

// ---------------------------------------------------------------------------
// 5. 保证我们测的确实是 IPC 派发层
// ---------------------------------------------------------------------------

#[test]
fn unregistered_command_is_rejected_by_the_ipc_layer() {
    let dir = tempfile::tempdir().expect("临时目录");
    let (_app, webview) = boot(&dir.path().join("app.db"));

    let err = invoke(&webview, "demo_item_no_such_command", serde_json::json!({}))
        .expect_err("未注册的命令必须失败");

    // 这条断言看似薄弱，但它挡的是一个很具体的退化：如果哪天有人把
    // `get_ipc_response` 换成"直接调用仓储函数"，测试依然全绿 ——
    // 而这条用例会立刻失败。它守的是**测试本身的有效性**。
    assert!(
        !err.is_null(),
        "未注册命令必须返回错误载荷，而不是空值：{err}"
    );
}
