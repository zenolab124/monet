<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, nextTick, watch } from 'vue'
import { GridStack } from 'gridstack'
import 'gridstack/dist/gridstack.min.css'

export interface CardDef {
  id: string
  x: number
  y: number
  w: number
  h: number
  hidden?: boolean
}

const props = defineProps<{
  cards: CardDef[]
}>()

const emit = defineEmits<{
  'delete-widget': [id: string]
}>()

const STORAGE_KEY = 'cc-space-home-layout'

const gridEl = ref<HTMLElement>()
const editing = ref(false)
const hiddenIds = ref<Set<string>>(new Set())

let grid: GridStack | null = null

function savedLayout(): Map<string, CardDef> | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return null
    const arr = JSON.parse(raw) as CardDef[]
    if (!Array.isArray(arr)) return null
    return new Map(arr.map(c => [c.id, c]))
  } catch { return null }
}

function getLayout(): CardDef[] {
  const saved = savedLayout()
  if (!saved) return props.cards
  return props.cards.map(c => {
    const s = saved.get(c.id)
    return s ? { ...c, x: s.x, y: s.y, w: s.w, h: s.h, hidden: s.hidden } : c
  })
}

function loadHiddenFromLayout(cards: CardDef[]): Set<string> {
  return new Set(cards.filter(c => c.hidden).map(c => c.id))
}

const layout = ref<CardDef[]>([])

const visibleCards = computed(() =>
  editing.value ? layout.value : layout.value.filter(c => !hiddenIds.value.has(c.id))
)

function persistLayout() {
  if (!grid) return
  const nodeMap = new Map<string, { x: number; y: number; w: number; h: number }>()
  for (const el of grid.getGridItems()) {
    const node = el.gridstackNode
    if (!node?.id) continue
    nodeMap.set(node.id as string, {
      x: node.x ?? 0, y: node.y ?? 0,
      w: node.w ?? 1, h: node.h ?? 1,
    })
  }
  const result: CardDef[] = layout.value.map(c => ({
    ...c,
    ...(nodeMap.get(c.id) ?? {}),
    hidden: hiddenIds.value.has(c.id) || undefined,
  }))
  if (result.length) localStorage.setItem(STORAGE_KEY, JSON.stringify(result))
}

async function rebuildGrid() {
  if (!grid || !gridEl.value) return
  grid.removeAll(false)
  await nextTick()
  const items = gridEl.value.querySelectorAll('.grid-stack-item')
  grid.batchUpdate()
  items.forEach(el => grid!.makeWidget(el as HTMLElement))
  grid.batchUpdate(false)
  grid.float(false)
  grid.float(true)
}

function toggleEdit() {
  if (!grid) return

  if (!editing.value) {
    editing.value = true
    nextTick(() => {
      if (!grid || !gridEl.value) return
      const items = gridEl.value.querySelectorAll('.grid-stack-item:not(.ui-draggable)')
      items.forEach(el => grid!.makeWidget(el as HTMLElement))
      grid!.enableMove(true)
      grid!.enableResize(true)
    })
  } else {
    grid.enableMove(false)
    grid.enableResize(false)
    persistLayout()
    editing.value = false
    nextTick(() => rebuildGrid())
  }
}

function toggleCardVisibility(id: string) {
  if (hiddenIds.value.has(id)) {
    hiddenIds.value.delete(id)
  } else {
    hiddenIds.value.add(id)
  }
  hiddenIds.value = new Set(hiddenIds.value)
}

function deleteWidget(id: string) {
  emit('delete-widget', id)
  removeWidget(id)
}

async function resetLayout() {
  if (!grid || !gridEl.value) return
  localStorage.removeItem(STORAGE_KEY)
  hiddenIds.value = new Set()
  layout.value = props.cards.map(c => ({ ...c }))

  // 先在编辑模式下重建（所有卡片可见），再退出编辑
  await nextTick()
  await rebuildGrid()

  grid!.batchUpdate()
  for (const card of props.cards) {
    const el = gridEl.value!.querySelector(`[gs-id="${card.id}"]`) as HTMLElement | null
    if (el) grid!.update(el, { x: card.x, y: card.y, w: card.w, h: card.h })
  }
  grid!.batchUpdate(false)

  grid!.enableMove(false)
  grid!.enableResize(false)
  editing.value = false
}

