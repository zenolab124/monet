<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import { useCliSettings, type SettingsField, type SchemaProperty } from '@/composables/useCliSettings'
import { useUiState } from '@/composables/useUiState'

const { activeSection } = useUiState()
const {
  hasSchema, loading, translating, groups, groupOrder,
  load, updateField, removeField, refreshSchema,
  translateMissing, getTranslation,
} = useCliSettings()

const expandedGroups = ref<Set<string>>(new Set())
const addingKey = ref('')
const addingValue = ref('')
const savingKeys = ref<Set<string>>(new Set())
const search = ref('')

function toggleGroup(g: string) {
  if (expandedGroups.value.has(g)) expandedGroups.value.delete(g)
  else expandedGroups.value.add(g)
}

function fieldMatchesSearch(f: SettingsField, q: string): boolean {
  const t = getTranslation(f.key)
  return f.key.toLowerCase().includes(q)
    || (t?.name ?? '').includes(q)
    || (t?.desc ?? '').includes(q)
    || (f.schema?.description ?? '').toLowerCase().includes(q)
}

const filteredGroups = computed(() => {
  const q = search.value.trim().toLowerCase()
  if (!q) return groups.value

  const result: Record<string, SettingsField[]> = {}
  for (const [g, fields] of Object.entries(groups.value)) {
    const matched = fields.filter(f => fieldMatchesSearch(f, q))
    if (matched.length) result[g] = matched
  }
  return result
})

const filteredGroupOrder = computed(() => {
  const q = search.value.trim()
  if (!q) return groupOrder.value
  return groupOrder.value.filter(g => filteredGroups.value[g]?.length)
})

onMounted(async () => { await load(); translateMissing() })
watch(activeSection, async (s) => { if (s === 'settings') { await load(); translateMissing() } })

function fieldType(field: SettingsField): string {
  const s = field.schema
  if (!s) return 'unknown'
  const t = Array.isArray(s.type) ? s.type[0] : s.type
  if (s.enum) return 'enum'
  return t ?? 'unknown'
}

function isSet(field: SettingsField): boolean {
  return field.value !== undefined
}

function displayValue(v: unknown): string {
  if (v === undefined || v === null) return ''
  if (typeof v === 'object') return JSON.stringify(v, null, 2)
  return String(v)
}

function shortDesc(desc?: string): string {
  if (!desc) return ''
  const first = desc.split(/[.。]/)
  let s = first[0] ?? ''
  s = s.replace(/See https?:\/\/\S+/g, '').trim()
  if (s.length > 120) s = s.slice(0, 117) + '…'
  return s
}

async function onToggle(field: SettingsField) {
  const cur = field.value as boolean | undefined
  savingKeys.value.add(field.key)
  try {
    await updateField(field.key, !cur)
  } finally {
    savingKeys.value.delete(field.key)
  }
}

async function onEnumChange(field: SettingsField, e: Event) {
  const val = (e.target as HTMLSelectElement).value
  savingKeys.value.add(field.key)
  try {
    if (val === '__unset__') await removeField(field.key)
    else await updateField(field.key, val)
  } finally {
    savingKeys.value.delete(field.key)
  }
}

async function onStringBlur(field: SettingsField, e: Event) {
  const val = (e.target as HTMLInputElement).value
  savingKeys.value.add(field.key)
  try {
    if (val === '') await removeField(field.key)
    else await updateField(field.key, val)
  } finally {
    savingKeys.value.delete(field.key)
  }
}

async function onNumberBlur(field: SettingsField, e: Event) {
  const raw = (e.target as HTMLInputElement).value
  savingKeys.value.add(field.key)
  try {
    if (raw === '') await removeField(field.key)
    else await updateField(field.key, Number(raw))
  } finally {
    savingKeys.value.delete(field.key)
  }
}

async function onJsonBlur(field: SettingsField, e: Event) {
  const raw = (e.target as HTMLTextAreaElement).value.trim()
  savingKeys.value.add(field.key)
  try {
    if (raw === '') { await removeField(field.key); return }
    const parsed = JSON.parse(raw)
    await updateField(field.key, parsed)
  } catch {
    // invalid JSON, ignore
  } finally {
    savingKeys.value.delete(field.key)
  }
}

async function onAddCustom() {
  const key = addingKey.value.trim()
  if (!key) return
  let val: unknown = addingValue.value.trim()
  try { val = JSON.parse(val as string) } catch { /* keep as string */ }
  await updateField(key, val)
  addingKey.value = ''
  addingValue.value = ''
}

async function onRemove(key: string) {
  savingKeys.value.add(key)
  try { await removeField(key) } finally { savingKeys.value.delete(key) }
}
</script>

