//! 应用错误类型：每个变体带一个**稳定错误码**。
//!
//! 为什么要有错误码：前端要能对错误做分支处理（"没找到就刷新列表" vs
//! "数据库坏了就弹警告"）。如果前端只能匹配 `message` 文本，那么任何一次
//! 文案润色都会变成一次隐性破坏性变更 —— 文字是给人看的，不是给代码看的。

use serde::Serialize;

/// 命令可能返回的错误。
///
/// `thiserror` 只负责 `Display`（给人看），错误码由下面的 `code()` 给（给代码看）。
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// 打开数据库连接失败（路径不可写、目录不存在、文件被占用……）。
    #[error("打开数据库失败：{0}")]
    DbOpen(#[source] rusqlite::Error),

    /// 数据库读写失败。绝大多数持久层错误落在这里。
    #[error("数据库操作失败：{0}")]
    Db(#[from] rusqlite::Error),

    /// 目标记录不存在。**这是正常的业务结果，不是系统故障** ——
    /// 前端应当据此刷新列表，而不是弹「系统错误」。
    #[error("记录不存在：{entity} #{id}")]
    NotFound { entity: &'static str, id: i64 },

    /// 入参不合法（纯校验，不涉及 IO）。校验规则见 `normalize_*` 系列纯函数。
    #[error("输入不合法：{0}")]
    Validation(String),

    /// 内部不变量被破坏（当前只有 Mutex 中毒一种来源）。
    /// 归到独立变体是为了可观测：它的出现意味着**此前有线程 panic 过**。
    #[error("内部错误：{0}")]
    Internal(String),
}

impl AppError {
    /// 稳定错误码。改动这里的字符串等于破坏前端契约，需同步改前端。
    pub fn code(&self) -> &'static str {
        match self {
            Self::DbOpen(_) => "E_DB_OPEN",
            Self::Db(_) => "E_DB_QUERY",
            Self::NotFound { .. } => "E_NOT_FOUND",
            Self::Validation(_) => "E_VALIDATION",
            Self::Internal(_) => "E_INTERNAL",
        }
    }
}

/// 错误穿过 IPC 时的线上形态。
///
/// 只暴露 `code` + `message`：**绝不**把 `rusqlite::Error` 的 `Debug` 展开给前端
/// ——那里面会带文件路径，属于信息泄露（DESIGN.md §8.5）。
#[derive(Serialize)]
struct Wire<'a> {
    code: &'a str,
    message: String,
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Wire {
            code: self.code(),
            message: self.to_string(),
        }
        .serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_found_has_stable_code_and_readable_message() {
        let e = AppError::NotFound {
            entity: "demo_item",
            id: 42,
        };
        assert_eq!(e.code(), "E_NOT_FOUND");
        assert!(e.to_string().contains("42"));
    }

    #[test]
    fn serialized_error_carries_code_but_no_internals() {
        let e = AppError::Validation("name 不能为空".into());
        let json = serde_json::to_value(&e).expect("AppError 必须能序列化穿过 IPC");
        assert_eq!(json["code"], "E_VALIDATION");
        // 只允许 code + message 两个键 —— 多一个都可能是内部细节外泄。
        assert_eq!(json.as_object().expect("object").len(), 2);
        assert_eq!(json["message"], "输入不合法：name 不能为空");
    }
}
