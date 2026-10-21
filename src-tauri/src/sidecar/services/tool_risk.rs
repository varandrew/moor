//! 工具风险启发式。
//!
//! 下游协商的协议版本是 `2024-11-05`，该版本没有 `annotations`，所以拿不到
//! `destructiveHint`。第一版只能靠名字与描述里的动词推断，且必须允许用户覆盖误判。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    /// 只读类动词：get / list / search …
    Safe,
    /// 会改动状态但通常可恢复：create / update / send …
    Write,
    /// 可能造成不可逆后果：delete / drop / exec / publish …
    Destructive,
    /// 推断不出来。
    Unknown,
}

impl RiskLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Write => "write",
            Self::Destructive => "destructive",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "safe" => Self::Safe,
            "write" => Self::Write,
            "destructive" => Self::Destructive,
            _ => Self::Unknown,
        }
    }
}

const DESTRUCTIVE: &[&str] = &[
    "delete", "drop", "destroy", "purge", "truncate", "remove", "reset", "exec", "execute", "run",
    "shell", "publish", "deploy", "kill", "revoke", "rm",
];

const WRITE: &[&str] = &[
    "create", "update", "write", "set", "add", "insert", "edit", "patch", "upload", "move",
    "rename", "apply", "commit", "merge", "push", "send", "post", "put", "install", "save",
];

const SAFE: &[&str] = &[
    "get", "list", "read", "search", "find", "fetch", "query", "describe", "show", "view", "info",
    "status", "check", "lookup", "count", "inspect", "preview", "diff", "explain",
];

/// Profile 对风险工具的处置方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskPolicy {
    /// 只标记，不拦截。
    Advisory,
    /// 拒绝全部破坏性工具：不出现在目录，也不可调用。
    BlockDestructive,
}

impl RiskPolicy {
    pub fn from_db(value: &str) -> Self {
        match value {
            "block_destructive" => Self::BlockDestructive,
            _ => Self::Advisory,
        }
    }

    pub fn blocks(self, level: RiskLevel) -> bool {
        matches!(self, Self::BlockDestructive) && matches!(level, RiskLevel::Destructive)
    }
}

/// 按工具名与描述里的动词推断风险等级。破坏性优先于写入，写入优先于只读。
pub fn classify(tool_name: &str, description: Option<&str>) -> RiskLevel {
    let tokens = tokenize(tool_name);
    let description_tokens: Vec<String> = description.map(tokenize).unwrap_or_default();

    for level in [DESTRUCTIVE, WRITE, SAFE] {
        if tokens.iter().any(|token| level.contains(&token.as_str()))
            || description_tokens
                .iter()
                .any(|token| level.contains(&token.as_str()))
        {
            return match level {
                DESTRUCTIVE => RiskLevel::Destructive,
                WRITE => RiskLevel::Write,
                _ => RiskLevel::Safe,
            };
        }
    }
    RiskLevel::Unknown
}

fn tokenize(value: &str) -> Vec<String> {
    value
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_by_name_verbs() {
        let cases = [
            ("get_file", RiskLevel::Safe),
            ("search_repositories", RiskLevel::Safe),
            ("create_issue", RiskLevel::Write),
            ("update_settings", RiskLevel::Write),
            ("delete_repository", RiskLevel::Destructive),
            ("run_command", RiskLevel::Destructive),
            ("echo", RiskLevel::Unknown),
        ];

        for (name, expected) in cases {
            assert_eq!(classify(name, None), expected, "tool {name}");
        }
    }

    #[test]
    fn destructive_wins_over_write_and_safe() {
        assert_eq!(
            classify("get_then_delete", None),
            RiskLevel::Destructive,
            "a combined verb must not be downgraded"
        );
    }

    #[test]
    fn falls_back_to_the_description() {
        assert_eq!(
            classify("gh", Some("Delete a repository permanently")),
            RiskLevel::Destructive
        );
    }

    #[test]
    fn ignores_substrings_inside_words() {
        assert_eq!(
            classify("unsettled", None),
            RiskLevel::Unknown,
            "tokens match whole words, not substrings"
        );
    }
}
