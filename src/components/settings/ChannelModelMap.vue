<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import {
  MODEL_ROLES,
  parseModelEnv,
  buildModelEnv,
  hasAnyMapping,
  type ModelMapForm,
  type ModelRole,
} from '@/utils/modelEnv'

/**
 * 渠道「模型角色映射」高级区(参照 CC Switch)。
 * 四角色行(Fable/Opus/Sonnet/Haiku)+ 自定义行。
 * 表单 ↔ env 翻译走 utils/modelEnv 纯函数;每次变更 emit 构建后的 env 给父组件。
 * 默认模型(ANTHROPIC_MODEL)已升格为主表单字段,由 ChannelForm 持有,
 * 本组件 emit 的 env 恒不含该键(父组件保存时合并)。
 *
 * 折叠默认收起,有任一映射时默认展开。
 */
const props = defineProps<{
  /** 渠道当前 modelEnv(编辑回显源);新建为 {} */
  modelEnv: Record<string, string>
  /** 实际请求模型候选(datalist,复用父组件 modelOptions computed) */
  modelOptions: string[]
  /** 探活转圈态 */
  probing: boolean
  /** datalist id 唯一后缀(避免多实例冲突) */
  domKey: string
}>()

const emit = defineEmits<{
  /** 构建后的 env 键值(整命名空间替换语义:空对象=清除全部映射) */
  (e: 'update:env', env: Record<string, string>): void
  /** 触发「获取模型列表」探活 */
  (e: 'probe'): void
}>()

// 角色标签(固定英文品牌名,不走 i18n)
const ROLE_LABELS: Record<ModelRole, string> = {
  FABLE: 'Fable',
  OPUS: 'Opus',
  SONNET: 'Sonnet',
  HAIKU: 'Haiku',
}

/** 解析回显并剥离 fallback(默认模型归主表单,本组件不产出 ANTHROPIC_MODEL) */
function parseWithoutFallback(env: Record<string, string>): ModelMapForm {
  const f = parseModelEnv(env)
  f.fallback = ''
  return f
}

const form = ref<ModelMapForm>(parseWithoutFallback(props.modelEnv))
// 有映射则默认展开
const expanded = ref(hasAnyMapping(props.modelEnv))

// 渠道切换(编辑不同渠道复用同一表单实例)时重新解析回显
watch(
  () => props.modelEnv,
  (v) => {
    form.value = parseWithoutFallback(v)
    expanded.value = hasAnyMapping(v)
  },
)

// 表单任意变更 → 构建 env 上抛(整命名空间替换语义)
watch(
  form,
  (v) => emit('update:env', buildModelEnv(v)),
  { deep: true },
)

const datalistId = computed(() => `channel-model-map-opts-${props.domKey}`)
</script>

<template>
  <div class="rounded-md border border-border/70 bg-background/40">
    <!-- 折叠头 -->
    <button
      type="button"
      class="w-full flex items-center gap-1.5 px-2.5 py-1.5 text-xs text-muted-foreground hover:text-foreground transition-colors"
      :aria-expanded="expanded"
      @click="expanded = !expanded"
    >
      <span :class="expanded ? 'i-carbon-chevron-down' : 'i-carbon-chevron-right'" class="w-3 h-3 shrink-0" />
      <span class="flex-1 text-left font-medium">{{ $t('settings.channelForm.modelMap.title') }}</span>
      <span class="text-[10px] text-muted-foreground/70 font-normal">{{ $t('settings.channelForm.modelMap.badge') }}</span>
    </button>

    <div v-show="expanded" class="px-2.5 pb-2.5 flex flex-col gap-2">
      <p class="text-[10px] text-muted-foreground/70 leading-snug">{{ $t('settings.channelForm.modelMap.intro') }}</p>

      <!-- 获取模型列表 -->
      <div class="flex items-center gap-2">
        <button
          type="button"
          :disabled="probing"
          class="px-2 py-0.5 text-[11px] rounded-md border border-border text-muted-foreground hover:text-foreground hover:bg-muted transition-colors disabled:opacity-50 inline-flex items-center gap-1"
          @click="emit('probe')"
        >
          <span :class="probing ? 'i-carbon-circle-dash animate-spin' : 'i-carbon-model-alt'" class="w-3 h-3" />
          {{ probing ? $t('settings.channelForm.modelMap.fetching') : $t('settings.channelForm.modelMap.fetchModels') }}
        </button>
        <span class="text-[10px] text-muted-foreground/60">{{ $t('settings.channelForm.modelMap.fetchHint') }}</span>
      </div>

      <!-- 表头 -->
      <div class="grid grid-cols-[3.5rem_1fr_1fr_2.5rem] items-center gap-1.5 text-[10px] text-muted-foreground/70 px-0.5">
        <span>{{ $t('settings.channelForm.modelMap.roleCol') }}</span>
        <span>{{ $t('settings.channelForm.modelMap.modelCol') }}</span>
        <span>{{ $t('settings.channelForm.modelMap.nameCol') }}</span>
        <span class="text-center">1M</span>
      </div>

      <!-- 四角色行 -->
      <div
        v-for="role in MODEL_ROLES"
        :key="role"
        class="grid grid-cols-[3.5rem_1fr_1fr_2.5rem] items-center gap-1.5"
      >
        <span class="text-xs font-medium">{{ ROLE_LABELS[role] }}</span>
        <input
          v-model="form.roles[role].model"
          type="text"
          :list="datalistId"
          :placeholder="$t('settings.channelForm.modelMap.modelPlaceholder')"
          class="form-input font-mono text-[11px] py-1"
          spellcheck="false"
        />
        <input
          v-model="form.roles[role].name"
          type="text"
          :placeholder="$t('settings.channelForm.modelMap.namePlaceholder')"
          class="form-input text-[11px] py-1"
        />
        <input
          v-model="form.roles[role].oneM"
          type="checkbox"
          class="accent-primary justify-self-center"
          :title="$t('settings.channelForm.modelMap.oneMHint')"
        />
      </div>

      <!-- 自定义行 -->
      <div class="grid grid-cols-[3.5rem_1fr_1fr_2.5rem] items-center gap-1.5 pt-1.5 border-t border-border/50">
        <span class="text-xs font-medium">{{ $t('settings.channelForm.modelMap.customRole') }}</span>
        <input
          v-model="form.custom.model"
          type="text"
          :list="datalistId"
          :placeholder="$t('settings.channelForm.modelMap.modelPlaceholder')"
          class="form-input font-mono text-[11px] py-1"
          spellcheck="false"
        />
        <input
          v-model="form.custom.name"
          type="text"
          :placeholder="$t('settings.channelForm.modelMap.namePlaceholder')"
          class="form-input text-[11px] py-1"
        />
        <input
          v-model="form.custom.oneM"
          type="checkbox"
          class="accent-primary justify-self-center"
          :title="$t('settings.channelForm.modelMap.oneMHint')"
        />
      </div>
      <p class="text-[10px] text-muted-foreground/60 leading-snug -mt-1">{{ $t('settings.channelForm.modelMap.customHint') }}</p>

      <datalist :id="datalistId">
        <option v-for="m in modelOptions" :key="m" :value="m" />
      </datalist>
    </div>
  </div>
</template>
