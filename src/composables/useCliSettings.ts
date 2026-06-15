import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface SchemaProperty {
  type?: string | string[]
  description?: string
  default?: unknown
  enum?: string[]
  examples?: unknown[]
  items?: Record<string, unknown>
  properties?: Record<string, SchemaProperty>
  minimum?: number
  maximum?: number
  anyOf?: Record<string, unknown>[]
  const?: unknown
}

export interface SettingsField {
  key: string
  schema: SchemaProperty | null
  value: unknown
  source: 'schema' | 'custom'
  group: string
}

interface SchemaResponse {
  properties: Record<string, SchemaProperty>
  $defs: Record<string, unknown>
  hasSchema: boolean
}

const GROUPS: Record<string, string[]> = {
  '模型与推理': [
    'model', 'effortLevel', 'alwaysThinkingEnabled', 'fastMode',
    'fastModePerSessionOptIn', 'availableModels', 'enforceAvailableModels',
    'modelOverrides', 'fallbackModel',
  ],
  '语言与输出': [
    'language', 'outputStyle',
  ],
  '权限': [
    'permissions', 'allowManagedPermissionRulesOnly',
    'autoMode', 'useAutoModeDuringPlan',
    'skipDangerousModePermissionPrompt',
  ],
  '界面与显示': [
    'showThinkingSummaries', 'showTurnDuration', 'showClearContextOnPlanAccept',
    'spinnerTipsEnabled', 'spinnerTipsOverride', 'spinnerVerbs',
    'prefersReducedMotion', 'feedbackSurveyRate',
    'tui', 'viewMode', 'terminalProgressBarEnabled',
    'statusLine', 'subagentStatusLine',
    'voiceEnabled', 'teammateMode',
  ],
  '文件与内存': [
    'autoMemoryEnabled', 'autoMemoryDirectory',
    'cleanupPeriodDays', 'respectGitignore', 'plansDirectory',
    'claudeMdExcludes',
  ],
  'Git 与归属': [
    'attribution', 'includeCoAuthoredBy', 'includeGitInstructions',
    'prUrlTemplate',
  ],
  'Hooks 与自动化': [
    'hooks', 'disableAllHooks', 'allowManagedHooksOnly',
    'allowedHttpHookUrls', 'httpHookAllowedEnvVars',
  ],
  'MCP 与插件': [
    'enableAllProjectMcpServers', 'enabledMcpjsonServers', 'disabledMcpjsonServers',
    'allowedMcpServers', 'deniedMcpServers', 'allowManagedMcpServersOnly',
    'enabledPlugins', 'pluginConfigs', 'blockedMarketplaces',
    'strictKnownMarketplaces', 'extraKnownMarketplaces',
    'allowedChannelPlugins', 'pluginTrustMessage',
    'skippedMarketplaces', 'skippedPlugins',
    'channelsEnabled',
  ],
  'Skills 与工作流': [
    'disableBundledSkills', 'disableSkillShellExecution', 'disableWorkflows',
    'skillOverrides', 'maxSkillDescriptionChars', 'skillListingBudgetFraction',
    'strictPluginOnlyCustomization',
  ],
  '认证与凭证': [
    'apiKeyHelper', 'forceLoginMethod', 'forceLoginOrgUUID',
    'awsAuthRefresh', 'awsCredentialExport', 'otelHeadersHelper',
  ],
  '环境变量': ['env'],
  '沙箱': ['sandbox', 'worktree'],
  '更新与版本': [
    'autoUpdatesChannel', 'minimumVersion',
    'disableDeepLinkRegistration',
  ],
  '高级': [
    'agent', 'skipWebFetchPreflight', 'disableAgentView',
    'fileSuggestion', 'defaultShell', 'companyAnnouncements',
    'forceRemoteSettingsRefresh', 'parentSettingsBehavior',
    'wslInheritsWindowsSettings', 'claudeMd',
  ],
}

const knownGrouped = new Set(Object.values(GROUPS).flat())

function getGroup(key: string): string {
  for (const [group, keys] of Object.entries(GROUPS)) {
    if (keys.includes(key)) return group
  }
  return '其他'
}

interface FieldTranslation {
  key: string
  name: string
  desc: string
}

const TRANSLATIONS_STORAGE_KEY = 'cc-space:settings-translations'

