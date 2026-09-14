/**
 * 日志门面的单元测试。
 *
 * 这里**不 spy console**，而是注入自己的 sink：断言"门面交给落地层的是什么"，
 * 而不是"console 被怎么调的"。前者是行为契约，后者是实现细节 ——
 * 后者会因为将来换落地方式（比如改成转发到后端）而全部失效。
 */

import { describe, expect, it } from 'vitest'

import {
  createLogger,
  isSensitiveKey,
  redact,
  sanitizeFields,
  type LogRecord,
} from './logger'

/** 收集所有落到 sink 的记录。 */
function collector(): { records: LogRecord[]; sink: (r: LogRecord) => void } {
  const records: LogRecord[] = []
  return { records, sink: (r) => records.push(r) }
}

describe('redact', () => {
  it('保留首尾各两个字符，遮住中间', () => {
    expect(redact('sk-abcdefghij')).toBe('sk***ij')
  })

  it('短值整体打码 —— 保留任何一个字符都可能等于泄露', () => {
    expect(redact('abcd')).toBe('****')
    expect(redact('ab')).toBe('**')
    expect(redact('')).toBe('')
  })

  it('按码点而不是 UTF-16 码元切分，中文与 emoji 都不会被切碎', () => {
    expect(redact('密钥一二三四五六')).toBe('密钥***五六')
    // emoji 是代理对：若用 slice 会切出半个字符，显示成乱码。
    expect(redact('🔑🔑abcd🔑🔑')).toBe('🔑🔑***🔑🔑')
  })
})

describe('isSensitiveKey', () => {
  it('大小写不敏感，且允许作为子串命中', () => {
    for (const key of ['password', 'PASSWORD', 'user_token', 'X-Authorization', 'apiKey']) {
      expect(isSensitiveKey(key), `${key} 应被判为敏感字段`).toBe(true)
    }
  })

  it('普通字段名不该被误判', () => {
    for (const key of ['name', 'quantity', 'note', 'id', 'createdAt']) {
      expect(isSensitiveKey(key), `${key} 不该被判为敏感字段`).toBe(false)
    }
  })
})

describe('sanitizeFields', () => {
  it('只脱敏敏感字段的字符串值', () => {
    const out = sanitizeFields({
      name: '普通名字',
      token: 'sk-abcdefghij',
      quantity: 3,
    })
    expect(out).toEqual({
      name: '普通名字',
      token: 'sk***ij',
      quantity: 3,
    })
  })

  it('字段名敏感但不是字符串时不做处理（数字不可能是密钥）', () => {
    expect(sanitizeFields({ token: 12345 })).toEqual({ token: 12345 })
  })

  it('undefined 原样返回 undefined，而不是空对象', () => {
    expect(sanitizeFields(undefined)).toBeUndefined()
  })
})

describe('createLogger 的级别过滤', () => {
  it('低于阈值的级别被完全丢弃（连 sink 都不会被调）', () => {
    const { records, sink } = collector()
    const log = createLogger('test', sink, 'warn')

    log.debug('不该出现')
    log.info('也不该出现')
    log.warn('这条要留下')
    log.error('这条也要留下')

    expect(records.map((r) => r.level)).toEqual(['warn', 'error'])
  })

  it('阈值取 debug 时全部放行，且记录带上 scope 与时间', () => {
    const { records, sink } = collector()
    const log = createLogger('demo-item', sink, 'debug')

    log.debug('开始加载')

    expect(records).toHaveLength(1)
    expect(records[0]?.level).toBe('debug')
    expect(records[0]?.scope).toBe('demo-item')
    expect(records[0]?.message).toBe('开始加载')
    // ISO 8601，能被日志系统直接解析。
    expect(records[0]?.time).toMatch(/^\d{4}-\d{2}-\d{2}T/)
  })

  it('敏感字段在到达 sink 之前就已经被脱敏 —— 落地层拿不到原文', () => {
    const { records, sink } = collector()
    const log = createLogger('test', sink, 'debug')

    log.info('调用外部接口', { url: 'https://example.com', authorization: 'Bearer abcdefghij' })

    expect(records[0]?.fields?.url).toBe('https://example.com')
    expect(records[0]?.fields?.authorization).toBe('Be***ij')
  })
})
