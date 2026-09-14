/**
 * 前端契约测试：**钉住命令名与参数形状**。
 *
 * 这不能替代 `src-tauri/tests/golden_slice.rs`（那边是真后端、真 IPC 派发）。
 * 它的职责是互补的另一半：Rust 侧的测试证明了"命令存在且行为对"，
 * 但证明不了"前端调用时参数包对了没有" —— 参数名写错（比如把 `input`
 * 写成 `item`）在后端测试里永远不会暴露。
 *
 * 用 `mockIPC` 而不是真实后端：这一步要能在纯 Node 环境里毫秒级跑完，
 * 好让它在每次保存时都跑。真后端那部分交给 golden_slice。
 */

import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { COMMANDS } from '../types/demo_item'
import { demoItemApi, isAppErrorPayload, toUserMessage } from './api'

/** 捕获到的调用，用于断言"前端到底发了什么"。 */
interface Captured {
  cmd: string
  args: Record<string, unknown>
}

let captured: Captured[] = []

function stubIPC(handler: (cmd: string) => unknown): void {
  captured = []
  mockIPC((cmd, args) => {
    captured.push({ cmd, args: (args ?? {}) as Record<string, unknown> })
    return handler(cmd)
  })
}

beforeEach(() => {
  captured = []
})

afterEach(() => {
  clearMocks()
})

describe('demoItemApi 发出的命令名与参数形状', () => {
  it('create 把入参包在 input 里（对应 Rust 侧 input: NewDemoItem）', async () => {
    stubIPC(() => ({
      id: 1,
      name: '写周报',
      quantity: 3,
      note: '',
      createdAt: '2026-09-10 12:00:00',
      updatedAt: '2026-09-10 12:00:00',
    }))

    await demoItemApi.create({ name: '写周报', quantity: 3 })

    expect(captured).toHaveLength(1)
    expect(captured[0]?.cmd).toBe('demo_item_create')
    expect(captured[0]?.args).toEqual({ input: { name: '写周报', quantity: 3 } })
  })

  it('list 同时传 limit 与 offset（Rust 侧两者都是 Option<i64>）', async () => {
    stubIPC(() => ({ items: [], total: 0, limit: 20, offset: 40 }))

    await demoItemApi.list(20, 40)

    expect(captured[0]?.cmd).toBe('demo_item_list')
    expect(captured[0]?.args).toEqual({ limit: 20, offset: 40 })
  })

  it('list 在前端就夹紧非法分页参数，避免必然失败的往返', async () => {
    stubIPC(() => ({ items: [], total: 0, limit: 0, offset: 0 }))

    // 超出上限 → 夹到 pageMax；负数偏移 → 归零。
    await demoItemApi.list(99999, -5)

    expect(captured[0]?.args).toEqual({ limit: 200, offset: 0 })
  })

  it('get / update / remove 的参数名与 Rust 侧拼写一致', async () => {
    stubIPC(() => ({ id: 7, name: 'x', quantity: 1, note: '', createdAt: '', updatedAt: '' }))

    await demoItemApi.get(7)
    expect(captured[0]?.cmd).toBe('demo_item_get')
    expect(captured[0]?.args).toEqual({ id: 7 })

    await demoItemApi.update(7, { quantity: 2 })
    expect(captured[1]?.cmd).toBe('demo_item_update')
    // patch 必须再包一层 —— 对应 Rust 侧 patch: DemoItemPatch。
    expect(captured[1]?.args).toEqual({ id: 7, patch: { quantity: 2 } })

    stubIPC(() => null)
    await demoItemApi.remove(7)
    expect(captured[0]?.cmd).toBe('demo_item_delete')
    expect(captured[0]?.args).toEqual({ id: 7 })
  })

  it('命令名常量表与调用点完全对应（防止改了常量表却漏改调用）', async () => {
    stubIPC(() => null)

    await demoItemApi.remove(1)

    // COMMANDS.remove 就是实际发出去的那个字符串。
    expect(captured[0]?.cmd).toBe(COMMANDS.remove)
  })
})

describe('错误载荷', () => {
  it('后端的 AppError 形状被正确识别', () => {
    expect(isAppErrorPayload({ code: 'E_NOT_FOUND', message: '记录不存在' })).toBe(true)
    // 缺字段、类型不对、根本不是对象 —— 全都要被挡住。
    expect(isAppErrorPayload({ code: 'E_NOT_FOUND' })).toBe(false)
    expect(isAppErrorPayload({ code: 1, message: 'x' })).toBe(false)
    expect(isAppErrorPayload('boom')).toBe(false)
    expect(isAppErrorPayload(null)).toBe(false)
  })

  it('已知错误码被翻成人话，且按 code 分支而不是按文案', () => {
    expect(toUserMessage({ code: 'E_NOT_FOUND', message: '记录不存在：demo_item #9' })).toContain(
      '已被删除',
    )
    expect(toUserMessage({ code: 'E_VALIDATION', message: 'name 不能为空' })).toContain(
      'name 不能为空',
    )
    expect(toUserMessage({ code: 'E_INTERNAL', message: '锁中毒' })).toContain('重启')
  })

  it('非约定形状的抛出物也有兜底文案（绝不能把 undefined 显示给用户）', () => {
    expect(toUserMessage(new Error('网络断了'))).toBe('网络断了')
    expect(toUserMessage('纯字符串错误')).toBe('纯字符串错误')
    expect(toUserMessage(undefined)).toBe('发生了未知错误。')
    expect(toUserMessage({ weird: true })).toBe('发生了未知错误。')
  })

  it('后端返回错误时，promise 以原始载荷 reject（不吞掉 code）', async () => {
    stubIPC(() => {
      throw { code: 'E_NOT_FOUND', message: '记录不存在：demo_item #999' }
    })

    await expect(demoItemApi.get(999)).rejects.toEqual({
      code: 'E_NOT_FOUND',
      message: '记录不存在：demo_item #999',
    })
  })
})
