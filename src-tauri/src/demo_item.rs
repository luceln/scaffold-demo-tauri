//! **黄金纵切**：一条从 IPC 命令打通到持久层的完整竖切（DESIGN.md §7.2）。
//!
//! 选它当纵切的理由：一个"条目管理"页面同时覆盖了增、删、改、查、分页五种形态，
//! 又足够小 —— 你读完这一个文件，就知道这个模板怎么处理命令、校验、SQL、错误码。
//!
//! # 为什么命令必须是**私有** `fn`
//!
//! `#[tauri::command]` 会生成一个 `#[macro_export]` 的辅助宏（`__cmd__<名字>`）。
//! `macro_export` 会把宏提到 **crate 根部**。如果命令函数是 `pub` / `pub(crate)`，
//! 宏展开过程中产生的 `use` 会与模块内的同名项冲突，报
//! `E0255: the name '__cmd__xxx' is defined multiple times`。
//!
//! 所以本文件的约定是：
//! - 命令一律写成**完全私有**的 `fn`；
//! - 由本模块自己导出一个 `register_commands()` 把私有命令注册进 handler。
//!
//! 代价是 lib.rs 里看不到"有哪些命令"，收益是它能编译。

use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::logging;

// ---------- 限额（校验与分页的唯一事实源）----------

/// `name` 的字符数上限。
pub const NAME_MAX: usize = 64;
/// `note` 的字符数上限。
pub const NOTE_MAX: usize = 512;
/// `quantity` 的取值上限（业务上是个库存数，负数无意义）。
pub const QUANTITY_MAX: i64 = 1_000_000;
/// 列表默认页大小。
pub const PAGE_DEFAULT: i64 = 50;
/// 列表页大小上限。
///
/// **这一条是性能底线**（生产级六项第 3 项）：没有它，前端一句
/// `invoke("demo_item_list", { limit: 1e9 })` 就能把整表拉进内存。
/// 分页上限不是"体验优化"，是防呆。
pub const PAGE_MAX: i64 = 200;

/// 被测对象实体名，出现在 `NotFound` 错误里。
const ENTITY: &str = "demo_item";

// ---------- DTO：IPC 的线上形状 ----------

/// 一个条目。
///
/// `rename_all = "camelCase"`：Rust 惯例是 snake_case，TS 惯例是 camelCase。
/// 在**这一个**边界上统一转换，比让两边全程迁就对方便宜得多。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoItem {
    pub id: i64,
    pub name: String,
    pub quantity: i64,
    pub note: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 新建入参（无 id / 无时间戳 —— 那两项由数据库决定）。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewDemoItem {
    pub name: String,
    #[serde(default)]
    pub quantity: i64,
    #[serde(default)]
    pub note: String,
}

/// 部分更新入参。
///
/// 三个字段全是 `Option`：`None` 表示"这一项不改"（不是"改成空"）。
/// SQL 侧用 `COALESCE` 实现同一语义，两边必须对齐。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoItemPatch {
    pub name: Option<String>,
    pub quantity: Option<i64>,
    pub note: Option<String>,
}

impl DemoItemPatch {
    /// 本次实际改动的字段名。用于日志打点，也用于"空 patch"校验。
    pub fn changed_fields(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.name.is_some() {
            out.push("name");
        }
        if self.quantity.is_some() {
            out.push("quantity");
        }
        if self.note.is_some() {
            out.push("note");
        }
        out
    }
}

/// 分页结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

// ---------- 纯函数：校验与规范化 ----------
//
// 抽成纯函数只有一个理由：**它们能脱离数据库被单元测试**。
// 校验规则是这块代码里最容易写错、也最容易被后续改动破坏的部分
// （宪法阶段二：优先交付纯函数核心，IO 不写单测、用纵切测）。

/// 规范化并校验新建入参。
///
/// 规范化的含义：`trim()` 掉首尾空白，并统一做空白校验。
/// 这样"名字是两个空格"这类输入不会变成一条查询起来很困惑的记录。
pub fn normalize_new(input: NewDemoItem) -> Result<NewDemoItem, AppError> {
    let name = input.name.trim().to_owned();
    validate_name(&name)?;

    let note = input.note.trim().to_owned();
    validate_note(&note)?;
    validate_quantity(input.quantity)?;

    Ok(NewDemoItem {
        name,
        quantity: input.quantity,
        note,
    })
}

