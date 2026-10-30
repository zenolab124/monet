//! Windows 远端的 PowerShell 5.1 脚本：与 POSIX 版输出格式一致，上层同步逻辑共用。
//!
//! 每次远端命令都要冷启动一次 PowerShell（0.3～1 秒），拉取改为一个进程处理一批文件：
//! 每个文件先输出一行 `MONET_FILE <字节数>`（不存在为 `-1`），紧跟原始字节。

use super::executor::ps_quote;

/// 批量拉取的帧头
pub const FRAME_MARKER: &[u8] = b"MONET_FILE ";

/// 写一行 UTF-8 文本到标准输出（不经 PowerShell 的格式化与折行）。
/// 辅助函数一律用动词-名词长名：单字母名会被内置别名（如 `H` = Get-History）抢先解析
const WRITE_LINE: &str = "function Out-MonetLine($s) { [Console]::Out.WriteLine($s) }\n";

/// 文件的 `大小 修改时间(秒)` 前缀，与 POSIX `stat` 输出对齐
const STAT_FN: &str =
    "function Get-MonetStat($i) { '{0} {1}' -f $i.Length, ([DateTimeOffset]$i.LastWriteTimeUtc).ToUnixTimeSeconds() }\n";

fn windows_rel(rel: &str) -> String {
    rel.replace('/', "\\")
}

fn projects_dir(root: &str) -> String {
    format!("(Join-Path {} 'projects')", ps_quote(root))
}

/// 列目录：每行 `大小 修改时间 ./项目目录/会话.jsonl`；数据目录缺失时退出码 3
pub fn list_script(root: &str) -> String {
    format!(
        r#"{WRITE_LINE}{STAT_FN}$p = {projects}
if (-not (Test-Path -LiteralPath $p -PathType Container)) {{ exit 3 }}
foreach ($d in Get-ChildItem -LiteralPath $p -Directory -Force) {{
  foreach ($f in Get-ChildItem -LiteralPath $d.FullName -File -Filter '*.jsonl' -Force) {{
    Out-MonetLine ((Get-MonetStat $f) + ' ./' + $d.Name + '/' + $f.Name)
  }}
}}
exit 0
"#,
        projects = projects_dir(root),
    )
}

/// 只列一个文件（跟随用）；文件不存在时输出为空
pub fn stat_script(root: &str, rel: &str) -> String {
    format!(
        r#"{WRITE_LINE}{STAT_FN}$p = {projects}
if (-not (Test-Path -LiteralPath $p -PathType Container)) {{ exit 3 }}
$f = Join-Path $p {file}
if (Test-Path -LiteralPath $f -PathType Leaf) {{ Out-MonetLine ((Get-MonetStat (Get-Item -LiteralPath $f -Force)) + ' ./' + {rel}) }}
exit 0
"#,
        projects = projects_dir(root),
        file = ps_quote(&windows_rel(rel)),
        rel = ps_quote(rel),
    )
}

