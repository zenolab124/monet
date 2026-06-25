<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'

const THEME_VARS = [
  '--background', '--foreground', '--card', '--card-foreground',
  '--primary', '--primary-foreground', '--secondary', '--secondary-foreground',
  '--muted', '--muted-foreground', '--accent', '--accent-foreground',
  '--border', '--destructive', '--radius',
] as const

const props = defineProps<{
  name: string
  html: string
}>()

const themeVersion = ref(0)

function getThemeVars(): string {
  const root = document.documentElement
  const style = getComputedStyle(root)
  return THEME_VARS
    .map(v => `${v}: ${style.getPropertyValue(v).trim()};`)
    .filter(line => !line.endsWith(': ;'))
    .join('\n    ')
}

const srcdoc = computed(() => {
  void themeVersion.value
  const themeVars = getThemeVars()
  return `<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  :root { ${themeVars} }
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
    font-size: 13px;
    color: var(--foreground, #2c2c2c);
    background: transparent;
    overflow: hidden;
    padding: 6px 12px 10px;
  }
</style>
</head>
<body>${props.html}</body>
</html>`
})

let observer: MutationObserver | null = null

onMounted(() => {
  observer = new MutationObserver(() => { themeVersion.value++ })
  observer.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ['class', 'data-theme'],
  })
})

onBeforeUnmount(() => {
  observer?.disconnect()
})
</script>

<template>
  <div class="widget-iframe-card">
    <div class="widget-header">
      <span class="i-carbon-dashboard w-3.5 h-3.5 text-primary" />
      <span class="widget-name">{{ name }}</span>
    </div>
    <iframe
      :srcdoc="srcdoc"
      sandbox="allow-scripts"
      class="widget-frame"
    />
  </div>
</template>

<style scoped>
.widget-iframe-card {
  height: 100%;
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  box-shadow: var(--shadow-paper);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.widget-header {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 12px 0;
  flex-shrink: 0;
}

.widget-name {
  font-size: 12px;
  font-weight: 600;
  color: var(--foreground);
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.widget-frame {
  flex: 1;
  border: none;
  width: 100%;
  background: transparent;
}
</style>
