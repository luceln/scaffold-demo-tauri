/**
 * 前后端共享的 IPC 契约（前端侧）。
 *
 * ⚠️ 这个文件是 Rust 侧 `src-tauri/src/demo_item.rs` 里 DTO 的**镜像**。
 * 改一边必须改另一边 —— 这正是 `src/lib/api.test.ts` 与
 * `src-tauri/tests/golden_slice.rs` 各自守住一端的原因：
 * 前者钉住"命令名与参数形状"，后者钉住"命令真的存在且行为正确"。
 *
 * 字段名用 camelCase：Rust 侧有 `#[serde(rename_all = "camelCase")]`，
 * 所以 `created_at` 在线上就叫 `createdAt`。
 */

/** 一个条目。 */
export interface DemoItem {
  id: number
  name: string
  quantity: number
  note: string
  /** ISO 字符串，由数据库的 datetime('now') 产生（UTC）。 */
  createdAt: string
  updatedAt: string
}

/** 新建入参。`quantity` / `note` 可省略（Rust 侧有 #[serde(default)]）。 */
export interface NewDemoItem {
  name: string
  quantity?: number
  note?: string
}

/**
 * 部分更新入参。
 *
 * 语义是"只改传了的字段"：`undefined` 表示不改，**不是**改成空。
 * 对应 SQL 里的 `COALESCE(?n, 原值)`。
 */
export interface DemoItemPatch {
  name?: string
  quantity?: number
  note?: string
}

/** 分页结果。 */
export interface Page<T> {
  items: T[]
  total: number
  limit: number
  offset: number
}

/**
 * Rust 侧 `AppError::code()` 的全部取值。
 *
 * 用它做分支判断，**绝不要**去匹配 message 文本 —— 文案是给人看的，
 * 改文案不该让前端逻辑失效。
 */
export type AppErrorCode =
  | 'E_DB_OPEN'
  | 'E_DB_QUERY'
  | 'E_NOT_FOUND'
  | 'E_VALIDATION'
  | 'E_INTERNAL'

/** 错误穿过 IPC 时的形状（Rust 侧 `AppError` 的 Serialize 实现）。 */
export interface AppErrorPayload {
  code: AppErrorCode
  message: string
}

/**
 * 命令名常量表。
 *
 * 集中在这里而不是散落在组件里：命令名一旦写错，报的是运行时的
 * "command not found"，没有任何编译期保护。收敛成一个常量对象，
 * 至少让"改名字"变成一处修改。
 */
export const COMMANDS = {
  create: 'demo_item_create',
  list: 'demo_item_list',
  get: 'demo_item_get',
  update: 'demo_item_update',
  remove: 'demo_item_delete',
} as const

/**
 * 校验限额，与 Rust 侧 `demo_item.rs` 的常量保持一致。
 *
 * 唯一事实源在 Rust 那边（服务端永远是最终裁判）；这里复制一份是为了
 * 让 UI 能在**不发请求**的情况下给出 `maxlength` 与即时提示。
 * 即便这里写错，用户也只会看到"填完才被拒"，不会写进非法数据。
 */
export const DEMO_ITEM_LIMITS = {
  nameMax: 64,
  noteMax: 512,
  quantityMax: 1_000_000,
  pageDefault: 50,
  pageMax: 200,
} as const