/// 列一个会话的子 Agent 文件，路径相对 `subagents/` 并以 `/` 分隔；目录不存在时输出为空
pub fn subagent_list_script(root: &str, session_dir: &str) -> String {
    format!(
        r#"{WRITE_LINE}{STAT_FN}$b = Join-Path {projects} {dir}
if (-not (Test-Path -LiteralPath $b -PathType Container)) {{ exit 0 }}
$n = (Get-Item -LiteralPath $b -Force).FullName.TrimEnd('\').Length + 1
foreach ($f in Get-ChildItem -LiteralPath $b -File -Recurse -Depth 2 -Force) {{
  if ($f.Extension -eq '.jsonl' -or $f.Extension -eq '.json') {{
    Out-MonetLine ((Get-MonetStat $f) + ' ./' + $f.FullName.Substring($n).Replace('\', '/'))
  }}
}}
exit 0
"#,
        projects = projects_dir(root),
        dir = ps_quote(&windows_rel(&format!("{session_dir}/subagents"))),
    )
}

/// 批量拉取：按顺序输出每个文件从 offset 起到当前末尾的字节。
/// 以共享读写方式打开，不打断 CLI 正在进行的追加写入
pub fn fetch_batch_script(root: &str, items: &[(&str, u64)]) -> String {
    // 路径与偏移分成两个平行数组：PowerShell 会把只有一项的 `@(@(...))` 展开
    let rels: Vec<String> = items
        .iter()
        .map(|(rel, _)| ps_quote(&windows_rel(rel)))
        .collect();
    let offsets: Vec<String> = items.iter().map(|(_, offset)| offset.to_string()).collect();
    format!(
        r#"$o = [Console]::OpenStandardOutput()
function Out-MonetFrame($t) {{ $h = [Text.Encoding]::ASCII.GetBytes($t + "`n"); $o.Write($h, 0, $h.Length) }}
$p = {projects}
$r = @({rels})
$f = @({offsets})
for ($i = 0; $i -lt $r.Count; $i++) {{
  try {{ $s = [IO.File]::Open((Join-Path $p $r[$i]), 'Open', 'Read', 'ReadWrite, Delete') }} catch {{ Out-MonetFrame 'MONET_FILE -1'; continue }}
  try {{
    $len = [Math]::Max([long]0, $s.Length - [long]$f[$i])
    [void]$s.Seek([long]$f[$i], 'Begin')
    $buf = New-Object byte[] $len
    $n = 0
    while ($n -lt $len) {{ $c = $s.Read($buf, $n, $len - $n); if ($c -le 0) {{ break }}; $n += $c }}
    Out-MonetFrame ('MONET_FILE ' + $n)
    $o.Write($buf, 0, $n)
  }} finally {{ $s.Dispose() }}
}}
$o.Flush()
exit 0
"#,
        projects = projects_dir(root),
        rels = rels.join(", "),
        offsets = offsets.join(", "),
    )
}

/// 解析批量拉取的输出：与请求顺序一一对应，`None` 表示远端没有这个文件。
/// 帧数不足或格式不对时报错（整批按失败处理，下一轮重拉）
pub fn parse_batch(stdout: &[u8], expected: usize) -> Result<Vec<Option<Vec<u8>>>, String> {
    let mut rest = stdout;
    let mut out = Vec::with_capacity(expected);
    while out.len() < expected {
        let header_end = rest
            .iter()
            .position(|b| *b == b'\n')
            .ok_or("batch_truncated")?;
        let header = rest[..header_end]
            .strip_prefix(FRAME_MARKER)
            .ok_or("batch_bad_frame")?;
        let header = std::str::from_utf8(header)
            .map_err(|_| "batch_bad_frame")?
            .trim();
        rest = &rest[header_end + 1..];
        if header == "-1" {
            out.push(None);
            continue;
        }
        let len: usize = header.parse().map_err(|_| "batch_bad_frame")?;
        if rest.len() < len {
            return Err("batch_truncated".into());
        }
        out.push(Some(rest[..len].to_vec()));
        rest = &rest[len..];
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engines::claude::remote::executor::fits_windows_command;

    #[test]
    fn batch_frames_round_trip_including_missing_files() {
        let mut stdout = b"MONET_FILE 6\nabc\nd\n".to_vec();
        stdout.extend_from_slice(b"MONET_FILE -1\n");
        stdout.extend_from_slice(b"MONET_FILE 0\n");
        stdout.extend_from_slice(b"MONET_FILE 3\n\xe4\xb8\xad");
        let frames = parse_batch(&stdout, 4).unwrap();
        assert_eq!(frames[0].as_deref(), Some(&b"abc\nd\n"[..]));
        assert_eq!(frames[1], None);
        assert_eq!(frames[2].as_deref(), Some(&b""[..]));
        assert_eq!(frames[3].as_deref(), Some("中".as_bytes()));
    }

    #[test]
    fn truncated_batches_fail_as_a_whole() {
        assert!(parse_batch(b"MONET_FILE 10\nabc", 1).is_err());
        assert!(parse_batch(b"MONET_FILE 1\na", 2).is_err());
        assert!(parse_batch(b"garbage\n", 1).is_err());
    }

    #[test]
    fn scripts_quote_paths_as_powershell_literals() {
        let script = stat_script(r"C:\Users\it's\.claude", "-C--work/s-1.jsonl");
        assert!(script.contains(r"'C:\Users\it''s\.claude'"));
        assert!(script.contains(r"'-C--work\s-1.jsonl'"));
        assert!(script.contains("' ./' + '-C--work/s-1.jsonl'"));
        let fetch = fetch_batch_script(r"C:\r", &[("p/a.jsonl", 0), ("p/b.jsonl", 42)]);
        assert!(fetch.contains(r"$r = @('p\a.jsonl', 'p\b.jsonl')"));
        assert!(fetch.contains("$f = @(0, 42)"));
    }

    #[test]
    fn a_typical_batch_fits_the_windows_command_line() {
        let rel =
            "C--Users-someone-workspace-project-name/0ebe7955-ab04-4f5b-b5b9-8362886222aa.jsonl";
        let items: Vec<(&str, u64)> = (0..20).map(|i| (rel, i)).collect();
        assert!(fits_windows_command(&fetch_batch_script(
            r"C:\Users\someone\.claude",
            &items
        )));
        assert!(fits_windows_command(&list_script(
            r"C:\Users\someone\.claude"
        )));
    }
}
