<script setup lang="ts">
import { computed, ref } from 'vue'
import { renderMarkdown } from '@/composables/useMarkdown'

const TEXT_TRUNCATE_LEN = 8192

const props = defineProps<{
  input: Record<string, unknown>
  toolUseId: string
  name: string
}>()

// 启发式字段映射(按优先级)
const FIELD_GROUPS = [
  { kind: 'path', keys: ['file_path', 'path', 'filename'] },
  { kind: 'command', keys: ['command', 'cmd', 'script'] },
  { kind: 'query', keys: ['query', 'pattern', 'search'] },
  { kind: 'url', keys: ['url', 'link', 'href'] },
  { kind: 'markdown', keys: ['prompt', 'description', 'message'] },
  { kind: 'longtext', keys: ['content', 'text', 'body'] },
] as const

type FieldKind = typeof FIELD_GROUPS[number]['kind']

interface MatchedField {
  kind: FieldKind
  key: string
  value: string
}

const KNOWN_KEYS = new Set<string>(FIELD_GROUPS.flatMap(g => g.keys as readonly string[]))

const matchedFields = computed<MatchedField[]>(() => {
  const result: MatchedField[] = []
  for (const group of FIELD_GROUPS) {
    for (const key of group.keys) {
      const v = props.input[key]
      if (typeof v === 'string' && v.length > 0) {
        result.push({ kind: group.kind, key, value: v })
      }
    }
  }
  return result
})

const otherFields = computed(() => {
  const out: Record<string, unknown> = {}
  let has = false
  for (const [k, v] of Object.entries(props.input)) {
    if (KNOWN_KEYS.has(k)) {
      // 忽略已经被识别为字符串的字段;但若值类型不是 string,仍纳入 other
      if (typeof v === 'string') continue
    }
    out[k] = v
    has = true
  }
  return has ? out : null
})

const otherJson = computed(() => {
  if (!otherFields.value) return ''
  try {
    return JSON.stringify(otherFields.value, null, 2)
  } catch {
    return String(otherFields.value)
  }
})

const otherExpanded = ref(false)
const longTextExpanded = ref<Record<string, boolean>>({})

function isLongText(v: string) {
  return v.length > TEXT_TRUNCATE_LEN
}

function displayLongText(key: string, v: string) {
  if (longTextExpanded.value[key] || !isLongText(v)) return v
  return v.slice(0, TEXT_TRUNCATE_LEN)
}

function toggleLongText(key: string) {
  longTextExpanded.value[key] = !longTextExpanded.value[key]
}

function renderMd(text: string) {
  return renderMarkdown(text)
}
</script>

<template>
  <div class="mt-2 rounded-md bg-green-500/5 border border-green-500/20 px-3 py-2 text-xs space-y-1.5">
    <div class="flex items-center gap-1.5">
      <span class="i-carbon-tool-kit w-3.5 h-3.5 shrink-0" />
      <span class="text-green-400 font-medium">{{ name }}</span>
    </div>

    <template v-for="f in matchedFields" :key="f.key">
      <!-- 文件路径 -->
      <div v-if="f.kind === 'path'" class="flex items-center gap-1.5">
        <span class="text-default4 shrink-0">{{ f.key }}:</span>
        <span class="font-mono text-default3 truncate" :title="f.value">{{ f.value }}</span>
      </div>

      <!-- 命令 -->
      <pre
        v-else-if="f.kind === 'command'"
        class="rounded bg-default4/10 px-2 py-1 text-default2 whitespace-pre-wrap break-all font-mono"
      >$ {{ f.value }}</pre>

      <!-- 查询 -->
      <div v-else-if="f.kind === 'query'" class="flex items-center gap-1.5 flex-wrap">
        <span class="text-default4 shrink-0">{{ f.key }}:</span>
        <code class="px-1.5 py-0.5 rounded bg-yellow-500/15 text-yellow-400 font-mono">{{ f.value }}</code>
      </div>

      <!-- URL -->
      <div v-else-if="f.kind === 'url'" class="flex items-center gap-1.5">
        <span class="text-default4 shrink-0">{{ f.key }}:</span>
        <a
          :href="f.value"
          target="_blank"
          rel="noopener noreferrer"
          class="font-mono text-blue-400 hover:text-blue-300 truncate underline-offset-2 hover:underline"
          :title="f.value"
        >{{ f.value }}</a>
      </div>

      <!-- markdown 段 -->
      <div v-else-if="f.kind === 'markdown'" class="prose-msg" v-html="renderMd(f.value)" />

      <!-- 长文本可折叠 -->
      <div v-else-if="f.kind === 'longtext'">
        <div class="text-default4">{{ f.key }}:</div>
        <pre class="mt-1 rounded bg-default4/10 px-2 py-1 text-default3 whitespace-pre-wrap break-all font-mono max-h-96 overflow-y-auto">{{ displayLongText(f.key, f.value) }}</pre>
        <button
          v-if="isLongText(f.value)"
          class="mt-1 text-default4 hover:text-default3"
          @click="toggleLongText(f.key)"
        >
          {{ longTextExpanded[f.key] ? '收起' : `…展开全部（${Math.round(f.value.length / 1024)}KB）` }}
        </button>
      </div>
    </template>

    <!-- 其它字段 JSON 折叠 -->
    <div v-if="otherFields">
      <button
        class="flex items-center gap-1 text-default4 hover:text-default3"
        @click="otherExpanded = !otherExpanded"
      >
        <span class="i-carbon-chevron-right w-3 h-3 transition-transform" :class="{ 'rotate-90': otherExpanded }" />
        其它字段
      </button>
      <pre v-if="otherExpanded" class="mt-1 rounded bg-default4/10 px-2 py-1 text-default3 whitespace-pre-wrap break-all font-mono max-h-96 overflow-y-auto">{{ otherJson }}</pre>
    </div>
  </div>
</template>
