<script setup lang="ts">
import { computed } from 'vue'

interface TodoItem {
  content: string
  status: string
  activeForm?: string
}

const props = defineProps<{
  input: Record<string, unknown>
  toolUseId: string
  name: string
}>()

const todos = computed<TodoItem[]>(() => {
  const v = props.input.todos
  if (!Array.isArray(v)) return []
  return v
    .filter(t => t && typeof t === 'object')
    .map(t => {
      const obj = t as Record<string, unknown>
      return {
        content: typeof obj.content === 'string' ? obj.content : '',
        status: typeof obj.status === 'string' ? obj.status : 'pending',
        activeForm: typeof obj.activeForm === 'string' ? obj.activeForm : undefined,
      }
    })
})

function statusIcon(status: string): string {
  if (status === 'completed') return 'i-carbon-checkmark'
  if (status === 'in_progress') return 'i-carbon-time'
  return 'i-carbon-radio-button'
}

function statusClasses(status: string): string {
  if (status === 'completed') return 'text-muted-foreground line-through opacity-60'
  if (status === 'in_progress') return 'text-foreground font-medium'
  return 'text-muted-foreground'
}

function iconClasses(status: string): string {
  if (status === 'completed') return 'text-primary'
  if (status === 'in_progress') return 'text-foreground'
  return 'text-muted-foreground'
}
</script>

<template>
  <div class="mt-2 text-xs">
    <ul class="space-y-1">
      <li v-for="(todo, i) in todos" :key="i" class="flex items-start gap-1.5">
        <span :class="[statusIcon(todo.status), iconClasses(todo.status), 'w-3.5 h-3.5 shrink-0 mt-0.5']" />
        <span :class="statusClasses(todo.status)">
          {{ todo.status === 'in_progress' && todo.activeForm ? todo.activeForm : todo.content }}
        </span>
      </li>
    </ul>
    <div v-if="todos.length === 0" class="text-muted-foreground">（无待办项）</div>
  </div>
</template>
