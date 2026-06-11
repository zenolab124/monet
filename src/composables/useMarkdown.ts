import MarkdownIt from 'markdown-it'
import markdownItShiki from '@shikijs/markdown-it'
import type { BundledLanguage } from 'shiki'

// 常用语言，按需加载
const LANGS: BundledLanguage[] = [
  'javascript', 'typescript', 'python', 'rust', 'go', 'java',
  'bash', 'shell', 'json', 'yaml', 'toml', 'html', 'css',
  'vue', 'jsx', 'tsx', 'sql', 'swift', 'kotlin', 'ruby',
  'c', 'cpp', 'diff', 'markdown', 'xml',
]

const mdOpts = { html: false, linkify: true, breaks: false, typographer: false }

// 轻量渲染器:无 shiki,常驻。流式期间用它,代码块素色但 parse 成本低一个数量级
const plainMd = new MarkdownIt(mdOpts)

// 当前活跃的完整渲染器（非响应式，避免触发递归更新）;shiki 就绪前由轻量版兜底
let activeMd: MarkdownIt = plainMd
let shikiReady = false

// 异步初始化 shiki，完成后替换活跃渲染器
markdownItShiki({
  themes: { light: 'github-light', dark: 'github-dark' },
  langs: LANGS,
  defaultColor: false,
}).then(plugin => {
  const md = new MarkdownIt(mdOpts)
  md.use(plugin)
  activeMd = md
  shikiReady = true
})

/** 流式降级渲染:跳过 shiki 高亮。流式中文本每帧变化,全量高亮是逐帧主线程大头 */
export function renderMarkdownPlain(text: string): string {
  return plainMd.render(text)
}

// 完成态渲染缓存:key 为原文,LRU。shiki 输出用 CSS 变量双主题,HTML 不随亮暗切换变,可安全缓存
const CACHE_MAX = 500
const htmlCache = new Map<string, string>()

/** 带缓存的完整渲染:用于内容不再变化的块(历史消息、流式结束后的块) */
export function renderMarkdownCached(text: string): string {
  const hit = htmlCache.get(text)
  if (hit !== undefined) {
    // 命中移到队尾,维持 LRU 序(Map 迭代序 = 插入序)
    htmlCache.delete(text)
    htmlCache.set(text, hit)
    return hit
  }
  const html = activeMd.render(text)
  // shiki 就绪前的结果是无高亮版,不入缓存,避免固化素色 HTML
  if (shikiReady) {
    htmlCache.set(text, html)
    if (htmlCache.size > CACHE_MAX) {
      htmlCache.delete(htmlCache.keys().next().value!)
    }
  }
  return html
}