<template>
  <div>
    <div class="flex items-center justify-between mb-4">
      <div>
        <h2 class="text-[13px] font-semibold">Claude Code 配置</h2>
        <p class="text-[11px] text-muted-foreground mt-0.5">
          读写 <span class="font-mono">~/.claude/settings.json</span>
          <span v-if="hasSchema" class="ml-1">· schema 已加载</span>
          <span v-else class="ml-1 text-accent">· schema 未就绪</span>
          <span v-if="translating" class="ml-1">· AI 翻译中…</span>
        </p>
      </div>
      <button
        class="px-2 py-1 text-[11px] rounded border border-border text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
        :disabled="loading"
        @click="refreshSchema"
      >
        <span class="i-carbon-renew w-3 h-3 mr-1" :class="{ 'animate-spin': loading }" />刷新 Schema
      </button>
    </div>

    <!-- 搜索框 -->
    <div class="search-bar">
      <span class="i-carbon-search w-3.5 h-3.5 text-muted-foreground shrink-0" />
      <input
        v-model="search"
        type="text"
        class="search-input"
        placeholder="搜索配置项（key、中文名、说明…）"
      />
      <button
        v-if="search"
        class="text-muted-foreground hover:text-foreground p-0.5"
        @click="search = ''"
      >
        <span class="i-carbon-close w-3 h-3" />
      </button>
    </div>

    <div v-if="loading && !groupOrder.length" class="text-xs text-muted-foreground py-8 text-center">
      加载中…
    </div>

    <div v-else-if="search && !filteredGroupOrder.length" class="text-xs text-muted-foreground py-6 text-center">
      无匹配项
    </div>

    <!-- 分组手风琴 -->
    <div v-for="g in filteredGroupOrder" :key="g" class="group-section">
      <button class="group-header" @click="toggleGroup(g)">
        <span
          class="i-carbon-chevron-right w-3 h-3 transition-transform"
          :class="{ 'rotate-90': expandedGroups.has(g) || !!search }"
        />
        <span class="font-medium">{{ g }}</span>
        <span class="text-muted-foreground ml-auto">{{ filteredGroups[g]?.length ?? 0 }}</span>
      </button>

      <div v-show="expandedGroups.has(g) || !!search" class="group-body">
        <div v-for="f in filteredGroups[g]" :key="f.key" class="field-row">
          <div class="field-meta">
            <div class="flex items-center gap-1.5">
              <template v-if="getTranslation(f.key)">
                <span class="text-[11.5px] font-medium">{{ getTranslation(f.key)!.name }}</span>
                <span class="font-mono text-[10px] text-muted-foreground">{{ f.key }}</span>
              </template>
              <template v-else>
                <span class="font-mono text-[11px] font-medium">{{ f.key }}</span>
              </template>
              <span v-if="f.source === 'custom'" class="custom-badge">自定义</span>
              <span v-if="savingKeys.has(f.key)" class="text-[10px] text-accent">保存中…</span>
            </div>
            <div class="text-[10.5px] text-muted-foreground mt-0.5 leading-snug">
              <template v-if="getTranslation(f.key)?.desc">
                {{ getTranslation(f.key)!.desc }}
              </template>
              <template v-else-if="f.schema?.description">
                {{ shortDesc(f.schema.description) }}
              </template>
            </div>
          </div>

          <div class="field-control">
            <!-- boolean -->
            <template v-if="fieldType(f) === 'boolean'">
              <button
                :class="['toggle-track', { on: !!f.value }]"
                @click="onToggle(f)"
              >
                <span class="toggle-knob" />
              </button>
            </template>

            <!-- enum / select -->
            <template v-else-if="fieldType(f) === 'enum'">
              <select
                class="ctrl-select"
                :value="f.value ?? '__unset__'"
                @change="onEnumChange(f, $event)"
              >
                <option value="__unset__">— 未设置 —</option>
                <option v-for="opt in f.schema?.enum" :key="String(opt)" :value="opt">
                  {{ opt }}
                </option>
              </select>
            </template>

            <!-- integer / number -->
            <template v-else-if="fieldType(f) === 'integer' || fieldType(f) === 'number'">
              <input
                type="number"
                class="ctrl-input w-24"
                :value="f.value ?? ''"
                :placeholder="f.schema?.default != null ? String(f.schema.default) : ''"
                :min="f.schema?.minimum"
                :max="f.schema?.maximum"
                @blur="onNumberBlur(f, $event)"
              />
            </template>

            <!-- string (simple) -->
            <template v-else-if="fieldType(f) === 'string' && !f.schema?.properties">
              <input
                type="text"
                class="ctrl-input"
                :value="f.value ?? ''"
                :placeholder="f.schema?.default != null ? String(f.schema.default) : ''"
                @blur="onStringBlur(f, $event)"
              />
            </template>

            <!-- object / array / complex -->
            <template v-else>
              <textarea
                class="ctrl-textarea"
                :value="displayValue(f.value)"
                rows="3"
                spellcheck="false"
                @blur="onJsonBlur(f, $event)"
              />
            </template>

            <!-- 删除按钮（仅已设置的值） -->
            <button
              v-if="isSet(f)"
              class="remove-btn"
              title="移除此字段"
              @click="onRemove(f.key)"
            >
              <span class="i-carbon-close w-3 h-3" />
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- 新增自定义字段 -->
    <div class="add-section">
      <h3 class="text-[11px] font-medium text-muted-foreground mb-2">添加配置项</h3>
      <div class="flex gap-2 items-end">
        <div class="flex-1 min-w-0">
          <label class="text-[10px] text-muted-foreground">字段名</label>
          <input
            v-model="addingKey"
            type="text"
            class="ctrl-input w-full"
            placeholder="camelCase key"
          />
        </div>
        <div class="flex-1 min-w-0">
          <label class="text-[10px] text-muted-foreground">值（JSON 或文本）</label>
          <input
            v-model="addingValue"
            type="text"
            class="ctrl-input w-full"
            placeholder="true / &quot;text&quot; / {}"
          />
        </div>
        <button
          class="px-2.5 py-[5px] text-xs rounded bg-primary text-primary-foreground hover:shadow-paper transition-shadow shrink-0"
          :disabled="!addingKey.trim()"
          @click="onAddCustom"
        >
          添加
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.search-bar {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 10px;
  margin-bottom: 8px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--popover);
  transition: border-color 0.15s;
}
.search-bar:focus-within { border-color: var(--ring); }
.search-input {
  flex: 1;
  border: none;
  background: none;
  font-size: 12px;
  color: var(--foreground);
  outline: none;
}
.search-input::placeholder { color: var(--muted-foreground); }

