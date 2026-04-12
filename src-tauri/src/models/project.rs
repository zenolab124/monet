use serde::Serialize;

use super::SessionSummary;

/// 项目：对应 ~/.claude/projects/ 下的一个子目录
#[derive(Debug, Clone, Serialize)]
pub struct Project {
    /// 编码后的目录名（如 -Users-xt-workspace）
    pub id: String,
    /// 解码后的可读路径
    pub display_path: String,
    pub sessions: Vec<SessionSummary>,
    pub session_count: usize,
    /// 最近活跃时间（秒级 Unix 时间戳）
    pub last_active: Option<f64>,
}
