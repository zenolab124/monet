<script setup lang="ts">
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const props = defineProps<{
  value: unknown
}>()

const emit = defineEmits<{
  update: [value: Record<string, unknown>]
}>()

interface PermValue {
  allow?: string[]
  deny?: string[]
  ask?: string[]
  defaultMode?: string
  disableBypassPermissionsMode?: string
  disableAutoMode?: string
  additionalDirectories?: string[]
}

const perm = computed<PermValue>(() => {
  if (props.value && typeof props.value === 'object') return props.value as PermValue
  return {}
})

const TOOLS = [
  'Bash', 'Read', 'Write', 'Edit', 'Glob', 'Grep', 'WebFetch', 'WebSearch',
  'Agent', 'Skill', 'Workflow', 'NotebookEdit', 'TodoWrite', 'Monitor',
  'TaskOutput', 'TaskStop', 'LSP', 'KillShell', 'PowerShell',
  'EnterPlanMode', 'ExitPlanMode', 'AskUserQuestion', 'ToolSearch',
]

const DEFAULT_MODES = [
  'default', 'acceptEdits', 'plan', 'auto', 'dontAsk', 'bypassPermissions', 'delegate',
]

type RuleCategory = 'allow' | 'deny' | 'ask'
const CATEGORIES: { key: RuleCategory; icon: string; color: string }[] = [
  { key: 'allow', icon: 'i-carbon-checkmark-filled', color: 'text-green-600' },
  { key: 'deny', icon: 'i-carbon-close-filled', color: 'text-red-500' },
  { key: 'ask', icon: 'i-carbon-help-filled', color: 'text-amber-500' },
]

const addTool = ref<Record<RuleCategory, string>>({ allow: 'Bash', deny: 'Bash', ask: 'Bash' })
const addPattern = ref<Record<RuleCategory, string>>({ allow: '', deny: '', ask: '' })
const addDirInput = ref('')

function patch(delta: Partial<PermValue>) {
  emit('update', { ...perm.value, ...delta })
}

function addRule(cat: RuleCategory) {
  const tool = addTool.value[cat]
  const pattern = addPattern.value[cat].trim()
  const rule = pattern ? `${tool}(${pattern})` : tool
  const list = [...(perm.value[cat] ?? [])]
  if (!list.includes(rule)) {
    list.push(rule)
    patch({ [cat]: list })
  }
  addPattern.value[cat] = ''
}

function removeRule(cat: RuleCategory, rule: string) {
  const list = (perm.value[cat] ?? []).filter(r => r !== rule)
  patch({ [cat]: list.length ? list : undefined })
}

function addDir() {
  const dir = addDirInput.value.trim()
  if (!dir) return
  const list = [...(perm.value.additionalDirectories ?? [])]
  if (!list.includes(dir)) {
    list.push(dir)
    patch({ additionalDirectories: list })
  }
  addDirInput.value = ''
}

function removeDir(dir: string) {
  const list = (perm.value.additionalDirectories ?? []).filter(d => d !== dir)
  patch({ additionalDirectories: list.length ? list : undefined })
}

