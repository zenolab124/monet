<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { useI18n } from 'vue-i18n'
import { projectHost, useProjects } from '@/composables/useProjects'
import { sameInstance } from '@/engines/identity'
import type { EngineDescriptor } from '@/engines/types'
import { fileName } from '@/utils/path'

/** 空白新建第二步:在所选实例的主机上选工作目录。远端不浏览文件夹,只给最近目录与手输路径(校验存在后才能开始) */
const props = defineProps<{
  descriptor: EngineDescriptor
  busy: boolean
}>()
const emit = defineEmits<{
  (e: 'pick', cwd: string): void
  (e: 'back'): void
}>()

const { t } = useI18n()
const { projects } = useProjects()
const remote = computed(() => props.descriptor.capabilities.facets.remoteWorkspace === true)

const recentDirs = computed(() => {
  const dirs = new Map<string, number>()
  for (const project of projects.value) {
    const host = projectHost(project)
    const onThisHost = remote.value
      ? !!host && sameInstance(host.instance, props.descriptor.instance)
      : !host
    const cwd = project.sessions.find(session => session.cwd)?.cwd ?? project.source_path
    if (!onThisHost || !cwd) continue
    dirs.set(cwd, Math.max(dirs.get(cwd) ?? 0, project.last_active ?? 0))
  }
  return [...dirs.entries()].sort((a, b) => b[1] - a[1]).map(([cwd]) => cwd).slice(0, 8)
})

type CheckState = 'idle' | 'checking' | 'ok' | 'missing' | 'invalid' | 'failed'
const manualPath = ref('')
const check = ref<CheckState>('idle')
const checkError = ref('')
let checkRevision = 0
watch(manualPath, () => {
  checkRevision++
  check.value = 'idle'
})

function normalizedPath(): string {
  const value = manualPath.value.trim()
  return value.length > 1 ? value.replace(/[\\/]+$/, '') || value : value
}

async function checkPath() {
  const path = normalizedPath()
  if (!path) return
  const revision = ++checkRevision
  check.value = 'checking'
  try {
    const exists = await invoke<boolean>('check_remote_directory', { instance: props.descriptor.instance, path })
    if (revision === checkRevision) check.value = exists ? 'ok' : 'missing'
  } catch (cause) {
    if (revision !== checkRevision) return
    const message = String(cause)
    check.value = message.includes('REMOTE_PATH_NOT_ABSOLUTE') ? 'invalid' : 'failed'
    checkError.value = message
  }
}

async function pickLocalFolder() {
  const selected = await open({ directory: true, multiple: false })
  if (typeof selected === 'string' && selected) emit('pick', selected)
}

const host = computed(() => props.descriptor.displayName)
const checkText = computed(() => ({
  idle: t('workbench.newTask.checkHint'),
  checking: t('workbench.newTask.checking', { host: host.value }),
  ok: t('workbench.newTask.exists'),
  missing: t('workbench.newTask.missing', { host: host.value }),
  invalid: t('workbench.newTask.notAbsolute'),
  failed: t('workbench.newTask.checkFailed', { error: checkError.value }),
})[check.value])
</script>

<template>
  <section class="h-full overflow-y-auto bg-background/35 px-5 py-8">
    <div class="mx-auto w-full max-w-md">
      <button class="mb-3 text-[11px] text-muted-foreground hover:text-foreground" :disabled="busy" @click="emit('back')">
        ← {{ $t('workbench.newTask.back') }}
      </button>
      <h2 class="text-base font-semibold text-foreground">{{ $t('workbench.newTask.directoryTitle') }}</h2>
      <p class="mt-1 text-xs text-muted-foreground">{{ $t('workbench.newTask.directoryDescription', { engine: host }) }}</p>

      <div class="mt-4 mb-1.5 text-[11px] text-muted-foreground">{{ $t('workbench.newTask.recent') }}</div>
      <div class="rounded border border-border bg-card shadow-paper divide-y divide-border">
        <button
          v-for="cwd in recentDirs"
          :key="cwd"
          class="w-full px-3 py-2 text-left hover:bg-muted transition-colors disabled:opacity-50"
          :disabled="busy"
          @click="emit('pick', cwd)"
        >
          <span class="block text-xs text-foreground truncate">{{ fileName(cwd) }}</span>
          <span class="block text-[10px] text-muted-foreground truncate">{{ cwd }}</span>
        </button>
        <div v-if="!recentDirs.length" class="px-3 py-2 text-xs text-muted-foreground">{{ $t('workbench.newTask.none') }}</div>
      </div>

      <template v-if="remote">
        <div class="mt-4 mb-1.5 text-[11px] text-muted-foreground">{{ $t('workbench.newTask.remotePath') }}</div>
        <div class="flex gap-1.5">
          <input
            v-model="manualPath"
            class="flex-1 min-w-0 rounded border border-input bg-card px-2 py-1 text-xs text-foreground outline-none focus:border-ring"
            :placeholder="$t('workbench.newTask.remotePlaceholder')"
            spellcheck="false"
            @keydown.enter="check === 'ok' ? emit('pick', normalizedPath()) : checkPath()"
          >
          <button class="rounded border border-border bg-card shadow-paper hover:shadow-paper-lifted transition-shadow disabled:opacity-45 disabled:shadow-none disabled:cursor-not-allowed text-xs px-2" :disabled="busy || !manualPath.trim() || check === 'checking'" @click="checkPath">
            {{ $t('workbench.newTask.check') }}
          </button>
          <button class="rounded border border-primary bg-primary text-primary-foreground shadow-paper disabled:opacity-45 disabled:shadow-none disabled:cursor-not-allowed text-xs px-2" :disabled="busy || check !== 'ok'" @click="emit('pick', normalizedPath())">
            {{ $t('workbench.newTask.start') }}
          </button>
        </div>
        <p
          class="mt-1.5 text-[11px] break-all"
          :class="check === 'ok' ? 'text-primary' : ['missing', 'invalid', 'failed'].includes(check) ? 'text-destructive' : 'text-muted-foreground'"
        >{{ checkText }}</p>
      </template>
      <button v-else class="rounded border border-border bg-card shadow-paper hover:shadow-paper-lifted transition-shadow disabled:opacity-45 disabled:shadow-none disabled:cursor-not-allowed mt-4 text-xs px-2.5 py-1" :disabled="busy" @click="pickLocalFolder">
        {{ $t('workbench.newTask.pickFolder') }}
      </button>
    </div>
  </section>
</template>
