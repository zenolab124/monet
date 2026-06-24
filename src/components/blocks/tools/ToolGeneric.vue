<script setup lang="ts">
import { computed, ref } from 'vue'
import { renderMarkdownCached } from '@/composables/useMarkdown'

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

// 模板内方法调用,组件每次重渲染都会执行——走缓存把重复 parse 压成 Map 查找
function renderMd(text: string) {
  return renderMarkdownCached(text)
}
</script>

<template>
  <div class="mt-2 rounded-md bg-background border border-border px-3 py-2 text-xs space-y-1.5">
    <div class="flex items-center gap-1.5">
      <span class="i-carbon-tool-kit w-3.5 h-3.5 shrink-0" />
      <span class="text-foreground font-medium">{{ name }}</span>
    </div>

    <template v-for="f in matchedFields" :key="f.key">
      <!-- 文件路径 -->
      <div v-if="f.kind === 'path'" class="flex items-center gap-1.5">
        <span class="text-muted-foreground shrink-0">{{ f.key }}:</span>
        <span class="font-mono text-muted-foreground truncate" :title="f.value">{{ f.value }}</span>
      </div>

      <!-- 命令 -->
      <pre
        v-else-if="f.kind === 'command'"
        class="rounded bg-muted px-2 py-1 text-foreground whitespace-pre-wrap break-all font-mono"
      >$ {{ f.value }}</pre>

      <!-- 查询 -->
      <div v-else-if="f.kind === 'query'" class="flex items-center gap-1.5 flex-wrap">
        <span class="text-muted-foreground shrink-0">{{ f.key }}:</span>
        <code class="px-1.5 py-0.5 rounded border border-border text-muted-foreground font-mono break-all">{{ f.value }}</code>
      </div>

      <!-- URL -->
      <div v-else-if="f.kind === 'url'" class="flex items-center gap-1.5">
        <span class="text-muted-foreground shrink-0">{{ f.key }}:</span>
        <a
          :href="f.value"
          target="_blank"
          rel="noopener noreferrer"
          class="font-mono text-primary hover:text-primary/80 truncate underline-offset-2 hover:underline"
          :title="f.value"
        >{{ f.value }}</a>
      </div>

      <!-- markdown 段 -->
      <div v-else-if="f.kind === 'markdown'" class="prose-msg" v-html="renderMd(f.value)" />

      <!-- 长文本可折叠 -->
      <div v-else-if="f.kind === 'longtext'">
        <div class="text-muted-foreground">{{ f.key }}:</div>
        <pre class="mt-1 rounded bg-muted px-2 py-1 text-muted-foreground whitespace-pre-wrap break-all font-mono max-h-96 overflow-y-auto">{{ displayLongText(f.key, f.value) }}</pre>
        <button
          v-if="isLongText(f.value)"
          class="mt-1 text-muted-foreground hover:text-foreground"
          @click="toggleLongText(f.key)"
        >
          {{ longTextExpanded[f.key] ? $t('common.collapse') : $t('common.expandAll', { size: Math.round(f.value.length / 1024) }) }}
        </button>
      </div>
    </template>

    <!-- 其它字段 JSON 折叠 -->
    <div v-if="otherFields">
      <button
        class="flex items-center gap-1 text-muted-foreground hover:text-foreground"
        @click="otherExpanded = !otherExpanded"
      >
        <span class="i-carbon-chevron-right w-3 h-3 transition-transform" :class="{ 'rotate-90': otherExpanded }" />
        {{ $t('block.toolGeneric.otherFields') }}
      </button>
      <pre v-if="otherExpanded" class="mt-1 rounded bg-muted px-2 py-1 text-muted-foreground whitespace-pre-wrap break-all font-mono max-h-96 overflow-y-auto">{{ otherJson }}</pre>
    </div>
  </div>
</template>
