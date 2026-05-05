import type { Component } from 'vue'
import ToolRead from './ToolRead.vue'
import ToolEdit from './ToolEdit.vue'
import ToolWrite from './ToolWrite.vue'
import ToolNotebookEdit from './ToolNotebookEdit.vue'
import ToolBash from './ToolBash.vue'
import ToolGrep from './ToolGrep.vue'
import ToolGlob from './ToolGlob.vue'
import ToolWebFetch from './ToolWebFetch.vue'
import ToolWebSearch from './ToolWebSearch.vue'
import ToolTodoWrite from './ToolTodoWrite.vue'
import ToolTask from './ToolTask.vue'
import ToolExitPlanMode from './ToolExitPlanMode.vue'
import ToolSkill from './ToolSkill.vue'
import ToolMcp from './ToolMcp.vue'
import ToolGeneric from './ToolGeneric.vue'

export const TOOL_MAP: Record<string, Component> = {
  Read: ToolRead, Edit: ToolEdit, Write: ToolWrite, NotebookEdit: ToolNotebookEdit,
  Bash: ToolBash,
  Grep: ToolGrep, Glob: ToolGlob,
  WebFetch: ToolWebFetch, WebSearch: ToolWebSearch,
  TodoWrite: ToolTodoWrite, Task: ToolTask,
  ExitPlanMode: ToolExitPlanMode, Skill: ToolSkill,
}

export { ToolMcp, ToolGeneric }

export function resolveTool(name: string): Component {
  if (name in TOOL_MAP) return TOOL_MAP[name]
  if (name.startsWith('mcp__')) return ToolMcp
  return ToolGeneric
}
