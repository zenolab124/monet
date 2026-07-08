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
import ToolEnterPlanMode from './ToolEnterPlanMode.vue'
import ToolAskUserQuestion from './ToolAskUserQuestion.vue'
import ToolSkill from './ToolSkill.vue'
import ToolSendUserFile from './ToolSendUserFile.vue'
import ToolMcp from './ToolMcp.vue'
import ToolGeneric from './ToolGeneric.vue'

export const TOOL_MAP: Record<string, Component> = {
  Read: ToolRead, Edit: ToolEdit, Write: ToolWrite, NotebookEdit: ToolNotebookEdit,
  Bash: ToolBash,
  Grep: ToolGrep, Glob: ToolGlob,
  WebFetch: ToolWebFetch, WebSearch: ToolWebSearch,
  TodoWrite: ToolTodoWrite,
  // Agent 是 Task 的新名（claude CLI 改名），共用同一组件，标题显示实际 name
  Task: ToolTask, Agent: ToolTask, Workflow: ToolTask,
  ExitPlanMode: ToolExitPlanMode, EnterPlanMode: ToolEnterPlanMode,
  AskUserQuestion: ToolAskUserQuestion, Skill: ToolSkill,
  SendUserFile: ToolSendUserFile,
}

export { ToolMcp, ToolGeneric }

export function resolveTool(name: string): Component {
  if (name in TOOL_MAP) return TOOL_MAP[name]
  if (name.startsWith('mcp__')) return ToolMcp
  return ToolGeneric
}
