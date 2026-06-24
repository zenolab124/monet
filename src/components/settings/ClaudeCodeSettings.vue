<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import { useCliSettings, type SettingsField, type SchemaProperty } from '@/composables/useCliSettings'
import { useUiState } from '@/composables/useUiState'
import PermissionsEditor from './PermissionsEditor.vue'

const { t } = useI18n()
const { activeSection } = useUiState()
const {
  hasSchema, loading, translating, extracting, settings, groups, groupOrder,
  load, updateField, removeField, refreshSchema,
  translateMissing, extractMissingDefaults, getTranslation, getDefault,
} = useCliSettings()

const mcpRegistered = ref(false)
const mcpLoading = ref(false)

async function loadMcpStatus() {
  try {
    const status = await invoke<{ registered: boolean }>('get_mcp_status')
    mcpRegistered.value = status.registered
  } catch { /* ignore */ }
}

async function toggleMcp() {
  mcpLoading.value = true
  try {
    if (mcpRegistered.value) {
      await invoke('unregister_mcp')
    } else {
      await invoke('register_mcp')
    }
    await loadMcpStatus()
  } catch { /* ignore */ }
  finally { mcpLoading.value = false }
}

const addingKey = ref('')
const addingValue = ref('')
const savingKeys = ref<Set<string>>(new Set())
const search = ref('')
const onlyConfigured = ref(false)
const configuredCount = computed(() =>
  Object.values(groups.value).flat().filter(f => f.value !== undefined).length,
)
const activeGroup = ref('')
const activeEditor = ref<string | null>(null)
const isNarrow = ref(false)
const NARROW_THRESHOLD = 900

function checkWidth() {
  const el = document.querySelector('.cli-root')
  isNarrow.value = (el?.clientWidth ?? window.innerWidth) < NARROW_THRESHOLD
}

const COMPLEX_FIELDS = new Set(['permissions'])

function hasEditor(key: string): boolean {
  return COMPLEX_FIELDS.has(key)
}

function openEditor(key: string) {
  activeEditor.value = activeEditor.value === key ? null : key
}

const contentRef = ref<HTMLElement | null>(null)
const groupEls = ref<Record<string, HTMLElement>>({})

function setGroupRef(g: string, el: any) {
  if (el) groupEls.value[g] = el as HTMLElement
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
  const result: Record<string, SettingsField[]> = {}
  for (const [g, fields] of Object.entries(groups.value)) {
    let list = fields
    if (onlyConfigured.value) list = list.filter(f => f.value !== undefined)
    if (q) list = list.filter(f => fieldMatchesSearch(f, q))
    if (list.length) result[g] = list
  }
  return result
})

const filteredGroupOrder = computed(() => {
  const q = search.value.trim()
  if (!q) return groupOrder.value
  return groupOrder.value.filter(g => filteredGroups.value[g]?.length)
})

let observer: IntersectionObserver | null = null

function setupObserver() {
  if (observer) observer.disconnect()
  const root = contentRef.value?.closest('.overflow-y-auto') as HTMLElement | null
  if (!root) return
  observer = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (e.isIntersecting) {
          const g = (e.target as HTMLElement).dataset.group
          if (g) activeGroup.value = g
          break
        }
      }
    },
    { root, rootMargin: '-20px 0px -70% 0px', threshold: 0 },
  )
  for (const el of Object.values(groupEls.value)) {
    observer.observe(el)
  }
}

let resizeObs: ResizeObserver | null = null

onMounted(async () => {
  await load()
  loadMcpStatus()
  translateMissing()
  extractMissingDefaults()
  await nextTick()
  if (!activeGroup.value && filteredGroupOrder.value.length) {
    activeGroup.value = filteredGroupOrder.value[0]
  }
  setupObserver()
  checkWidth()
  const root = document.querySelector('.cli-root')
  if (root) {
    resizeObs = new ResizeObserver(checkWidth)
    resizeObs.observe(root)
  }
})

watch(activeSection, async (s) => {
  if (s === 'settings') {
    await load()
    translateMissing()
    extractMissingDefaults()
  }
})

watch(filteredGroupOrder, async () => {
  await nextTick()
  setupObserver()
})

onBeforeUnmount(() => { observer?.disconnect(); resizeObs?.disconnect() })

