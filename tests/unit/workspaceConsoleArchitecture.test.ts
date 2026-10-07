import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'

const workspace = readFileSync('src-tauri/src/workspace.rs', 'utf8')
const gitOutput = workspace.match(/fn git_output\([\s\S]*?\n\}/)?.[0] ?? ''

describe('workspace Git subprocesses', () => {
  it('hides the console before spawning background workspace probes on Windows', () => {
    expect(workspace).toContain('use crate::proc_ext::HideConsole;')
    expect(gitOutput).toMatch(/Command::new\("git"\)[\s\S]*\.hide_console\(\)\s*\.output\(\)/)
  })

  it('preserves the enhanced PATH and Git failure handling', () => {
    expect(gitOutput).toContain('.env("PATH", crate::path_env::enhanced_path())')
    expect(gitOutput).toContain('output.status.success().then_some(output.stdout)')
  })
})
