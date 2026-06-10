# Paper · 纸张主题

暖米色卡纸、墨上纸、墨绿+砖红点缀、棕偏三层阴影、系统字、颗粒+vignette 氛围层。
Notion / Are.na 气质，反深色科技感。源出 inkast，与 Glass 主题互为镜像（暖/冷、哑光/透光、贴地/悬浮）。

本 README 是**使用规范真理源**；`paper.css` 是 **token 真理源**。两者一起走，不拆开。

## 取用

1. **整文件夹复制**进目标项目（如 `src/styles/paper/`）——规范随行，目标项目里的下一个 Agent 也能读到。
   **不要软链**：链接出仓即断，其他机器 / CI / 部署环境直接失效；要更新就重新复制覆盖（快照语义，项目自治）。
2. 引入 token：`import './styles/paper/paper.css'`。
3. 暗色：`<html class="dark">`；氛围层（推荐）：`<body class="paper-atmosphere">`。

组件零修改换肤的前提：颜色 / 阴影 / 字体 / 圆角**只消费 CSS 变量**，绝不硬编码。

## Token 速查（完整定义见 paper.css，shadcn 语义命名）

| 维度 | 亮色 | 暗色 |
| --- | --- | --- |
| 背景 `--background` | 浓米色 `#F2EBDC` | 暗墨 `#1A1714` |
| 文字 `--foreground` | 深棕墨 `#2A2620` | 暖白 |
| 卡片 `--card` | 米黄 `#FBF6EA`，比背景亮一档 | 比背景亮一档 |
| 主色 `--primary` | 墨绿 `#3A5A40` | 浅墨绿 |
| 强调 `--accent` | 砖红 `#A4453B` | 浅砖红 |
| 阴影 | `--shadow-paper` / `--shadow-paper-lifted`，三层棕偏 | 同结构，黑色 |
| 圆角 `--radius` | `0.3rem`（4–6px） | 同 |

## 已内置（import 即生效，不要在项目里重写）

- **h1–h4 排版**：600 字重、字距 `-0.012em`。
- **`.paper-atmosphere` 氛围层**：SVG fractalNoise 颗粒（7% multiply，暗色 10% screen）+ 四角棕偏 vignette。
  fixed 全屏、以 multiply **盖在内容之上**（7% 几乎无感，但卡片表面也有纸纤维）。
  **全站只挂一次（body），子容器禁止重复叠加。**

## 设计铁律

红线（违反任意一条即破功）：

- 中文回落任何衬线字体（宋体一出，瞬间过时）；引入任何 webfont（@fontsource / Google Fonts / 本地 ttf）
- 纯黑 `#000` 与纯白 `#FFF`——最暗落到 `--foreground`，最亮落到 `--card`
- 中性灰阴影（瞬间破坏纸感）——只用 `--shadow-paper` / `--shadow-paper-lifted`
- 组件里写颜色字面量或 `font-family`
- backdrop-blur 毛玻璃、霓虹边、高饱和发光 ring、拟物大渐变（那是 Glass 主题的事）
- 圆角超过 6px

正确做法：

- 颜色一律语义 token：`bg-background` / `text-foreground` / `bg-card` / `text-muted-foreground` / `border-border`…
- 正文 400–500 字重、行高 ~1.8，让墨色在米色纸面上有呼吸感
- hover 模式：静置 `shadow-paper` → 悬浮 `shadow-paper-lifted` + 上移 1–2px（"把纸从桌面拈起来"）
- 字体统一 `var(--font-sans)`，落到系统栈（PingFang SC 接管中文）

## 预览

style-lab 下 `npm run dev` → `/themes/paper.html`（色板 / 阴影 / 氛围层开关 / 组件示范 / 铁律对照）。
