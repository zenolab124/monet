import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import i18n from '../locales'

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

const GROUP_KEYS: Record<string, string[]> = {
  'modelAndReasoning': [
    'model', 'effortLevel', 'alwaysThinkingEnabled', 'fastMode',
    'fastModePerSessionOptIn', 'availableModels', 'enforceAvailableModels',
    'modelOverrides', 'fallbackModel',
  ],
  'languageAndOutput': [
    'language', 'outputStyle',
  ],
  'permissions': [
    'permissions', 'allowManagedPermissionRulesOnly',
    'autoMode', 'useAutoModeDuringPlan',
    'skipDangerousModePermissionPrompt',
  ],
  'uiAndDisplay': [
    'showThinkingSummaries', 'showTurnDuration', 'showClearContextOnPlanAccept',
    'spinnerTipsEnabled', 'spinnerTipsOverride', 'spinnerVerbs',
    'prefersReducedMotion', 'feedbackSurveyRate',
    'tui', 'viewMode', 'terminalProgressBarEnabled',
    'statusLine', 'subagentStatusLine',
    'voiceEnabled', 'teammateMode',
  ],
  'fileAndMemory': [
    'autoMemoryEnabled', 'autoMemoryDirectory',
    'cleanupPeriodDays', 'respectGitignore', 'plansDirectory',
    'claudeMdExcludes',
  ],
  'gitAndAttribution': [
    'attribution', 'includeCoAuthoredBy', 'includeGitInstructions',
    'prUrlTemplate',
  ],
  'hooksAndAutomation': [
    'hooks', 'disableAllHooks', 'allowManagedHooksOnly',
    'allowedHttpHookUrls', 'httpHookAllowedEnvVars',
  ],
  'mcpAndPlugins': [
    'enableAllProjectMcpServers', 'enabledMcpjsonServers', 'disabledMcpjsonServers',
    'allowedMcpServers', 'deniedMcpServers', 'allowManagedMcpServersOnly',
    'enabledPlugins', 'pluginConfigs', 'blockedMarketplaces',
    'strictKnownMarketplaces', 'extraKnownMarketplaces',
    'allowedChannelPlugins', 'pluginTrustMessage',
    'skippedMarketplaces', 'skippedPlugins',
    'channelsEnabled',
  ],
  'skillsAndWorkflows': [
    'disableBundledSkills', 'disableSkillShellExecution', 'disableWorkflows',
    'skillOverrides', 'maxSkillDescriptionChars', 'skillListingBudgetFraction',
    'strictPluginOnlyCustomization',
  ],
  'authAndCredentials': [
    'apiKeyHelper', 'forceLoginMethod', 'forceLoginOrgUUID',
    'awsAuthRefresh', 'awsCredentialExport', 'otelHeadersHelper',
  ],
  'envVars': ['env'],
  'sandbox': ['sandbox', 'worktree'],
  'updateAndVersion': [
    'autoUpdatesChannel', 'minimumVersion',
    'disableDeepLinkRegistration',
  ],
  'advanced': [
    'agent', 'skipWebFetchPreflight', 'disableAgentView',
    'fileSuggestion', 'defaultShell', 'companyAnnouncements',
    'forceRemoteSettingsRefresh', 'parentSettingsBehavior',
    'wslInheritsWindowsSettings', 'claudeMd',
  ],
}

function groupLabel(groupKey: string): string {
  const key = `settings.cliGroups.${groupKey}`
  const result = i18n.global.t(key)
  return result !== key ? result : groupKey
}

const knownGrouped = new Set(Object.values(GROUP_KEYS).flat())

function getGroup(key: string): string {
  for (const [group, keys] of Object.entries(GROUP_KEYS)) {
    if (keys.includes(key)) return groupLabel(group)
  }
  return groupLabel('other')
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
      const group = knownGrouped.has(key) ? getGroup(key) : groupLabel('uncategorized')
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
    const order = Object.keys(GROUP_KEYS).map(groupLabel)
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
