<script setup lang="ts">
import { ref, onMounted, watch } from 'vue'
import {
  useChannels,
  refreshChannels,
  OFFICIAL_CHANNEL_ID,
  type ChannelInfo,
} from '@/composables/useChannels'
import { useUiState } from '@/composables/useUiState'
import { useConfirm } from '@/composables/useConfirm'
import { useNotifications } from '@/composables/useNotifications'
import { useHomeStats } from '@/composables/useHomeStats'
import ChannelForm from '@/components/settings/ChannelForm.vue'
import DiagnosisCard from '@/components/home/DiagnosisCard.vue'

/**
 * 设置域(第一期只有渠道管理)。
 * 配置事实源是 ~/.claude/cc-space/ 下的文件(用户可手编、终端可复用),
 * 本页只是它的 GUI——每次进入都重读,不持有权威状态。
 */
const { channels, defaultChannelId, deleteChannel, setDefaultChannel, revealChannelsDir } =
  useChannels()
const { activeSection } = useUiState()
const { diag, diagLoading, diagError, diagAt, retryDiag, ensureLoaded } = useHomeStats()
const { confirm } = useConfirm()
const { notifyTransient } = useNotifications()

/** 表单状态:null = 收起;'new' = 新建;ChannelInfo = 编辑 */
const editing = ref<'new' | ChannelInfo | null>(null)

onMounted(() => refreshChannels())

// v-show 常驻挂载,onMounted 只触发一次:每次切到设置页重读,反映他处(下拉/手编)的改动
watch(activeSection, (s) => {
  if (s === 'settings') {
    refreshChannels()
    ensureLoaded()
  }
})

async function onDelete(ch: ChannelInfo) {
  const ok = await confirm(
    `删除渠道「${ch.name}」?将删除 channels/${ch.id}.json 文件,不可恢复。`,
    '删除',
  )
  if (!ok) return
  try {
    await deleteChannel(ch.id)
    if (editing.value !== 'new' && editing.value?.id === ch.id) editing.value = null
    notifyTransient('渠道已删除')
  } catch (e) {
    notifyTransient('删除失败', String(e))
  }
}

async function onDefaultChange(e: Event) {
  const value = (e.target as HTMLSelectElement).value
  try {
    await setDefaultChannel(value === OFFICIAL_CHANNEL_ID ? null : value)
  } catch (err) {
    notifyTransient('设置默认渠道失败', String(err))
    await refreshChannels()
  }
}

async function onReveal() {
  try {
    await revealChannelsDir()
  } catch (e) {
    notifyTransient('打开目录失败', String(e))
  }
}

function onSaved() {
  editing.value = null
  notifyTransient('渠道已保存')
}
</script>