function parseRule(rule: string): { tool: string; pattern?: string } {
  const m = rule.match(/^([^(]+?)(?:\((.+)\))?$/)
  return m ? { tool: m[1], pattern: m[2] } : { tool: rule }
}
</script>

<template>
  <div class="perm-editor">
    <!-- defaultMode -->
    <div class="perm-row">
      <span class="perm-label">{{ $t('settings.permissions.defaultMode') }}</span>
      <select
        class="form-select form-select-sm"
        :value="perm.defaultMode ?? 'default'"
        @change="patch({ defaultMode: ($event.target as HTMLSelectElement).value })"
      >
        <option v-for="m in DEFAULT_MODES" :key="m" :value="m">{{ m }}</option>
      </select>
    </div>

    <!-- disable toggles -->
    <div class="perm-row">
      <span class="perm-label">{{ $t('settings.permissions.disableBypass') }}</span>
      <button
        :class="['form-toggle form-toggle-sm', { on: perm.disableBypassPermissionsMode === 'disable' }]"
        @click="patch({ disableBypassPermissionsMode: perm.disableBypassPermissionsMode === 'disable' ? undefined : 'disable' })"
      >
        <span class="form-toggle-knob" />
      </button>
    </div>
    <div class="perm-row">
      <span class="perm-label">{{ $t('settings.permissions.disableAuto') }}</span>
      <button
        :class="['form-toggle form-toggle-sm', { on: perm.disableAutoMode === 'disable' }]"
        @click="patch({ disableAutoMode: perm.disableAutoMode === 'disable' ? undefined : 'disable' })"
      >
        <span class="form-toggle-knob" />
      </button>
    </div>

    <!-- allow / deny / ask -->
    <div v-for="cat in CATEGORIES" :key="cat.key" class="perm-section">
      <div class="perm-section-header">
        <span :class="[cat.icon, 'w-3.5 h-3.5', cat.color]" />
        <span class="font-medium">{{ $t(`settings.permissions.${cat.key}`) }}</span>
        <span class="text-muted-foreground text-[10px] ml-1">{{ (perm[cat.key] ?? []).length }}</span>
      </div>

      <div class="perm-rules">
        <span
          v-for="rule in (perm[cat.key] ?? [])"
          :key="rule"
          class="perm-chip"
        >
          <span class="font-medium">{{ parseRule(rule).tool }}</span>
          <span v-if="parseRule(rule).pattern" class="text-muted-foreground">({{ parseRule(rule).pattern }})</span>
          <button class="perm-chip-del" @click="removeRule(cat.key, rule)">
            <span class="i-carbon-close w-2.5 h-2.5" />
          </button>
        </span>
        <span v-if="!(perm[cat.key] ?? []).length" class="text-[10.5px] text-muted-foreground/50 italic">
          {{ $t('settings.permissions.empty') }}
        </span>
      </div>

      <div class="perm-add">
        <select v-model="addTool[cat.key]" class="form-select form-select-sm w-[110px] shrink-0">
          <option v-for="tool in TOOLS" :key="tool" :value="tool">{{ tool }}</option>
        </select>
        <input
          v-model="addPattern[cat.key]"
          class="form-input form-input-sm font-mono flex-1"
          placeholder="pattern（可选）"
          @keydown.enter="addRule(cat.key)"
        />
        <button
          class="px-2 py-[3px] text-[11px] rounded bg-primary text-primary-foreground shrink-0"
          @click="addRule(cat.key)"
        >{{ $t('common.add') }}</button>
      </div>
    </div>

    <!-- additionalDirectories -->
    <div class="perm-section">
      <div class="perm-section-header">
        <span class="i-carbon-folder w-3.5 h-3.5 text-muted-foreground" />
        <span class="font-medium">{{ $t('settings.permissions.additionalDirs') }}</span>
      </div>
      <div class="perm-rules">
        <span v-for="dir in (perm.additionalDirectories ?? [])" :key="dir" class="perm-chip">
          <span class="font-mono">{{ dir }}</span>
          <button class="perm-chip-del" @click="removeDir(dir)">
            <span class="i-carbon-close w-2.5 h-2.5" />
          </button>
        </span>
        <span v-if="!(perm.additionalDirectories ?? []).length" class="text-[10.5px] text-muted-foreground/50 italic">
          {{ $t('settings.permissions.empty') }}
        </span>
      </div>
      <div class="perm-add">
        <input
          v-model="addDirInput"
          class="form-input form-input-sm font-mono flex-1"
          placeholder="~/projects 或绝对路径"
          @keydown.enter="addDir"
        />
        <button
          class="px-2 py-[3px] text-[11px] rounded bg-primary text-primary-foreground shrink-0"
          :disabled="!addDirInput.trim()"
          @click="addDir"
        >{{ $t('common.add') }}</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.perm-editor {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.perm-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 6px 10px;
  background: var(--popover);
  border: 1px solid var(--border);
  border-radius: var(--radius);
}
.perm-label {
  font-size: 11px;
  font-weight: 500;
}

.perm-section {
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 10px 12px;
  background: var(--popover);
}
.perm-section-header {
  display: flex;
  align-items: center;
  gap: 5px;
  font-size: 11.5px;
  margin-bottom: 8px;
  padding-bottom: 6px;
  border-bottom: 1px solid var(--border);
}

.perm-rules {
  display: flex;
  flex-wrap: wrap;
  gap: 5px;
  min-height: 26px;
  margin-bottom: 8px;
}

.perm-chip {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  padding: 3px 8px;
  font-size: 11px;
  background: var(--muted);
  border: 1px solid var(--border);
  border-radius: 100px;
  line-height: 1.3;
  box-shadow: 0 1px 2px hsl(var(--foreground) / 0.04);
}
.perm-chip-del {
  margin-left: 1px;
  padding: 1px;
  color: var(--muted-foreground);
  border: none;
  background: none;
  cursor: pointer;
  border-radius: 100px;
  transition: color 0.15s, background 0.15s;
}
.perm-chip-del:hover {
  color: var(--destructive);
  background: hsl(var(--destructive) / 0.1);
}

.perm-add {
  display: flex;
  gap: 4px;
  align-items: center;
}
</style>
