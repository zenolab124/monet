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
  if (status === 'completed') return 'text-default4 line-through opacity-60'
  if (status === 'in_progress') return 'text-blue-400 font-medium'
  return 'text-default3'
}

function iconClasses(status: string): string {
  if (status === 'completed') return 'text-green-400'
  if (status === 'in_progress') return 'text-blue-400'
  return 'text-default4'
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
    <div v-if="todos.length === 0" class="text-default4">（无待办项）</div>
  </div>
</template>