<template>
  <div class="h-full overflow-y-auto bg-background">
    <div class="max-w-2xl mx-auto px-6 py-6">
      <h1 class="text-base font-semibold mb-5 flex items-center gap-2">
        <span class="i-carbon-settings w-4 h-4" />设置
      </h1>

      <!-- 渠道管理 -->
      <section>
        <h2 class="text-sm font-medium mb-1">渠道</h2>
        <p class="text-xs text-muted-foreground mb-3 leading-relaxed">
          每个渠道是一份标准 Claude Code settings 格式的 JSON 文件(<span class="font-mono">~/.claude/cc-space/channels/&lt;id&gt;.json</span>),
          发送消息时按会话选择经 <span class="font-mono">--settings</span> 注入;
          终端里也可直接 <span class="font-mono">claude --settings &lt;文件&gt;</span> 复用同一渠道。
          模型映射、自定义请求头等高级 env 可直接手编文件,本页改动不会抹掉手编字段。
        </p>

        <!-- 默认渠道 -->
        <div class="flex items-center gap-2 mb-3">
          <span class="text-xs text-muted-foreground shrink-0">新会话默认渠道</span>
          <select
            :value="defaultChannelId ?? OFFICIAL_CHANNEL_ID"
            class="px-2 py-1 text-xs rounded-md border border-border bg-popover text-foreground
                   focus:outline-none focus:border-ring"
            @change="onDefaultChange"
          >
            <option :value="OFFICIAL_CHANNEL_ID">官方(不注入,沿用登录态)</option>
            <option v-for="c in channels" :key="c.id" :value="c.id">{{ c.name }}</option>
          </select>
        </div>

        <!-- 渠道列表 -->
        <div class="flex flex-col gap-2 mb-3">
          <div
            v-for="c in channels"
            :key="c.id"
            class="rounded-md border border-border bg-card px-3 py-2 flex items-center gap-3"
          >
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-1.5 text-xs">
                <span class="font-medium truncate">{{ c.name }}</span>
                <span class="text-muted-foreground font-mono">{{ c.id }}</span>
                <span v-if="c.isDefault" class="channel-chip">默认</span>
                <span v-if="!c.valid" class="channel-chip text-destructive border-destructive">JSON 解析失败</span>
              </div>
              <div class="text-[11px] text-muted-foreground truncate mt-0.5 font-mono">
                {{ c.baseUrl ?? '(未配置 ANTHROPIC_BASE_URL)' }}
                <span v-if="c.authTokenMasked" class="ml-1.5">token {{ c.authTokenMasked }}</span>
              </div>
              <div v-if="c.extraEnvKeys.length || c.note" class="text-[11px] text-muted-foreground truncate mt-0.5">
                <span v-if="c.note">{{ c.note }}</span>
                <span v-if="c.extraEnvKeys.length" :class="{ 'ml-1.5': c.note }">
                  高级 env: {{ c.extraEnvKeys.join('、') }}
                </span>
              </div>
            </div>
            <button
              class="shrink-0 p-1 rounded text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
              title="编辑"
              @click="editing = c"
            >
              <span class="i-carbon-edit w-3.5 h-3.5" />
            </button>
            <button
              class="shrink-0 p-1 rounded text-muted-foreground hover:text-destructive hover:bg-destructive/10 transition-colors"
              title="删除"
              @click="onDelete(c)"
            >
              <span class="i-carbon-trash-can w-3.5 h-3.5" />
            </button>
          </div>

          <p v-if="channels.length === 0" class="text-xs text-muted-foreground py-2">
            尚无渠道。新增后即可在会话顶栏按会话切换。
          </p>
        </div>

        <!-- 表单(新建/编辑) -->
        <ChannelForm
          v-if="editing"
          :key="editing === 'new' ? '__new__' : editing.id"
          :channel="editing === 'new' ? null : editing"
          class="mb-3"
          @saved="onSaved"
          @cancel="editing = null"
        />

        <div class="flex items-center gap-2">
          <button
            v-if="!editing"
            class="px-2.5 py-1 text-xs rounded-md bg-primary text-primary-foreground hover:shadow-paper transition-shadow"
            @click="editing = 'new'"
          >
            + 新增渠道
          </button>
          <button
            class="px-2.5 py-1 text-xs rounded-md text-muted-foreground border border-border hover:text-foreground hover:bg-muted transition-colors"
            @click="onReveal"
          >
            打开配置目录
          </button>
        </div>
      </section>

      <!-- 兼容性诊断 -->
      <section class="mt-8">
        <h2 class="text-sm font-medium mb-1">兼容性诊断</h2>
        <p class="text-xs text-muted-foreground mb-3 leading-relaxed">
          扫描所有会话文件，检查记录类型和工具的解析覆盖率。未识别的类型和兜底工具会在这里列出，用于开发排查。
        </p>
        <DiagnosisCard
          :diag="diag"
          :loading="diagLoading"
          :error="diagError"
          :scanned-at="diagAt"
          @retry="retryDiag"
        />
      </section>
    </div>
  </div>
</template>

<style scoped>
.channel-chip {
  padding: 0 4px;
  font-size: 9.5px;
  line-height: 16px;
  border: 1px solid var(--primary);
  color: var(--primary);
  border-radius: calc(var(--radius) - 2px);
  flex-shrink: 0;
}
</style>
