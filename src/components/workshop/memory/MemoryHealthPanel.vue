<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { useMemory } from '@/composables/useMemory'

/**
 * 记忆体检面板（v2.9.0 FR-008）：
 * 三组问题：孤儿/悬空/断链，各附建议文案。
 * 悬空组附「打开 MEMORY.md」按钮。
 */

const { t } = useI18n()
const { orphanFiles, danglingRefs, brokenWikiLinks, openMemoryIndex, currentProject } = useMemory()
</script>

<template>
  <div class="health-panel">
    <div class="hp-title">{{ t('memory.healthTitle') }}</div>

    <!-- 孤儿文件 -->
    <div v-if="orphanFiles.length > 0" class="health-group">
      <div class="health-group-title">
        {{ t('memory.orphanFiles') }}（{{ orphanFiles.length }}）
      </div>
      <div v-for="entry in orphanFiles" :key="entry.file" class="health-item">
        <span class="i-carbon-document w-3 h-3 text-amber flex-shrink-0" />
        <div>
          <div class="mono">{{ entry.file }}</div>
          <div class="suggest">{{ t('memory.orphanSuggest') }}</div>
        </div>
      </div>
    </div>

    <!-- 悬空引用 -->
    <div v-if="danglingRefs.length > 0" class="health-group">
      <div class="health-group-title">
        {{ t('memory.danglingRefs') }}（{{ danglingRefs.length }}）
      </div>
      <div v-for="ref in danglingRefs" :key="ref" class="health-item">
        <span class="i-carbon-warning w-3 h-3 text-amber flex-shrink-0" />
        <div>
          <div class="mono">{{ ref }}</div>
          <div class="suggest">{{ t('memory.danglingSuggest') }}</div>
          <button class="open-btn" @click="openMemoryIndex">
            <span class="i-carbon-launch w-2.5 h-2.5" />
            {{ t('memory.openMemoryMd') }}
          </button>
        </div>
      </div>
    </div>

    <!-- Wiki-link 断链 -->
    <div v-if="brokenWikiLinks.length > 0" class="health-group">
      <div class="health-group-title">
        {{ t('memory.brokenLinks') }}（{{ brokenWikiLinks.length }}）
      </div>
      <div v-for="link in brokenWikiLinks" :key="link.sourceFile + link.slug" class="health-item">
        <span class="i-carbon-warning w-3 h-3 text-amber flex-shrink-0" />
        <div>
          <div class="mono-text">
            {{ link.sourceFile }} {{ t('memory.brokenLinkRef') }}
            <span class="wikilink-broken">[[{{ link.slug }}]]</span>
          </div>
          <div class="suggest">{{ t('memory.brokenLinkSuggest') }}</div>
        </div>
      </div>
    </div>

    <!-- 全部为空 -->
    <div
      v-if="orphanFiles.length === 0 && danglingRefs.length === 0 && brokenWikiLinks.length === 0"
      class="all-clear"
    >
      <span class="i-carbon-checkmark w-3 h-3" />
      {{ t('memory.allClear') }}
    </div>

    <!-- legacyIndex 提示 -->
    <div v-if="currentProject?.legacyIndex" class="legacy-note">
      {{ t('memory.legacyIndexNote') }}
    </div>
  </div>
</template>

<style scoped>
.health-panel {
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  box-shadow: var(--shadow-paper);
  margin: 0 14px 10px;
  padding: 10px 12px;
  font-size: 11px;
}
.hp-title {
  font-weight: 600;
  margin-bottom: 6px;
  font-size: 11.5px;
}
.health-group {
  margin-bottom: 8px;
}
.health-group:last-child {
  margin-bottom: 0;
}
.health-group-title {
  font-size: 10px;
  font-weight: 600;
  color: var(--muted-foreground);
  margin-bottom: 4px;
}
.health-item {
  padding: 4px 0;
  display: flex;
  align-items: flex-start;
  gap: 6px;
  color: var(--foreground);
}
.mono {
  font-family: var(--font-mono);
  font-size: 10px;
  color: var(--muted-foreground);
}
.mono-text {
  font-size: 10.5px;
}
.suggest {
  font-size: 10.5px;
  color: var(--muted-foreground);
  margin-top: 1px;
}
.wikilink-broken {
  color: var(--destructive);
  text-decoration: underline;
  text-decoration-style: dashed;
  text-underline-offset: 2px;
  cursor: default;
  opacity: 0.7;
}
.open-btn {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  font-size: 10px;
  padding: 2px 8px;
  margin-top: 3px;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: var(--card);
  cursor: pointer;
  color: var(--foreground);
}
.open-btn:hover {
  background: var(--muted);
}
.text-amber {
  color: var(--amber, oklch(0.62 0.14 70));
}
.all-clear {
  display: flex;
  align-items: center;
  gap: 5px;
  color: var(--mem-project, var(--primary));
  font-size: 11px;
}
.legacy-note {
  margin-top: 8px;
  font-size: 10px;
  color: var(--amber, oklch(0.62 0.14 70));
  background: var(--amber-bg, oklch(0.92 0.04 70));
  padding: 4px 8px;
  border-radius: var(--radius);
}
</style>
