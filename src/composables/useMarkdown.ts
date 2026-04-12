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

// 当前活跃的渲染器（非响应式，避免触发递归更新）
let activeMd: MarkdownIt = new MarkdownIt(mdOpts)

// 异步初始化 shiki，完成后替换活跃渲染器
markdownItShiki({
  themes: { light: 'github-light', dark: 'github-dark' },
  langs: LANGS,
  defaultColor: false,
}).then(plugin => {
  const md = new MarkdownIt(mdOpts)
  md.use(plugin)
  activeMd = md
})

/** 渲染 markdown 文本为 HTML（纯函数，无响应式依赖） */
export function renderMarkdown(text: string): string {
  return activeMd.render(text)
}
