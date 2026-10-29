<script setup lang="ts">
import { computed, ref, watch, onBeforeUnmount } from 'vue'
import {
  composerPrefix,
  filterCommands,
  type ArgumentCompletion,
  type CommandArgOption,
  type CommandCatalogContext,
  type SlashCommand,
  type SlashCommandCategory,
} from '@/composables/useSlashCommands'
import type { WorkshopSkill, WorkshopCommand } from '@/types'
import { useI18n } from 'vue-i18n'

const props = defineProps<{
  visible: boolean
  query: string
  position?: { top: number; left: number }
  skills?: WorkshopSkill[]
  commands?: WorkshopCommand[]
  context?: CommandCatalogContext
  /** 非空时进入参数候选模式（命令名已输完、正在输参数） */
  argCompletion?: ArgumentCompletion | null
}>()

const emit = defineEmits<{
  (e: 'select', command: SlashCommand): void
  (e: 'select-arg', command: SlashCommand, option: CommandArgOption): void
  (e: 'close'): void
}>()

const { t } = useI18n()

const filtered = computed<SlashCommand[]>(() =>
  filterCommands(props.query, props.skills, props.commands, props.context),
)

const displayPrefix = computed(() => composerPrefix(props.query) ?? '/')

const argOptions = computed<CommandArgOption[] | null>(() => props.argCompletion?.options ?? null)
/** 当前模式下可选条目数（命令或参数） */
const itemCount = computed(() => argOptions.value?.length ?? filtered.value.length)

const activeIndex = ref(0)

watch(
  () => [props.query, props.visible] as const,
  () => { activeIndex.value = 0 },
)

watch(itemCount, (count) => {
  if (activeIndex.value >= count) {
    activeIndex.value = Math.max(0, count - 1)
  }
})

function selectAt(index: number) {
  const completion = props.argCompletion
  if (completion) {
    const option = completion.options[index]
    if (option) emit('select-arg', completion.command, option)
    return
  }
  const list = filtered.value
  if (index < 0 || index >= list.length) return
  emit('select', list[index])
}

function onKeydown(e: KeyboardEvent) {
  if (!props.visible) return
  const key = e.key
  if (key === 'ArrowDown' || key === 'ArrowUp' || key === 'Enter' || key === 'Escape') {
    if (key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      emit('close')
      return
    }

    if (itemCount.value === 0) {
      if (key === 'ArrowUp' || key === 'ArrowDown') {
        e.preventDefault()
        e.stopPropagation()
      }
      return
    }

    e.preventDefault()
    e.stopPropagation()

    if (key === 'ArrowDown') {
      const len = itemCount.value
      activeIndex.value = (activeIndex.value + 1) % len
    } else if (key === 'ArrowUp') {
      const len = itemCount.value
      activeIndex.value = (activeIndex.value - 1 + len) % len
    } else if (key === 'Enter') {
      selectAt(activeIndex.value)
    }
  }
}

watch(
  () => props.visible,
  (v) => {
    if (v) {
      window.addEventListener('keydown', onKeydown, { capture: true })
    } else {
      window.removeEventListener('keydown', onKeydown, { capture: true } as any)
    }
  },
  { immediate: true },
)

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown, { capture: true } as any)
})

const positionStyle = computed(() => {
  if (!props.position) return undefined
  return {
    top: `${props.position.top}px`,
    left: `${props.position.left}px`,
  }
})

function badgeFor(cat: SlashCommandCategory): string | null {
  if (cat === 'skill') return t('slash.badgeSkill')
  if (cat === 'command') return t('slash.badgeCommand')
  if (cat === 'terminal') return t('slash.badgeTerminal')
  return null
}
</script>

<template>
  <div
    v-if="visible"
    class="slash-panel rounded-md border border-border bg-popover shadow-paper-lifted overflow-hidden"
    :class="position ? 'fixed z-50' : 'absolute z-50'"
    :style="positionStyle"
    role="listbox"
    :aria-label="$t('slash.panel')"
  >
    <div
      v-if="itemCount === 0"
      class="px-3 py-2 text-xs text-muted-foreground"
    >
      {{ $t('slash.noMatch') }}
    </div>

    <ul v-else-if="argOptions && argCompletion" class="py-1 max-h-80 overflow-y-auto">
      <template v-for="(option, i) in argOptions" :key="option.value">
        <li v-if="i > 0 && !!option.secondary !== !!argOptions[i - 1].secondary" class="my-1 border-t border-border" role="separator" />
        <li
          role="option"
          :aria-selected="i === activeIndex"
          class="px-3 py-1.5 cursor-pointer flex items-baseline gap-2 transition-colors"
          :class="i === activeIndex ? 'bg-primary/10' : 'hover:bg-muted'"
          @mouseenter="activeIndex = i"
          @click="selectAt(i)"
        >
          <span class="text-sm shrink-0" :class="option.secondary ? 'text-muted-foreground' : 'text-foreground'">{{ option.label }}</span>
          <span
            v-if="option.value.toLocaleLowerCase() !== option.label.toLocaleLowerCase()"
            class="text-xs text-muted-foreground font-mono truncate"
          >{{ option.value }}</span>
        </li>
      </template>
    </ul>

    <ul v-else class="py-1 max-h-80 overflow-y-auto">
      <li
        v-for="(cmd, i) in filtered"
        :key="cmd.name"
        role="option"
        :aria-selected="i === activeIndex"
        class="px-3 py-1.5 cursor-pointer flex items-baseline gap-2 transition-colors"
        :class="i === activeIndex ? 'bg-primary/10' : 'hover:bg-muted'"
        @mouseenter="activeIndex = i"
        @click="selectAt(i)"
      >
        <span class="text-sm font-mono text-primary shrink-0">{{ displayPrefix }}{{ cmd.name }}</span>
        <span
          v-if="cmd.hasArg && cmd.argHint"
          class="text-xs text-muted-foreground font-mono shrink-0"
        >
          {{ cmd.argHint }}
        </span>
        <span
          v-if="badgeFor(cmd.category)"
          class="slash-badge shrink-0"
        >{{ badgeFor(cmd.category) }}</span>
        <span class="text-xs text-muted-foreground truncate">{{ cmd.hint }}</span>
      </li>
    </ul>

    <div
      v-if="itemCount > 0"
      class="px-3 py-1 border-t border-border text-2xs text-muted-foreground flex items-center gap-3"
    >
      <span><kbd class="kbd">↑↓</kbd> {{ $t('slash.move') }}</span>
      <span><kbd class="kbd">Enter</kbd> {{ $t('slash.select') }}</span>
      <span><kbd class="kbd">Esc</kbd> {{ $t('common.close') }}</span>
    </div>
  </div>
</template>

<style scoped>
.slash-panel {
  min-width: 280px;
  max-width: 420px;
}

.text-2xs {
  font-size: 10px;
  line-height: 1.3;
}

.kbd {
  font-family: var(--font-mono);
  font-size: 10px;
  padding: 0 4px;
  border: 1px solid var(--border);
  border-radius: 3px;
  background: transparent;
  color: var(--muted-foreground);
}

.slash-badge {
  font-size: 9px;
  padding: 0 4px;
  border-radius: 3px;
  border: 1px solid var(--border);
  color: var(--muted-foreground);
  line-height: 1.4;
  white-space: nowrap;
}
</style>
