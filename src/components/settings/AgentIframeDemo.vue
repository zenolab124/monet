<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'

const logs = ref<string[]>([])
const iframeRef = ref<HTMLIFrameElement>()

function log(direction: '收到' | '发出', type: string, data: unknown) {
  const line = `[${direction}] ${type}: ${JSON.stringify(data)}`
  logs.value = [...logs.value.slice(-19), line]
}

function onMessage(e: MessageEvent) {
  if (e.data?.source !== 'agent-iframe') return
  const { type, payload } = e.data

  log('收到', type, payload)

  if (type === 'ping') {
    postToIframe('pong', { echo: payload.ts, hostTs: Date.now() })
  } else if (type === 'tool_request') {
    const result = handleToolRequest(payload.tool, payload.args)
    postToIframe('tool_result', { tool: payload.tool, result })
  } else if (type === 'ui_action') {
    postToIframe('ack', { received: payload.value })
  }
}

function handleToolRequest(tool: string, args: Record<string, unknown>): unknown {
  if (tool === 'get_time') return new Date().toLocaleString('zh-CN')
  if (tool === 'echo') return args
  return { error: `unknown tool: ${tool}` }
}

function postToIframe(type: string, payload: unknown) {
  log('发出', type, payload)
  iframeRef.value?.contentWindow?.postMessage({ source: 'host', type, payload }, '*')
}

onMounted(() => window.addEventListener('message', onMessage))
onUnmounted(() => window.removeEventListener('message', onMessage))

const iframeSrc = `<!DOCTYPE html>
<html>
<head>
<style>
  * { margin: 0; box-sizing: border-box; }
  body { font-family: system-ui; padding: 12px; background: #faf8f5; color: #3d3229; font-size: 13px; }
  button { padding: 4px 10px; border: 1px solid #c4b5a0; border-radius: 4px; background: #fff;
           cursor: pointer; font-size: 12px; margin-right: 6px; }
  button:hover { background: #f0ebe4; }
  .log { margin-top: 10px; font-size: 11px; font-family: monospace; color: #6b5c4d;
         max-height: 120px; overflow-y: auto; white-space: pre-wrap; word-break: break-all; }
  .actions { display: flex; flex-wrap: wrap; gap: 6px; }
  select { padding: 3px 6px; border: 1px solid #c4b5a0; border-radius: 4px; font-size: 12px; background: #fff; }
</style>
</head>
<body>
  <div style="font-weight:600; margin-bottom:8px;">Agent iframe 沙箱</div>
  <div class="actions">
    <button onclick="ping()">Ping 宿主</button>
    <button onclick="callTool('get_time')">请求时间</button>
    <select onchange="if(this.value){action(this.value);this.selectedIndex=0}">
      <option value="">选择方案...</option>
      <option value="方案A">方案 A</option>
      <option value="方案B">方案 B</option>
      <option value="方案C">方案 C</option>
    </select>
  </div>
  <div class="log" id="log"></div>
  <script>
    const logEl = document.getElementById('log');
    function log(msg) { logEl.textContent += msg + '\\n'; logEl.scrollTop = logEl.scrollHeight; }

    window.addEventListener('message', (e) => {
      if (e.data?.source !== 'host') return;
      log('[host → iframe] ' + e.data.type + ': ' + JSON.stringify(e.data.payload));
    });

    function post(type, payload) {
      log('[iframe → host] ' + type + ': ' + JSON.stringify(payload));
      window.parent.postMessage({ source: 'agent-iframe', type, payload }, '*');
    }
    function ping() { post('ping', { ts: Date.now() }); }
    function callTool(tool, args) { post('tool_request', { tool, args: args || {} }); }
    function action(value) { post('ui_action', { action: 'user_selected', value }); }
  <\/script>
</body>
</html>`
</script>

<template>
  <div class="rounded-md border border-border bg-card overflow-hidden">
    <iframe
      ref="iframeRef"
      :srcdoc="iframeSrc"
      sandbox="allow-scripts"
      class="w-full border-0"
      style="height: 200px;"
    />
    <div class="border-t border-border px-3 py-2">
      <div class="text-[11px] text-muted-foreground mb-1 font-medium">宿主日志</div>
      <div class="font-mono text-[11px] text-foreground/70 max-h-30 overflow-y-auto whitespace-pre-wrap break-all">
        <div v-for="(line, i) in logs" :key="i">{{ line }}</div>
        <div v-if="!logs.length" class="text-muted-foreground">等待 iframe 消息...</div>
      </div>
    </div>
  </div>
</template>
