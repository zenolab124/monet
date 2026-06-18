import { ref, watch } from 'vue'

const STORAGE_KEY = 'cc-space:feature:html-visual'

const enabled = ref(localStorage.getItem(STORAGE_KEY) === 'true')

watch(enabled, v => localStorage.setItem(STORAGE_KEY, String(v)))

export const HTML_VISUAL_PROMPT = `当前客户端为 CC Space，支持在 Markdown 中渲染内嵌 HTML。请在以下场景主动使用 HTML 增强表达，替代纯 Markdown 的垂直流式输出：

触发场景：
1. 横向对比：方案优劣、参数矩阵、多维对照 → flex 并排卡片
2. 信息卡片：多字段聚合、视觉分组的密集信息 → 带边框 div 分区
3. 折叠内容：长日志、补充细节、非关键信息 → <details>/<summary>
4. 结构图：简单流程、架构关系、时间线 → HTML+CSS 或内嵌 SVG

标签用法：
- 直接用，客户端已有样式：<details>+<summary>、<table>、<mark>、<kbd>、<abbr title="...">
- 布局用内联 style：flex 并排(display:flex;gap:12px)、多列(columns:2)、卡片边框(padding:12px;border:1px solid #e0e0e0;border-radius:6px)
- 对比卡片必须用不同浅色背景区分立场（如暖色 #faf5ef vs 冷色 #eff4fa，或红调 #fdf0f0 vs 绿调 #f0fdf4），不要全白底

禁止：<script>、on* 事件属性、<style> 标签、class 属性、完整 HTML 页面框架。这些会被过滤，输出即浪费 token。

原则：Markdown 优先，HTML 穿插增强，每个片段服务于具体表达需求。`

export function useHtmlVisual() {
  return { enabled }
}
