//! Routine 运行标记（running marker）：跨进程的「正在执行」事实源。
//!
//! 执行方（主 App 立即运行 / 调度触发的 runner）spawn claude 后写入 marker，
//! 收尾时删除；主 App 据此展示运行状态并实现终止（stop_routine 杀 claude 进程组）。
//! 存活判定不用 PID 探测（有复用误判风险），用执行方持有的 fs2 文件锁佐证：
//! marker 在 + 锁被占 = 真在跑；锁空闲 = 执行方已死，marker 属陈旧残留。
//!
//! runner 经 #[path] 共享本文件，不得依赖 tauri / app_lib 其他模块。

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningMarker {
    /// claude 子进程 PID。unix 下 spawn 时 process_group(0) 自立进程组
    /// （组 ID = 该 PID），终止走组信号整树回收且不伤及执行方
    pub pid: u32,
    /// 开始时刻（RFC3339），供前端显示已耗时
    pub started_at: String,
    /// 执行方：manual（主 App 立即运行）| cron（调度触发 runner）
    pub source: String,
    /// 用户已发起终止：stop_routine 杀进程前置位，执行方收尾读取记入日志
    #[serde(default)]
    pub cancelled: bool,
}

pub fn running_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("routines").join("running")
}

fn marker_path(data_dir: &Path, routine_id: &str) -> PathBuf {
    running_dir(data_dir).join(format!("{}.json", routine_id))
}

/// 执行方互斥锁路径（与 runner 既有锁文件同一位置，单一事实源）
pub fn lock_path(data_dir: &Path, routine_id: &str) -> PathBuf {
    data_dir
        .join("routines")
        .join("locks")
        .join(format!("{}.lock", routine_id))
}

pub fn write_marker(data_dir: &Path, routine_id: &str, marker: &RunningMarker) {
    let path = marker_path(data_dir, routine_id);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let Ok(json) = serde_json::to_string_pretty(marker) else {
        return;
    };
    // 原子写：读方（get_routines / stop_routine）跨进程并发读，撕裂读会误判陈旧
    let tmp = path.with_extension(format!("json.tmp{}", std::process::id()));
    if fs::write(&tmp, json).is_ok() {
        let _ = fs::rename(&tmp, &path);
    }
}

pub fn read_marker(data_dir: &Path, routine_id: &str) -> Option<RunningMarker> {
    let content = fs::read_to_string(marker_path(data_dir, routine_id)).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn remove_marker(data_dir: &Path, routine_id: &str) {
    let _ = fs::remove_file(marker_path(data_dir, routine_id));
}

/// 置 cancelled=true，返回置位前读到的 marker（含待杀 PID）。
/// 与执行方收尾 remove 并发交错可能把 marker 短暂复活，遗留由
/// 陈旧清理（锁空闲判定）自愈，无需加锁。
pub fn mark_cancelled(data_dir: &Path, routine_id: &str) -> Option<RunningMarker> {
    let marker = read_marker(data_dir, routine_id)?;
    let mut updated = marker.clone();
    updated.cancelled = true;
    write_marker(data_dir, routine_id, &updated);
    Some(marker)
}

/// 执行方是否存活：试拿执行锁，拿不到 = 执行方仍持锁在跑。
/// 只在 marker 存在时调用——执行方先拿锁后写 marker，此时序保证
/// 本探测不会与执行方的抢锁窗口撞车（探测得手即真陈旧）。
pub fn executor_alive(data_dir: &Path, routine_id: &str) -> bool {
    use fs2::FileExt;
    let path = lock_path(data_dir, routine_id);
    let Ok(file) = fs::File::create(&path) else {
        return false;
    };
    // 拿到即放（drop 释放 flock），只做占用探测
    file.try_lock_exclusive().is_err()
}