function addWidget(card: CardDef) {
  if (!grid || !gridEl.value) return
  layout.value.push(card)
  if (!hiddenIds.value.has(card.id)) {
    nextTick(() => {
      const el = gridEl.value?.querySelector(`[gs-id="${card.id}"]`) as HTMLElement | null
      if (el) grid!.makeWidget(el)
    })
  }
}

function removeWidget(id: string) {
  if (!grid || !gridEl.value) return
  const el = gridEl.value.querySelector(`[gs-id="${id}"]`) as HTMLElement | null
  if (el) grid.removeWidget(el, true, false)
  layout.value = layout.value.filter(c => c.id !== id)
  hiddenIds.value.delete(id)
  persistLayout()
}

function isCustomWidget(id: string): boolean {
  return id.startsWith('w:')
}

onMounted(async () => {
  layout.value = getLayout()
  hiddenIds.value = loadHiddenFromLayout(layout.value)
  await nextTick()
  if (!gridEl.value) return

  grid = GridStack.init({
    column: 4,
    cellHeight: 180,
    float: true,
    animate: true,
    margin: 7,
    disableDrag: true,
    disableResize: true,
  }, gridEl.value)

  grid.on('change', () => {
    if (editing.value) persistLayout()
  })
})

onBeforeUnmount(() => {
  if (grid) {
    persistLayout()
    grid.destroy(false)
    grid = null
  }
})

watch(editing, (val) => {
  gridEl.value?.classList.toggle('gs-editing', val)
})

watch(() => props.cards, (newCards) => {
  if (!grid) return
  const currentIds = new Set(layout.value.map(c => c.id))
  const newIds = new Set(newCards.map(c => c.id))
  const added = newCards.filter(c => !currentIds.has(c.id))
  const removed = [...currentIds].filter(id => !newIds.has(id))
  for (const id of removed) removeWidget(id)
  for (const card of added) addWidget(card)
}, { deep: true })

defineExpose({ toggleEdit, resetLayout, editing, addWidget, removeWidget })
</script>

<template>
  <div ref="gridEl" class="grid-stack home-grid">
    <div
      v-for="card in visibleCards"
      :key="card.id"
      class="grid-stack-item"
      :class="{ 'gs-hidden': hiddenIds.has(card.id) }"
      :gs-id="card.id"
      :gs-x="card.x"
      :gs-y="card.y"
      :gs-w="card.w"
      :gs-h="card.h"
    >
      <div class="grid-stack-item-content">
        <slot v-if="$slots[card.id]" :name="card.id" />
        <slot v-else name="custom-widget" :card="card" />

        <div v-if="editing" class="card-edit-overlay">
          <button
            class="icon-btn"
            v-tooltip="hiddenIds.has(card.id) ? $t('home.grid.show') : $t('home.grid.hide')"
            @click.stop="toggleCardVisibility(card.id)"
          >
            <span class="w-3.5 h-3.5" :class="hiddenIds.has(card.id) ? 'i-carbon-view' : 'i-carbon-view-off'" />
          </button>
          <button
            v-if="isCustomWidget(card.id)"
            class="icon-btn delete"
            v-tooltip="$t('common.delete')"
            @click.stop="deleteWidget(card.id)"
          >
            <span class="i-carbon-trash-can w-3.5 h-3.5" />
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style>
.home-grid .grid-stack-item-content {
  overflow: hidden;
  border-radius: var(--radius);
  position: relative;
}

.home-grid .grid-stack-item.gs-hidden > .grid-stack-item-content {
  opacity: 0.3;
}

.home-grid .grid-stack-placeholder > .placeholder-content {
  background: color-mix(in srgb, var(--accent) 30%, transparent) !important;
  border: 2px dashed color-mix(in srgb, var(--border) 60%, transparent) !important;
  border-radius: var(--radius) !important;
}

.home-grid .ui-resizable-handle {
  opacity: 0;
  transition: opacity 0.15s;
}
.home-grid.gs-editing .ui-resizable-handle {
  opacity: 0.6;
}

.home-grid.gs-editing .grid-stack-item {
  cursor: grab;
}
.home-grid.gs-editing .grid-stack-item.ui-draggable-dragging {
  cursor: grabbing;
}

.home-grid .ui-draggable-dragging > .grid-stack-item-content,
.home-grid .ui-resizable-resizing > .grid-stack-item-content {
  box-shadow: var(--shadow-paper-lifted) !important;
  opacity: 0.9;
}

.card-edit-overlay {
  position: absolute;
  top: 6px;
  right: 6px;
  display: flex;
  gap: 4px;
  z-index: 10;
}

</style>
