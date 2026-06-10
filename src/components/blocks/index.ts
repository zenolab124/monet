import type { Component } from 'vue'
import type { ContentBlock } from '@/types'
import BlockText from './BlockText.vue'
import BlockThinking from './BlockThinking.vue'
import BlockToolUse from './BlockToolUse.vue'
import BlockToolResult from './BlockToolResult.vue'
import BlockImage from './BlockImage.vue'
import BlockDocument from './BlockDocument.vue'
import BlockSkillPrompt from './BlockSkillPrompt.vue'
import BlockIdeOpenedFile from './BlockIdeOpenedFile.vue'
import BlockSystemReminder from './BlockSystemReminder.vue'
import BlockTaskNotification from './BlockTaskNotification.vue'
import BlockUnknown from './BlockUnknown.vue'

export {
  BlockText,
  BlockThinking,
  BlockToolUse,
  BlockToolResult,
  BlockImage,
  BlockDocument,
  BlockSkillPrompt,
  BlockIdeOpenedFile,
  BlockSystemReminder,
  BlockTaskNotification,
  BlockUnknown,
}

/** ContentBlock.type → 渲染组件的映射表 */
const BLOCK_MAP: Record<string, Component> = {
  text: BlockText,
  thinking: BlockThinking,
  tool_use: BlockToolUse,
  tool_result: BlockToolResult,
  image: BlockImage,
  document: BlockDocument,
  skill_prompt: BlockSkillPrompt,
  ide_opened_file: BlockIdeOpenedFile,
  'system-reminder': BlockSystemReminder,
  'task-notification': BlockTaskNotification,
}

/** 按 ContentBlock.type 解析到对应 Block 组件,未知类型走 BlockUnknown 兜底 */
export function resolveBlock(block: ContentBlock): Component {
  return BLOCK_MAP[block.type] ?? BlockUnknown
}