function scrollToGroup(g: string) {
  activeGroup.value = g
  const el = groupEls.value[g]
  if (el) el.scrollIntoView({ behavior: 'smooth', block: 'start' })
}

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

function effectiveBool(field: SettingsField): boolean {
  if (field.value !== undefined) return !!field.value
  const d = getDefault(field.key)
  if (d) return !!d.value
  return false
}

async function onToggle(field: SettingsField) {
  const cur = effectiveBool(field)
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
  <div class="cli-root">
    <!-- 顶部：标题 + 搜索 -->
    <div class="cli-header">
      <div class="flex items-center justify-between mb-2">
        <div>
          <h2 class="text-[13px] font-semibold">{{ $t('settings.cliConfig.title') }}</h2>
          <p class="text-[11px] text-muted-foreground mt-0.5">
            {{ $t('settings.cliConfig.subtitle') }}
            <span v-if="hasSchema" class="ml-1">{{ $t('settings.cliConfig.schemaLoaded') }}</span>
            <span v-else class="ml-1 text-accent">{{ $t('settings.cliConfig.schemaNotReady') }}</span>
            <span v-if="translating" class="ml-1">{{ $t('settings.cliConfig.aiTranslating') }}</span>
            <span v-if="extracting" class="ml-1">{{ $t('settings.cliConfig.aiExtracting') }}</span>
          </p>
        </div>
      </div>
      <!-- MCP Server 注册 -->
      <div class="mcp-card">
        <div class="flex items-center gap-2">
          <span class="i-carbon-plug w-3.5 h-3.5 text-muted-foreground" />
          <span class="text-[11.5px] font-medium">{{ $t('settings.mcp.title') }}</span>
          <span :class="['mcp-status', { active: mcpRegistered }]">
            {{ mcpRegistered ? $t('settings.mcp.registered') : $t('settings.mcp.notRegistered') }}
          </span>
          <button
            :class="['form-toggle ml-auto', { on: mcpRegistered }]"
            :disabled="mcpLoading"
            @click="toggleMcp"
          >
            <span class="form-toggle-knob" />
          </button>
        </div>
        <p class="text-[10.5px] text-muted-foreground mt-1 leading-snug">
          {{ $t('settings.mcp.description') }}
        </p>
      </div>

      <div class="flex gap-2 items-center">
        <div class="search-bar flex-1">
          <span class="i-carbon-search w-3.5 h-3.5 text-muted-foreground shrink-0" />
          <input
            v-model="search"
            type="text"
            class="search-input"
            :placeholder="$t('settings.cliConfig.searchPlaceholder')"
          />
          <button
            v-if="search"
            class="text-muted-foreground hover:text-foreground p-0.5"
            @click="search = ''"
          >
            <span class="i-carbon-close w-3 h-3" />
          </button>
        </div>
        <button
          :class="['filter-chip', { active: onlyConfigured }]"
          @click="onlyConfigured = !onlyConfigured"
        >
          <span class="i-carbon-filter w-3 h-3" />
          {{ $t('settings.cliConfig.onlyConfigured') }}
          <span class="filter-chip-count">{{ configuredCount }}</span>
        </button>
        <button
          class="filter-chip"
          :disabled="loading"
          @click="refreshSchema"
        >
          <span class="i-carbon-renew w-3 h-3" :class="{ 'animate-spin': loading }" />
          {{ $t('settings.cliConfig.refreshSchema') }}
        </button>
      </div>
    </div>

    <div v-if="loading && !groupOrder.length" class="text-xs text-muted-foreground py-8 text-center">
      {{ $t('common.loading') }}
    </div>

    <div v-else-if="search && !filteredGroupOrder.length" class="text-xs text-muted-foreground py-6 text-center">
      {{ $t('common.noMatch') }}
    </div>

    <!-- 主体：左区（滚动）+ 右区（不滚动） -->
    <div v-else class="cli-body">
      <div class="cli-left">
        <nav class="cli-nav">
          <button
            v-for="g in filteredGroupOrder"
            :key="g"
            :class="['cli-nav-item', { active: activeGroup === g }]"
            @click="scrollToGroup(g)"
          >
            <span class="truncate">{{ g }}</span>
            <span class="cli-nav-count">{{ filteredGroups[g]?.length ?? 0 }}</span>
          </button>
        </nav>

        <div ref="contentRef" class="cli-content">
          <section
            v-for="g in filteredGroupOrder"
            :key="g"
            :ref="(el) => setGroupRef(g, el)"
            :data-group="g"
            class="group-section"
          >
            <h3 class="group-title">{{ g }}</h3>

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
                  <span v-if="f.source === 'custom'" class="custom-badge">{{ $t('settings.cliConfig.custom') }}</span>
                  <span v-if="savingKeys.has(f.key)" class="text-[10px] text-accent">{{ $t('common.saving') }}</span>
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
                <!-- 有专用编辑器的复杂字段 -->
                <template v-if="hasEditor(f.key)">
                  <button
                    :class="['editor-btn', { active: activeEditor === f.key }]"
                    @click="openEditor(f.key)"
                  >
                    <span class="i-carbon-settings-adjust w-3 h-3" />
                    {{ $t('settings.cliConfig.configure') }}
                  </button>
                </template>

                <!-- boolean -->
                <template v-else-if="fieldType(f) === 'boolean'">
                  <span
                    v-if="!isSet(f) && getDefault(f.key)"
                    :class="['source-badge', getDefault(f.key)!.source === 'schema' ? 'source-schema' : 'source-extracted']"
                    :title="getDefault(f.key)!.source === 'schema' ? $t('settings.cliConfig.sourceSchema') : $t('settings.cliConfig.sourceExtracted')"
                  >{{ getDefault(f.key)!.source === 'schema' ? $t('settings.cliConfig.default') : $t('settings.cliConfig.extracted') }}</span>
                  <button
                    :class="['form-toggle', { on: effectiveBool(f) }]"
                    @click="onToggle(f)"
                  >
                    <span class="form-toggle-knob" />
                  </button>
                </template>

                <!-- enum / select -->
                <template v-else-if="fieldType(f) === 'enum'">
                  <select
                    class="form-select form-select-sm max-w-[220px]"
                    :value="f.value ?? '__unset__'"
                    @change="onEnumChange(f, $event)"
                  >
                    <option value="__unset__">
                      {{ getDefault(f.key)
                        ? `${getDefault(f.key)!.source === 'schema' ? $t('settings.cliConfig.default') : $t('settings.cliConfig.extracted')}: ${getDefault(f.key)!.value}`
                        : $t('settings.cliConfig.notSet') }}
                    </option>
                    <option v-for="opt in f.schema?.enum" :key="String(opt)" :value="opt">
                      {{ opt }}
                    </option>
                  </select>
                </template>

                <!-- integer / number -->
                <template v-else-if="fieldType(f) === 'integer' || fieldType(f) === 'number'">
                  <input
                    type="number"
                    class="form-input form-input-sm font-mono w-24"
                    :value="f.value ?? ''"
                    :placeholder="getDefault(f.key) ? String(getDefault(f.key)!.value) : ''"
                    :min="f.schema?.minimum"
                    :max="f.schema?.maximum"
                    @blur="onNumberBlur(f, $event)"
                  />
                </template>

                <!-- string (simple) -->
                <template v-else-if="fieldType(f) === 'string' && !f.schema?.properties">
                  <input
                    type="text"
                    class="form-input form-input-sm font-mono max-w-[220px]"
                    :value="f.value ?? ''"
                    :placeholder="getDefault(f.key) ? String(getDefault(f.key)!.value) : ''"
                    @blur="onStringBlur(f, $event)"
                  />
                </template>

                <!-- object / array / complex -->
                <template v-else>
                  <textarea
                    class="form-textarea form-textarea-sm font-mono w-[220px]"
                    :value="displayValue(f.value)"
                    rows="3"
                    spellcheck="false"
                    @blur="onJsonBlur(f, $event)"
                  />
                </template>

                <!-- 删除按钮 -->
                <button
                  v-if="isSet(f)"
                  class="remove-btn"
                  :title="$t('settings.cliConfig.removeField')"
                  @click="onRemove(f.key)"
                >
                  <span class="i-carbon-close w-3 h-3" />
                </button>
              </div>
            </div>
          </section>

          <!-- 新增自定义字段 -->
          <div class="add-section">
            <h3 class="text-[11px] font-medium text-muted-foreground mb-2">{{ $t('settings.cliConfig.addConfig') }}</h3>
            <div class="flex gap-2 items-end">
              <div class="flex-1 min-w-0">
                <label class="text-[10px] text-muted-foreground">{{ $t('settings.cliConfig.fieldName') }}</label>
                <input
                  v-model="addingKey"
                  type="text"
                  class="form-input form-input-sm font-mono w-full"
                  placeholder="camelCase key"
                />
              </div>
              <div class="flex-1 min-w-0">
                <label class="text-[10px] text-muted-foreground">{{ $t('settings.cliConfig.fieldValue') }}</label>
                <input
                  v-model="addingValue"
                  type="text"
                  class="form-input form-input-sm font-mono w-full"
                  placeholder="true / &quot;text&quot; / {}"
                />
              </div>
              <button
                class="px-2.5 py-[5px] text-xs rounded bg-primary text-primary-foreground hover:shadow-paper transition-shadow shrink-0"
                :disabled="!addingKey.trim()"
                @click="onAddCustom"
              >
                {{ $t('common.add') }}
              </button>
            </div>
          </div>
        </div>
      </div>

      <!-- 右区：宽屏常驻 / 窄屏悬浮 -->
      <div v-if="!isNarrow" class="cli-detail">
        <template v-if="activeEditor">
          <div class="cli-detail-header">
            <span class="font-medium text-[12px]">{{ getTranslation(activeEditor)?.name ?? activeEditor }}</span>
            <span class="font-mono text-[10px] text-muted-foreground ml-1.5">{{ activeEditor }}</span>
            <button class="cli-detail-close" @click="activeEditor = null">
              <span class="i-carbon-close w-3.5 h-3.5" />
            </button>
          </div>
          <div class="cli-detail-body">
            <PermissionsEditor
              v-if="activeEditor === 'permissions'"
              :value="settings[activeEditor]"
              @update="val => updateField(activeEditor!, val)"
            />
          </div>
        </template>
        <div v-else class="cli-detail-empty">
          <span class="i-carbon-settings-adjust w-5 h-5 text-muted-foreground/15" />
          <p class="text-[11px] text-muted-foreground/30">{{ $t('settings.cliConfig.selectEditor') }}</p>
        </div>
      </div>

      <Transition name="detail-float">
        <div v-if="isNarrow && activeEditor" class="cli-detail-float">
          <div class="cli-detail-header">
            <span class="font-medium text-[12px]">{{ getTranslation(activeEditor)?.name ?? activeEditor }}</span>
            <span class="font-mono text-[10px] text-muted-foreground ml-1.5">{{ activeEditor }}</span>
            <button class="cli-detail-close" @click="activeEditor = null">
              <span class="i-carbon-close w-3.5 h-3.5" />
            </button>
          </div>
          <div class="cli-detail-body">
            <PermissionsEditor
              v-if="activeEditor === 'permissions'"
              :value="settings[activeEditor]"
              @update="val => updateField(activeEditor!, val)"
            />
          </div>
        </div>
      </Transition>
    </div>
  </div>
</template>

<style scoped>
.cli-root {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

.cli-header {
  flex-shrink: 0;
  padding-bottom: 10px;
}

.mcp-card {
  padding: 8px 12px;
  margin-bottom: 8px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--card);
}
.mcp-status {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 100px;
  color: var(--muted-foreground);
  border: 1px solid var(--border);
}
.mcp-status.active {
  color: var(--primary);
  border-color: var(--primary);
  background: hsl(var(--primary) / 0.08);
}

.search-bar {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 10px;
  margin-top: 10px;
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

/* 主体：不滚动，左右子区各自管滚动 */
.cli-body {
  position: relative;
  display: flex;
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

/* 左区：独立滚动 */
.cli-left {
  flex: 1;
  min-width: 0;
  display: flex;
  overflow-y: auto;
}

/* 分组导航 */
.cli-nav {
  width: 120px;
  flex-shrink: 0;
  position: sticky;
  top: 0;
  align-self: flex-start;
  padding-right: 8px;
  border-right: 1px solid var(--border);
}

.cli-nav-item {
  display: flex;
  align-items: center;
  gap: 4px;
  width: 100%;
  padding: 5px 8px;
  font-size: 11px;
  text-align: left;
  color: var(--muted-foreground);
  border-radius: var(--radius);
  transition: all 0.15s;
  margin-bottom: 1px;
  border: none;
  background: none;
  cursor: pointer;
}
.cli-nav-item:hover {
  color: var(--foreground);
  background: var(--muted);
}
.cli-nav-item.active {
  color: var(--primary);
  font-weight: 500;
  background: var(--card);
  box-shadow: var(--shadow-paper);
}
.cli-nav-count {
  margin-left: auto;
  font-size: 10px;
  color: var(--muted-foreground);
  opacity: 0.6;
}

/* 平铺内容 */
.cli-content {
  flex: 1;
  min-width: 0;
  padding: 0 20px 20px 16px;
}

/* 右区：常驻 detail 面板 */
.cli-detail {
  width: 400px;
  flex-shrink: 0;
  border-left: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  background: var(--card);
}

/* 窄屏悬浮 detail 面板 */
.cli-detail-float {
  position: absolute;
  top: 8px;
  right: 8px;
  bottom: 8px;
  width: min(400px, calc(100% - 16px));
  display: flex;
  flex-direction: column;
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  box-shadow: var(--shadow-paper), 0 8px 32px hsl(var(--foreground) / 0.1);
  z-index: 10;
}
.detail-float-enter-active,
.detail-float-leave-active {
  transition: opacity 0.15s, transform 0.15s;
}
.detail-float-enter-from,
.detail-float-leave-to {
  opacity: 0;
  transform: translateX(12px);
}

.cli-detail-header {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  padding: 10px 14px;
  border-bottom: 1px solid var(--border);
}
.cli-detail-close {
  margin-left: auto;
  padding: 4px;
  color: var(--muted-foreground);
  border: none;
  background: none;
  cursor: pointer;
  border-radius: var(--radius);
  transition: color 0.15s, background 0.15s;
}
.cli-detail-close:hover {
  color: var(--foreground);
  background: var(--muted);
}
.cli-detail-body {
  flex: 1;
  overflow-y: auto;
  padding: 14px;
}
.cli-detail-empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 6px;
}

.editor-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 4px 10px;
  font-size: 11px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: none;
  color: var(--muted-foreground);
  cursor: pointer;
  transition: all 0.15s;
}
.editor-btn:hover { color: var(--foreground); border-color: var(--ring); }
.editor-btn.active {
  color: var(--primary);
  border-color: var(--primary);
  background: hsl(var(--primary) / 0.06);
}

.filter-chip {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 5px 10px;
  font-size: 11px;
  white-space: nowrap;
  border: 1px solid var(--border);
  border-radius: 100px;
  background: var(--popover);
  color: var(--muted-foreground);
  cursor: pointer;
  transition: all 0.15s;
  flex-shrink: 0;
}
.filter-chip:hover { color: var(--foreground); border-color: var(--ring); }
.filter-chip.active {
  color: var(--primary);
  border-color: var(--primary);
  background: hsl(var(--primary) / 0.08);
}
.filter-chip-count {
  font-size: 10px;
  min-width: 16px;
  height: 16px;
  line-height: 16px;
  text-align: center;
  border-radius: 100px;
  background: var(--muted);
  color: var(--muted-foreground);
}
.filter-chip.active .filter-chip-count {
  background: var(--primary);
  color: var(--primary-foreground);
}

.group-section {
  margin-bottom: 20px;
}
.group-section:last-of-type {
  margin-bottom: 12px;
}

.group-title {
  font-size: 12px;
  font-weight: 600;
  padding-bottom: 6px;
  margin-bottom: 4px;
  border-bottom: 1px solid var(--border);
  color: var(--foreground);
}

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
.source-badge {
  padding: 0 4px;
  font-size: 9px;
  line-height: 14px;
  border-radius: 3px;
  white-space: nowrap;
  cursor: help;
}
.source-schema {
  color: var(--muted-foreground);
  border: 1px solid var(--border);
}
.source-extracted {
  color: var(--accent);
  border: 1px dashed var(--accent);
  opacity: 0.8;
}

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
