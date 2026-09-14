//! **唯一的日志出口**（九件套第 2 项）。
//!
//! 规矩只有两条，但都是硬的：
//!
//! 1. 应用代码里**不许出现** `println!` / `eprintln!` / `dbg!`。
//!    这条不是靠自觉，是靠 `lib.rs` 顶部的 `#![deny(clippy::print_stdout, ...)]`
//!    加上 CI 里的 `cargo clippy -- -D warnings` **机械拦住** —— 写下去就编译不过。
//! 2. 日志一律走 **stderr**。stdout 留给"程序结果"（本应用是 GUI，stdout 名义上为空，
//!    但保持这条纪律可以让将来加 CLI 子命令时不需要改日志配置）。
//!
//! 落地形态：控制台人类可读 + **文件按天滚动、JSON 结构化**（可直接被日志收集系统消费）。

use std::borrow::Cow;
use std::sync::OnceLock;

// `path()` 来自 Manager trait，必须引入作用域才能调。
use tauri::Manager as _;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// 日志文件名前缀。实际文件形如 `app.2026-09-10`。
const FILE_PREFIX: &str = "app";

/// 没有 RUST_LOG 时的默认级别。
/// 默认 `info` 而不是 `debug`：防止发布版意外刷出大量调试日志。
const DEFAULT_FILTER: &str = "info";

/// 认得出这些词就按敏感字段处理（大小写不敏感、允许作为子串出现）。
const SENSITIVE_KEY_HINTS: [&str; 6] = [
    "password",
    "passwd",
    "token",
    "secret",
    "authorization",
    "apikey",
];

/// 持有非阻塞写盘的 worker。
///
/// 必须活到进程结束 —— 一旦被 drop，日志文件的缓冲内容会丢失（这正是
/// `tracing_appender` 返回 guard 的原因）。放静态变量里最省心，
/// 也避免了"必须在 main 里一路传参"的丑陋管线。
static WORKER_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

/// 初始化全局日志。**必须在任何 tracing 宏调用之前执行**，且只应调用一次。
///
/// 之所以接收 `AppHandle` 而不是自己猜路径：日志要落在"应用自己的"目录里
/// （Windows 上是 `%LOCALAPPDATA%\<identifier>\logs`），而不是当前工作目录 ——
/// 否则从不同目录启动就会各自生成一份日志，排查时根本找不全。
pub fn init<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<(), Box<dyn std::error::Error>> {
    // 幂等：tracing 的全局 subscriber 只能设置一次，重复 init 会 panic。
    // 与其让调用方去记这条规则，不如在这里挡掉。
    if WORKER_GUARD.get().is_some() {
        return Ok(());
    }

    // Manager 提供 path()；AppHandle 自带 handle()。
    let log_dir = app.path().app_log_dir()?;
    std::fs::create_dir_all(&log_dir)?;

    let file_appender = tracing_appender::rolling::daily(&log_dir, FILE_PREFIX);
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
    // set() 失败只可能是"已经初始化过"，此时旧 guard 仍在，忽略即可。
    let _ = WORKER_GUARD.set(guard);

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));

    // 文件层：JSON + 无 ANSI 色。字段齐全（时间/级别/目标/span），
    // 这样日志系统可以直接按字段检索，不需要再写正则解析。
    let file_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_ansi(false)
        .with_writer(file_writer)
        .with_current_span(true)
        .with_span_list(true);

    // 控制台层：人类可读，只到 stderr。
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_target(false)
        .with_writer(std::io::stderr);

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stderr_layer)
        // try_init 而不是 init：init 在"已有全局 subscriber"时会 panic，
        // 而 panic 出现在启动路径上最难排查。这里把原因讲清楚。
        .try_init()
        .map_err(|e| format!("日志初始化失败（是否已有其它 subscriber？）：{e}"))?;

    tracing::info!(log_dir = %log_dir.display(), "logging.initialized");
    Ok(())
}

/// 把可能含敏感内容的值打码，保留首尾各 2 个字符用于比对。
///
/// 为什么保留首尾：排查时经常只需要确认"是不是同一个 token"
/// （`sk-ab***xy`），全量替换成 `***` 会让日志彻底失去诊断价值。
pub fn redact(raw: &str) -> String {
    let len = raw.chars().count();
    if len == 0 {
        return String::new();
    }
    if len <= 4 {
        // 太短的值保留任何字符都可能泄露，整体打码。
        return "*".repeat(len);
    }
    let head: String = raw.chars().take(2).collect();
    let tail: String = raw.chars().skip(len - 2).collect();
    format!("{head}***{tail}")
}

/// 字段名是否看起来像敏感字段。
pub fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SENSITIVE_KEY_HINTS.iter().any(|hint| lower.contains(hint))
}

/// 打点用：按字段名自动决定要不要打码。
///
/// 用法：`tracing::info!(user = %logging::field("user", &name), "…")`。
/// 返回 `Cow` 是为了非敏感字段零拷贝 —— 日志在热路径上，不该有额外分配。
pub fn field<'a>(key: &str, value: &'a str) -> Cow<'a, str> {
    if is_sensitive_key(key) {
        Cow::Owned(redact(value))
    } else {
        Cow::Borrowed(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_keeps_edges_and_hides_the_middle() {
        assert_eq!(redact("sk-abcdefghij"), "sk***ij");
    }

    #[test]
    fn redact_fully_masks_short_values() {
        // 短值保留任何字符都可能等于泄露，所以整体打码。
        assert_eq!(redact("abcd"), "****");
        assert_eq!(redact("ab"), "**");
        assert_eq!(redact(""), "");
    }

    #[test]
    fn redact_is_char_based_not_byte_based() {
        // 用中文验证：按 chars 计数，绝不能按字节切片（会 panic 或切出乱码）。
        assert_eq!(redact("密钥一二三四五六"), "密钥***五六");
    }

    #[test]
    fn sensitive_keys_are_detected_case_insensitively() {
        for key in [
            "password",
            "PASSWORD",
            "user_token",
            "X-Authorization",
            "apiKey",
        ] {
            assert!(is_sensitive_key(key), "{key} 应被判为敏感字段");
        }
        for key in ["name", "quantity", "note", "id"] {
            assert!(!is_sensitive_key(key), "{key} 不该被判为敏感字段");
        }
    }

    #[test]
    fn field_only_masks_sensitive_names() {
        // 非敏感字段必须零拷贝直通（Cow::Borrowed）。
        assert!(matches!(field("name", "hello"), Cow::Borrowed("hello")));
        // 敏感字段必须被替换成脱敏值。
        assert_eq!(field("token", "abcdefghij").as_ref(), "ab***ij");
    }
}
