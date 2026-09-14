/**
 * 前端**唯一**的 IPC 出口。
 *
 * 为什么必须收敛成一个文件：`invoke('命令名', 参数)` 里的命令名是**字符串**，
 * 拼错不会编译报错，只会在运行时收到 "command not found"。
 * 集中在这里之后，全前端只有这一个地方能把命令名写错，
 * 而 `api.test.ts` 正好钉住了这一处。
 *
 * 组件里禁止直接 `import { invoke } from '@tauri-apps/api/core'`。
 */

import { invoke } from '@tauri-apps/api/core'

import {
  COMMANDS,
  DEMO_ITEM_LIMITS,
  type AppErrorPayload,
  type DemoItem,
  type DemoItemPatch,
  type NewDemoItem,
  type Page,
} from '../types/demo_item'

/**
 * 判断一个 unknown 是不是我们约定的错误载荷。
 *
 * Rust 侧 `AppError` 的 Serialize 实现保证形状是 `{code, message}`；
 * 但 invoke 也可能抛出完全不是这个形状的东西（比如 IPC 层自己的错误），
 * 所以必须真的检查一次，而不是直接 `as`。
 */
export function isAppErrorPayload(value: unknown): value is AppErrorPayload {
  if (typeof value !== 'object' || value === null) return false
  const candidate = value as Record<string, unknown>
  return typeof candidate.code === 'string' && typeof candidate.message === 'string'
}

/**
 * 把任意抛出物转成一句能给用户看的话。
 *
 * 纯函数，因此可以脱离 Tauri 运行时做单元测试
 * （`invoke` 会抛什么形状的东西，正是最容易写错的部分）。
 */
export function toUserMessage(error: unknown): string {
  if (isAppErrorPayload(error)) {
    // 已知错误码 → 给一句人话。**不在 UI 里做分支逻辑**，
    // 需要分支的地方请用 error.code。
    switch (error.code) {
      case 'E_NOT_FOUND':
        return '这条记录已经不存在了，可能已被删除。请刷新列表。'
      case 'E_VALIDATION':
        return `输入不合法：${error.message}`
      case 'E_DB_OPEN':
      case 'E_DB_QUERY':
        return '本地数据读写失败，请查看日志；若持续出现可尝试重启应用。'
      case 'E_INTERNAL':
        return '应用内部错误，请重启后再试。'
    }
  }
  if (error instanceof Error) return error.message
  if (typeof error === 'string') return error
  return '发生了未知错误。'
}

/** 条目接口。全部返回 Promise，错误按上表形状抛出。 */
export const demoItemApi = {
  create(input: NewDemoItem): Promise<DemoItem> {
    // 参数必须包在 `input` 里 → 对应 Rust 侧 `input: NewDemoItem`。
    return invoke<DemoItem>(COMMANDS.create, { input })
  },

  list(limit: number = DEMO_ITEM_LIMITS.pageDefault, offset = 0): Promise<Page<DemoItem>> {
    // 把夹紧规则也在前端做一遍：不是为了安全（后端才是裁判），
    // 而是为了**少一次必然失败的往返**。
    const safeLimit = Math.min(Math.max(limit, 1), DEMO_ITEM_LIMITS.pageMax)
    const safeOffset = Math.max(offset, 0)
    return invoke<Page<DemoItem>>(COMMANDS.list, { limit: safeLimit, offset: safeOffset })
  },

  get(id: number): Promise<DemoItem> {
    return invoke<DemoItem>(COMMANDS.get, { id })
  },

  update(id: number, patch: DemoItemPatch): Promise<DemoItem> {
    return invoke<DemoItem>(COMMANDS.update, { id, patch })
  },

  remove(id: number): Promise<void> {
    // Rust 侧返回 `Result<(), AppError>`，线上是 `null`。
    return invoke<null>(COMMANDS.remove, { id }).then(() => undefined)
  },
}