/// 规范化并校验部分更新入参。
pub fn normalize_patch(patch: DemoItemPatch) -> Result<DemoItemPatch, AppError> {
    let name = match patch.name {
        Some(raw) => {
            let trimmed = raw.trim().to_owned();
            validate_name(&trimmed)?;
            Some(trimmed)
        }
        None => None,
    };
    let note = match patch.note {
        Some(raw) => {
            let trimmed = raw.trim().to_owned();
            validate_note(&trimmed)?;
            Some(trimmed)
        }
        None => None,
    };
    if let Some(q) = patch.quantity {
        validate_quantity(q)?;
    }

    let out = DemoItemPatch {
        name,
        quantity: patch.quantity,
        note,
    };
    // 一个什么都没改的 patch 通常是前端 bug（提交按钮没禁用）。与其静默地
    // 返回原记录让调用方以为成功了，不如明确报错。
    if out.changed_fields().is_empty() {
        return Err(AppError::Validation(
            "patch 里没有任何要修改的字段".to_owned(),
        ));
    }
    Ok(out)
}

/// 把用户传来的分页参数夹紧到合法区间。
///
/// 返回 `(limit, offset)`。这里的 `clamp` / `max` 是**静默修正**而不是报错：
/// 分页参数越界属于"前端算错了"，没必要让整个列表加载失败。
pub fn clamp_page(limit: Option<i64>, offset: Option<i64>) -> (i64, i64) {
    let limit = limit.unwrap_or(PAGE_DEFAULT).clamp(1, PAGE_MAX);
    let offset = offset.unwrap_or(0).max(0);
    (limit, offset)
}

fn validate_name(name: &str) -> Result<(), AppError> {
    if name.is_empty() {
        return Err(AppError::Validation("name 不能为空".to_owned()));
    }
    // 用 chars().count() 而不是 len()：len() 是字节数，中文名会被误判超长。
    if name.chars().count() > NAME_MAX {
        return Err(AppError::Validation(format!("name 超过 {NAME_MAX} 个字符")));
    }
    Ok(())
}

fn validate_note(note: &str) -> Result<(), AppError> {
    if note.chars().count() > NOTE_MAX {
        return Err(AppError::Validation(format!("note 超过 {NOTE_MAX} 个字符")));
    }
    Ok(())
}

fn validate_quantity(quantity: i64) -> Result<(), AppError> {
    if !(0..=QUANTITY_MAX).contains(&quantity) {
        return Err(AppError::Validation(format!(
            "quantity 必须在 0..={QUANTITY_MAX} 之间"
        )));
    }
    Ok(())
}

// ---------- 仓储：唯一接触 SQL 的地方 ----------

/// 条目仓储。持有数据库连接。
///
/// 用 `Mutex<Connection>` 而不是连接池：这是**单进程桌面应用**，
/// 并发上限就是"UI 线程 + 若干后台任务"，连接池带来的复杂度换不来收益。
///
/// 升级路径：若某个命令将来要做网络请求或重计算，把它改成 `async` 并用
/// `tauri::async_runtime::spawn_blocking` 包住数据库访问；那时再考虑
/// `r2d2_sqlite` 之类的池。现在不需要。
pub struct DemoItemRepo {
    conn: Mutex<Connection>,
}

impl DemoItemRepo {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Mutex::new(conn),
        }
    }

    /// 取连接锁。
    ///
    /// 刻意不 `unwrap()`：Mutex 中毒意味着"持锁线程 panic 了"，
    /// 此时数据状态未知。把它转成 `E_INTERNAL` 让上层知道"这是内部故障"，
    /// 比 panic 更能保住 UI（用户至少能看到一个错误提示）。
    fn lock(&self) -> Result<MutexGuard<'_, Connection>, AppError> {
        self.conn
            .lock()
            .map_err(|_| AppError::Internal("数据库连接锁已中毒（此前有线程 panic）".to_owned()))
    }

    fn get_with(conn: &Connection, id: i64) -> Result<DemoItem, AppError> {
        // 全部使用参数绑定（?1）—— 绝不拼接字符串。这是 §8.5 的底线：
        // 一旦开始拼 SQL，"输入校验的一处疏漏"就升级成"任意 SQL 执行"。
        conn.query_row(
            "SELECT id, name, quantity, note, created_at, updated_at
               FROM demo_item WHERE id = ?1",
            params![id],
            row_to_item,
        )
        .map_err(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => AppError::NotFound { entity: ENTITY, id },
            other => AppError::Db(other),
        })
    }

    fn create(&self, input: &NewDemoItem) -> Result<DemoItem, AppError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO demo_item (name, quantity, note) VALUES (?1, ?2, ?3)",
            params![input.name, input.quantity, input.note],
        )?;
        // last_insert_rowid 与上面的 INSERT 在同一次加锁内，不会被别的写插入打断。
        let id = conn.last_insert_rowid();
        Self::get_with(&conn, id)
    }

    fn list(&self, limit: i64, offset: i64) -> Result<Page<DemoItem>, AppError> {
        let conn = self.lock()?;
        let total: i64 = conn.query_row("SELECT COUNT(*) FROM demo_item", [], |row| row.get(0))?;

        let mut stmt = conn.prepare(
            "SELECT id, name, quantity, note, created_at, updated_at
               FROM demo_item ORDER BY id DESC LIMIT ?1 OFFSET ?2",
        )?;
        let items = stmt
            .query_map(params![limit, offset], row_to_item)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Page {
            items,
            total,
            limit,
            offset,
        })
    }

    fn update(&self, id: i64, patch: &DemoItemPatch) -> Result<DemoItem, AppError> {
        let conn = self.lock()?;
        let affected = conn.execute(
            "UPDATE demo_item
                SET name      = COALESCE(?2, name),
                    quantity   = COALESCE(?3, quantity),
                    note       = COALESCE(?4, note),
                    updated_at = datetime('now')
              WHERE id = ?1",
            params![
                id,
                patch.name.as_deref(),
                patch.quantity,
                patch.note.as_deref()
            ],
        )?;
        // 影响 0 行 = 这条记录不存在。**不能**当成成功：
        // 前端会以为改成功了，然后困惑于列表没变化。
        if affected == 0 {
            return Err(AppError::NotFound { entity: ENTITY, id });
        }
        Self::get_with(&conn, id)
    }

    fn delete(&self, id: i64) -> Result<(), AppError> {
        let conn = self.lock()?;
        let affected = conn.execute("DELETE FROM demo_item WHERE id = ?1", params![id])?;
        if affected == 0 {
            return Err(AppError::NotFound { entity: ENTITY, id });
        }
        Ok(())
    }
}

fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<DemoItem> {
    Ok(DemoItem {
        id: row.get("id")?,
        name: row.get("name")?,
        quantity: row.get("quantity")?,
        note: row.get("note")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

// ---------- 命令入口 ----------

/// 统一的命令包装：打点 + 计时 + 错误归集。
///
/// 日志规范（宪法）要求"所有命令入口记录操作类型、参数摘要、执行耗时"。
/// 把它做成一个函数而不是靠每个命令自己写，是因为**靠自觉的打点迟早会漏**。
fn run_command<T>(
    command: &'static str,
    body: impl FnOnce() -> Result<T, AppError>,
) -> Result<T, AppError> {
    let started = Instant::now();
    tracing::info!(command, "command.start");

    let outcome = body();
    let elapsed_ms = started.elapsed().as_millis() as u64;

    match &outcome {
        Ok(_) => tracing::info!(command, elapsed_ms, "command.ok"),
        Err(err) => tracing::error!(
            command,
            elapsed_ms,
            code = err.code(),
            error = %err,
            "command.err"
        ),
    }
    outcome
}

/// 新建条目。
///
/// 同步 `fn` 而不是 `async fn` —— 这不是偷懒，是**测试可行性**决定的：
/// `tauri::test::get_ipc_response` 对 async 命令会提前返回，拿不到结果。
/// 本地 SQLite 的读写是亚毫秒级，用同步命令 + Mutex 串行化完全够用。
#[tauri::command]
fn demo_item_create(
    state: tauri::State<'_, DemoItemRepo>,
    input: NewDemoItem,
) -> Result<DemoItem, AppError> {
    run_command("demo_item_create", || {
        let input = normalize_new(input)?;
        tracing::debug!(
            name = %logging::field("name", &input.name),
            quantity = input.quantity,
            note_len = input.note.chars().count(),
            "command.params"
        );
        state.create(&input)
    })
}

/// 分页列出条目（按 id 倒序，新的在前）。
#[tauri::command]
fn demo_item_list(
    state: tauri::State<'_, DemoItemRepo>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Page<DemoItem>, AppError> {
    run_command("demo_item_list", || {
        let (limit, offset) = clamp_page(limit, offset);
        tracing::debug!(limit, offset, "command.params");
        state.list(limit, offset)
    })
}

/// 按主键取单条。
#[tauri::command]
fn demo_item_get(state: tauri::State<'_, DemoItemRepo>, id: i64) -> Result<DemoItem, AppError> {
    run_command("demo_item_get", || {
        tracing::debug!(id, "command.params");
        let conn = state.lock()?;
        DemoItemRepo::get_with(&conn, id)
    })
}

/// 部分更新。
#[tauri::command]
fn demo_item_update(
    state: tauri::State<'_, DemoItemRepo>,
    id: i64,
    patch: DemoItemPatch,
) -> Result<DemoItem, AppError> {
    run_command("demo_item_update", || {
        let patch = normalize_patch(patch)?;
        tracing::debug!(id, changed = ?patch.changed_fields(), "command.params");
        state.update(id, &patch)
    })
}

/// 删除。
#[tauri::command]
fn demo_item_delete(state: tauri::State<'_, DemoItemRepo>, id: i64) -> Result<(), AppError> {
    run_command("demo_item_delete", || {
        tracing::debug!(id, "command.params");
        state.delete(id)
    })
}

/// 把本模块的命令注册进 Tauri 的 IPC 派发层。
///
/// 这是唯一一处需要"知道全部命令名"的地方 —— 漏注册一条，
/// 前端 `invoke` 就会收到 `command xxx not found`。
/// 纵切测试里专门有一条用例守着这个失败形态
/// （`golden_slice::ipc_rejects_unknown_command`）。
pub fn register_commands<R: tauri::Runtime>()
-> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        demo_item_create,
        demo_item_list,
        demo_item_get,
        demo_item_update,
        demo_item_delete,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_item(name: &str, quantity: i64, note: &str) -> NewDemoItem {
        NewDemoItem {
            name: name.to_owned(),
            quantity,
            note: note.to_owned(),
        }
    }

    // ---- normalize_new ----

    #[test]
    fn normalize_new_trims_whitespace() {
        let out = normalize_new(new_item("  写周报  ", 1, "  备注  ")).expect("合法输入");
        assert_eq!(out.name, "写周报");
        assert_eq!(out.note, "备注");
    }

    #[test]
    fn normalize_new_rejects_blank_name() {
        // 空串与"全是空格"必须同样被拒 —— 否则库里会多出一条查不到、删不掉的记录。
        for raw in ["", "   ", "\t\n"] {
            let err = normalize_new(new_item(raw, 0, "")).expect_err("空名字应被拒");
            assert_eq!(err.code(), "E_VALIDATION");
        }
    }

    #[test]
    fn normalize_new_counts_characters_not_bytes() {
        // 边界：正好 NAME_MAX 个中文字符应当通过（若按字节算会在这里误拒）。
        let ok = "中".repeat(NAME_MAX);
        assert!(normalize_new(new_item(&ok, 0, "")).is_ok());

        // 多一个字符就必须拒绝。
        let too_long = "中".repeat(NAME_MAX + 1);
        let err = normalize_new(new_item(&too_long, 0, "")).expect_err("超长应被拒");
        assert_eq!(err.code(), "E_VALIDATION");
    }

    #[test]
    fn normalize_new_rejects_out_of_range_quantity() {
        for bad in [-1, QUANTITY_MAX + 1] {
            let err = normalize_new(new_item("x", bad, "")).expect_err("越界数量应被拒");
            assert_eq!(err.code(), "E_VALIDATION");
        }
        // 两端闭区间必须放行。
        assert!(normalize_new(new_item("x", 0, "")).is_ok());
        assert!(normalize_new(new_item("x", QUANTITY_MAX, "")).is_ok());
    }

    // ---- normalize_patch ----

    #[test]
    fn normalize_patch_rejects_empty_patch() {
        let err = normalize_patch(DemoItemPatch::default()).expect_err("空 patch 应被拒");
        assert_eq!(err.code(), "E_VALIDATION");
    }

    #[test]
    fn normalize_patch_keeps_untouched_fields_as_none() {
        let patch = normalize_patch(DemoItemPatch {
            name: Some("  新名字 ".to_owned()),
            quantity: None,
            note: None,
        })
        .expect("合法 patch");
        assert_eq!(patch.name.as_deref(), Some("新名字"));
        // 关键语义：None 表示"不改"，规范化不能把它变成 Some(空串)。
        assert!(patch.quantity.is_none());
        assert!(patch.note.is_none());
        assert_eq!(patch.changed_fields(), vec!["name"]);
    }

    #[test]
    fn normalize_patch_rejects_blank_name_even_when_other_fields_present() {
        let err = normalize_patch(DemoItemPatch {
            name: Some("   ".to_owned()),
            quantity: Some(1),
            note: None,
        })
        .expect_err("把名字改成空白串应被拒");
        assert_eq!(err.code(), "E_VALIDATION");
    }

    // ---- clamp_page ----

    #[test]
    fn clamp_page_applies_defaults() {
        assert_eq!(clamp_page(None, None), (PAGE_DEFAULT, 0));
    }

    #[test]
    fn clamp_page_clamps_into_range() {
        // 上界夹紧 —— 这是防"一次拉全表"的那道闸。
        assert_eq!(clamp_page(Some(i64::MAX), None), (PAGE_MAX, 0));
        // 下界夹紧（0 / 负数页大小没有意义）。
        assert_eq!(clamp_page(Some(0), None), (1, 0));
        assert_eq!(clamp_page(Some(-5), None), (1, 0));
        // 负偏移无意义，归零。
        assert_eq!(clamp_page(None, Some(-100)), (PAGE_DEFAULT, 0));
        // 合法值原样透传。
        assert_eq!(clamp_page(Some(20), Some(40)), (20, 40));
    }
}