const schema = ref<Record<string, SchemaProperty>>({})
const settings = ref<Record<string, unknown>>({})
const translations = ref<Record<string, FieldTranslation>>({})
const hasSchema = ref(false)
const loading = ref(false)
const translating = ref(false)
let loaded = false

function loadCachedTranslations() {
  try {
    const raw = localStorage.getItem(TRANSLATIONS_STORAGE_KEY)
    if (raw) translations.value = JSON.parse(raw)
  } catch { /* ignore */ }
}

function saveCachedTranslations() {
  localStorage.setItem(TRANSLATIONS_STORAGE_KEY, JSON.stringify(translations.value))
}

loadCachedTranslations()

export function useCliSettings() {
  async function load(force = false) {
    if (loaded && !force) return
    loading.value = true
    try {
      const [schemaResp, settingsResp] = await Promise.all([
        invoke<SchemaResponse>('get_settings_schema'),
        invoke<Record<string, unknown>>('get_full_cli_settings'),
      ])
      const props = schemaResp.properties ?? {}
      delete props['$schema']
      schema.value = props
      hasSchema.value = schemaResp.hasSchema
      settings.value = settingsResp
      loaded = true
    } finally {
      loading.value = false
    }
  }

  const groups = computed(() => {
    const result: Record<string, SettingsField[]> = {}
    const seen = new Set<string>()

    for (const key of Object.keys(schema.value)) {
      seen.add(key)
      const group = getGroup(key)
      if (!result[group]) result[group] = []
      result[group].push({
        key,
        schema: schema.value[key],
        value: settings.value[key],
        source: 'schema',
        group,
      })
    }

    for (const key of Object.keys(settings.value)) {
      if (seen.has(key) || key === '$schema') continue
      const group = knownGrouped.has(key) ? getGroup(key) : '未收录'
      if (!result[group]) result[group] = []
      result[group].push({
        key,
        schema: null,
        value: settings.value[key],
        source: 'custom',
        group,
      })
    }

    return result
  })

  const groupOrder = computed(() => {
    const order = Object.keys(GROUPS)
    const extra = Object.keys(groups.value).filter(g => !order.includes(g))
    return [...order.filter(g => groups.value[g]?.length), ...extra]
  })

  async function updateField(key: string, value: unknown) {
    settings.value = { ...settings.value, [key]: value }
    await invoke('update_cli_settings', {
      updates: { [key]: value === undefined ? null : value },
    })
  }

  async function removeField(key: string) {
    const copy = { ...settings.value }
    delete copy[key]
    settings.value = copy
    await invoke('update_cli_settings', { updates: { [key]: null } })
  }

  function stripMarkdownFence(text: string): string {
    return text.replace(/^```(?:json)?\s*\n?/m, '').replace(/\n?```\s*$/m, '').trim()
  }

  async function translateBatch(keys: string[]): Promise<void> {
    const batch = keys.map(k => ({
      key: k,
      description: schema.value[k]?.description?.slice(0, 200) ?? '',
    }))
    const raw = await invoke<string>('translate_settings_fields', {
      fieldsJson: JSON.stringify(batch),
    })
    const cleaned = stripMarkdownFence(raw)
    const parsed: FieldTranslation[] = JSON.parse(cleaned)
    const updated = { ...translations.value }
    for (const item of parsed) {
      if (item.key && item.name) {
        updated[item.key] = item
      }
    }
    translations.value = updated
    saveCachedTranslations()
  }

  async function translateMissing() {
    const allKeys = Object.keys(schema.value)
    const untranslated = allKeys.filter(k => !translations.value[k])
    if (!untranslated.length) return

    translating.value = true
    try {
      const BATCH_SIZE = 20
      for (let i = 0; i < untranslated.length; i += BATCH_SIZE) {
        const chunk = untranslated.slice(i, i + BATCH_SIZE)
        try {
          await translateBatch(chunk)
        } catch (e) {
          console.warn(`[useCliSettings] 批次 ${i}~${i + chunk.length} 翻译失败:`, e)
        }
      }
    } finally {
      translating.value = false
    }
  }

  function getTranslation(key: string): FieldTranslation | null {
    return translations.value[key] ?? null
  }

  async function refreshSchema() {
    await invoke('refresh_settings_schema')
    await load(true)
  }

  return {
    schema,
    settings,
    hasSchema,
    loading,
    translating,
    translations,
    groups,
    groupOrder,
    load,
    updateField,
    removeField,
    refreshSchema,
    translateMissing,
    getTranslation,
  }
}
