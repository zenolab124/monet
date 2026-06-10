<script setup lang="ts">
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

const props = defineProps<{
  sessionId: string
  projectId: string
  cwd: string | null
}>()

const emit = defineEmits<{
  deleted: []
}>()

const confirmingDelete = ref(false)

async function resumeInTerminal() {
  if (!props.cwd) return
  try {
    await invoke('resume_in_terminal', { cwd: props.cwd, sessionId: props.sessionId })
  } catch (e) {
    console.error('恢复失败:', e)
  }
}

async function resumeInVscode() {
  if (!props.cwd) return
  try {
    await invoke('resume_in_vscode', { cwd: props.cwd })
  } catch (e) {
    console.error('打开 VSCode 失败:', e)
  }
}

async function deleteSession() {
  try {
    await invoke('delete_session', { projectId: props.projectId, sessionId: props.sessionId })
    confirmingDelete.value = false
    emit('deleted')
  } catch (e) {
    console.error('删除失败:', e)
  }
}
</script>

<template>
  <div class="flex items-center gap-1.5">
    <!-- 在终端恢复 -->
    <button
      v-if="cwd"
      class="px-2 py-1 text-xs rounded-md text-muted-foreground hover:text-foreground hover:bg-muted transition-colors flex items-center gap-1"
      title="在终端恢复"
      @click="resumeInTerminal"
    >
      <span class="i-carbon-terminal w-3.5 h-3.5" />
      终端
    </button>

    <!-- 在 VSCode 恢复 -->
    <button
      v-if="cwd"
      class="px-2 py-1 text-xs rounded-md text-muted-foreground hover:text-foreground hover:bg-muted transition-colors flex items-center gap-1"
      title="在 VSCode 恢复"
      @click="resumeInVscode"
    >
      <span class="i-carbon-code w-3.5 h-3.5" />
      VSCode
    </button>

    <!-- 删除 -->
    <template v-if="!confirmingDelete">
      <button
        class="px-2 py-1 text-xs rounded-md text-muted-foreground hover:text-destructive hover:bg-destructive/10 transition-colors flex items-center gap-1"
        title="删除会话"
        @click="confirmingDelete = true"
      >
        <span class="i-carbon-trash-can w-3.5 h-3.5" />
      </button>
    </template>
    <template v-else>
      <span class="text-xs text-muted-foreground">确认删除？</span>
      <button
        class="px-2 py-0.5 text-xs rounded-md bg-destructive/15 text-destructive hover:bg-destructive/25 transition-colors"
        @click="deleteSession"
      >
        删除
      </button>
      <button
        class="px-2 py-0.5 text-xs rounded-md text-muted-foreground hover:text-foreground transition-colors"
        @click="confirmingDelete = false"
      >
        取消
      </button>
    </template>
  </div>
</template>
