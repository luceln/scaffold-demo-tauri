/**
 * **前端唯一的日志出口**（九件套第 2 项的前端侧）。
 *
 * 规矩：业务代码里不许出现 `console.log` / `console.error`。
 * 这条不是靠自觉 —— `eslint.config.mjs` 里配了 `no-console: 'error'`，
 * 而**本文件是唯一的豁免**。想打日志，就 import 这个模块。
 *
 * 与 Rust 侧的关系：`src-tauri/src/logging.rs` 是后端门面，本文件是前端门面。
 * 两者**刻意不合并**成一条链路（那需要引入 tauri-plugin-log + 额外 capabilities
 * 权限，与"最小权限"基线冲突）。前端日志当前落在 webview 控制台；
 * 需要统一落盘时，做法见项目 README 的「日志」一节。
 */

/** 日志级别。数值越大越严重，用于阈值过滤。 */
export type LogLevel = 'debug' | 'info' | 'warn' | 'error'

const LEVEL_ORDER: Record<LogLevel, number> = {
  debug: 10,
  info: 20,
  warn: 30,
  error: 40,
}

/** 认得出这些词就按敏感字段处理（与 Rust 侧 SENSITIVE_KEY_HINTS 对齐）。 */
const SENSITIVE_KEY_HINTS = ['password', 'passwd', 'token', 'secret', 'authorization', 'apikey']

/** 一条日志记录。结构化字段是关键：日志系统按字段检索，不靠正则解析文本。 */
export interface LogRecord {
  time: string
  level: LogLevel
  scope: string
  message: string
  fields?: Record<string, unknown>
}

/** 日志的落地方式。抽成函数是为了**让级别过滤与脱敏能被单元测试**。 */
export type LogSink = (record: LogRecord) => void

/**
 * 把可能含敏感内容的值打码，保留首尾各 2 个字符。
 *
 * 保留首尾是刻意的：排查时常常只需要确认"是不是同一个 token"
 * （`sk-ab***xy`），全部变成 `***` 会让日志失去诊断价值。
 */
export function redact(raw: string): string {
  // 用展开运算符按**码点**切分，而不是 str.slice —— 后者按 UTF-16 码元切，
  // 遇到 emoji 或部分汉字会切出半个字符（显示成乱码）。
  const chars = [...raw]
  if (chars.length === 0) return ''
  if (chars.length <= 4) return '*'.repeat(chars.length)
  return `${chars.slice(0, 2).join('')}***${chars.slice(-2).join('')}`
}

/** 字段名是否看起来像敏感字段（大小写不敏感，允许子串命中）。 */
export function isSensitiveKey(key: string): boolean {
  const lower = key.toLowerCase()
  return SENSITIVE_KEY_HINTS.some((hint) => lower.includes(hint))
}

/**
 * 按字段名决定是否脱敏。
 *
 * 注意：只处理字符串值。数字/布尔不可能承载密钥，
 * 而对对象递归脱敏会让日志变得难读且容易踩循环引用。
 */
export function sanitizeFields(
  fields: Record<string, unknown> | undefined,
): Record<string, unknown> | undefined {
  if (!fields) return undefined
  const out: Record<string, unknown> = {}
  for (const [key, value] of Object.entries(fields)) {
    out[key] = isSensitiveKey(key) && typeof value === 'string' ? redact(value) : value
  }
  return out
}

/** 默认落地方式：写到 webview 控制台，按级别选对应的方法。 */
const consoleSink: LogSink = (record) => {
  const prefix = `[${record.scope}] ${record.message}`
  const method =
    record.level === 'error'
      ? console.error
      : record.level === 'warn'
        ? console.warn
        : record.level === 'debug'
          ? console.debug
          : console.info

  if (record.fields && Object.keys(record.fields).length > 0) {
    method(prefix, record.fields)
  } else {
    method(prefix)
  }
}

/**
 * 开发环境默认 `debug`，生产默认 `info`。
 *
 * 生产不打 debug 是有意的：桌面应用没有集中的日志收集，
 * 刷屏只会让用户截图求助时截到一屏噪音。
 */
function defaultLevel(): LogLevel {
  return import.meta.env.DEV ? 'debug' : 'info'
}

/**
 * 造一个带 scope 的 logger。
 *
 * @param scope 模块名，出现在每条日志的行首（便于 `grep` 一个模块）。
 * @param sink 落地方式。测试传自己的 sink 来断言**收到了什么**，
 *             而不是去 spy 全局 console。
 * @param minLevel 低于它的级别直接丢弃，不产生任何开销。
 */
export function createLogger(
  scope: string,
  sink: LogSink = consoleSink,
  minLevel: LogLevel = defaultLevel(),
) {
  const threshold = LEVEL_ORDER[minLevel]

  const emit = (level: LogLevel, message: string, fields?: Record<string, unknown>): void => {
    if (LEVEL_ORDER[level] < threshold) return
    sink({
      time: new Date().toISOString(),
      level,
      scope,
      message,
      fields: sanitizeFields(fields),
    })
  }

  return {
    debug: (message: string, fields?: Record<string, unknown>) => emit('debug', message, fields),
    info: (message: string, fields?: Record<string, unknown>) => emit('info', message, fields),
    warn: (message: string, fields?: Record<string, unknown>) => emit('warn', message, fields),
    error: (message: string, fields?: Record<string, unknown>) => emit('error', message, fields),
  }
}

/** 应用级 logger。业务模块建议自建 `createLogger('模块名')`。 */
export const logger = createLogger('app')