.group-section {
  border: 1px solid var(--border);
  border-radius: var(--radius);
  margin-bottom: 6px;
  overflow: hidden;
}
.group-header {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  padding: 8px 12px;
  font-size: 12px;
  background: var(--background);
  border: none;
  cursor: pointer;
  color: var(--foreground);
  transition: background 0.15s;
}
.group-header:hover { background: var(--muted); }
.group-body { padding: 0 12px 8px; }

.field-row {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 8px 0;
}
.field-row + .field-row { border-top: 1px solid var(--border); }
.field-meta { flex: 1 1 0; min-width: 0; }
.field-control {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 6px;
  max-width: 260px;
}

.custom-badge {
  padding: 0 4px;
  font-size: 9px;
  line-height: 14px;
  border: 1px solid var(--accent);
  color: var(--accent);
  border-radius: 3px;
}

.ctrl-input {
  padding: 4px 8px;
  font-size: 11px;
  font-family: var(--font-mono, monospace);
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: var(--popover);
  color: var(--foreground);
  max-width: 220px;
}
.ctrl-input:focus { outline: none; border-color: var(--ring); }

.ctrl-textarea {
  padding: 4px 8px;
  font-size: 11px;
  font-family: var(--font-mono, monospace);
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: var(--popover);
  color: var(--foreground);
  width: 220px;
  resize: vertical;
  min-height: 48px;
}
.ctrl-textarea:focus { outline: none; border-color: var(--ring); }

.ctrl-select {
  padding: 4px 8px;
  font-size: 11px;
  font-family: inherit;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: var(--popover);
  color: var(--foreground);
  max-width: 220px;
}
.ctrl-select:focus { outline: none; border-color: var(--ring); }

.toggle-track {
  position: relative;
  width: 36px; height: 20px;
  border-radius: 10px;
  background: var(--muted);
  border: 1px solid var(--border);
  cursor: pointer;
  transition: background 0.2s;
  flex-shrink: 0;
}
.toggle-track.on { background: var(--primary); border-color: var(--primary); }
.toggle-knob {
  display: block; width: 14px; height: 14px;
  border-radius: 50%; background: white;
  position: absolute; top: 2px; left: 2px;
  transition: transform 0.2s;
  box-shadow: 0 1px 2px rgba(0,0,0,0.15);
}
.toggle-track.on .toggle-knob { transform: translateX(16px); }

.remove-btn {
  padding: 3px;
  border-radius: var(--radius);
  color: var(--muted-foreground);
  border: none;
  background: none;
  cursor: pointer;
  transition: color 0.15s, background 0.15s;
}
.remove-btn:hover {
  color: var(--destructive);
  background: hsl(var(--destructive) / 0.1);
}

.add-section {
  margin-top: 16px;
  padding: 12px;
  border: 1px dashed var(--border);
  border-radius: var(--radius);
}
</style>
