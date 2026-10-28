<script setup lang="ts">
import { onMounted, ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

interface ProtocolSample {
  cli_version: string
  kind: string
  subtype: string | null
  keys: Record<string, string>
  first_seen: number
  count: number
}

interface ProtocolDiagnostics {
  cli_version: string | null
  verified_version: string
  samples: ProtocolSample[]
}

const info = ref<ProtocolDiagnostics | null>(null)

/** 数字段逐段比较；非数字段视为 0 */
function compareVersions(a: string, b: string): number {
  const pa = a.split('.').map(part => Number.parseInt(part, 10) || 0)
  const pb = b.split('.').map(part => Number.parseInt(part, 10) || 0)
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const diff = (pa[i] ?? 0) - (pb[i] ?? 0)
    if (diff !== 0) return diff
  }
  return 0
}

const newerThanVerified = computed(() => {
  const cli = info.value?.cli_version
  return !!cli && compareVersions(cli, info.value!.verified_version) > 0
})

/** 只列本机当前版本下出现的采样，其余版本的旧采样保留在文件里 */
const currentSamples = computed(() => {
  const cli = info.value?.cli_version
  const samples = info.value?.samples ?? []
  return cli ? samples.filter(sample => sample.cli_version === cli) : samples
})

function label(sample: ProtocolSample): string {
  return sample.subtype ? `${sample.kind}/${sample.subtype}` : sample.kind
}

onMounted(async () => {
  try {
    info.value = await invoke<ProtocolDiagnostics>('get_claude_protocol_diagnostics')
  } catch { /* 诊断不可用时不展示内容 */ }
})
</script>

<template>
  <div class="env-card">
    <div class="flex items-center gap-2">
      <span class="i-carbon-data-structured w-3.5 h-3.5 text-muted-foreground" />
      <span class="text-[11.5px] font-medium">{{ t('settings.protocol.title') }}</span>
    </div>
    <div v-if="info" class="mt-1 grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-[10.5px]">
      <span class="text-muted-foreground">{{ t('settings.protocol.cliVersion') }}</span>
      <span class="font-mono">{{ info.cli_version ?? t('settings.protocol.unknown') }}</span>
      <span class="text-muted-foreground">{{ t('settings.protocol.verifiedVersion') }}</span>
      <span class="font-mono">{{ info.verified_version }}</span>
    </div>
    <p v-if="newerThanVerified" class="text-[10.5px] text-accent mt-1 leading-snug">
      {{ t('settings.protocol.newerThanVerified') }}
    </p>
    <p class="text-[10.5px] text-muted-foreground mt-2">{{ t('settings.protocol.samples') }}</p>
    <p v-if="!currentSamples.length" class="text-[10.5px] text-muted-foreground/80 mt-0.5">
      {{ t('settings.protocol.empty') }}
    </p>
    <ul v-else class="mt-0.5 space-y-0.5 max-h-48 overflow-y-auto">
      <li v-for="sample in currentSamples" :key="`${sample.cli_version}/${label(sample)}`" class="text-[10.5px]">
        <span class="font-mono">{{ label(sample) }}</span>
        <span class="text-muted-foreground"> · {{ t('settings.protocol.count', { count: sample.count }) }}</span>
        <span class="env-path block">{{ t('settings.protocol.fields') }}: {{ Object.keys(sample.keys).join(', ') }}</span>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.env-card {
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 8px 10px;
  margin-bottom: 8px;
  background: var(--card);
}
.env-path {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 10.5px;
  color: var(--muted-foreground);
  word-break: break-all;
}
</style>
