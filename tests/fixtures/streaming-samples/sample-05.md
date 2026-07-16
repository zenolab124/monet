Here is the complete report of all findings across the five search categories.

---

## 1. Command+W / Command+Q / CmdOrCtrl+W / CmdOrCtrl+Q 相关代码

**仅在一个文件中定义了这两个加速器：**

**`/Users/zz/workspace/cc-apps/cc-space-tauri/src-tauri/src/menu.rs`**

- **第 16 行** -- `CmdOrCtrl+Q` 绑定到菜单项 "退出 CC Space"，id 为 `"quit"`：
  ```rust
  .item(&MenuItemBuilder::new("退出 CC Space").id("quit").accelerator("CmdOrCtrl+Q").build(app)?)
  ```

- **第 19-22 行** -- `CmdOrCtrl+W` 绑定到菜单项 "关闭标签页"，id 为 `"close-tab"`：
  ```rust
  let close_tab = MenuItemBuilder::new("关闭标签页")
      .id("close-tab")
      .accelerator("CmdOrCtrl+W")
      .build(app)?;
  ```

项目中没有其他地方出现 `Command+W`、`Command+Q`、`Cmd+W`、`Cmd+Q`、`Super+W`、`Super+Q` 等变体写法。

---

## 2. 窗口关闭/退出相关的 Tauri 配置

**`/Users/zz/workspace/cc-apps/cc-space-tauri/src-tauri/tauri.conf.json`**

整个 windows 配置如下（第 13-28 行）：

```json
"windows": [
  {
    "title": "CC Space",
    "width": 1200,
    "height": 800,
    "minWidth": 800,
    "minHeight": 500,
    "resizable": true,
    "fullscreen": false,
    "decorations": true,
    "titleBarStyle": "Overlay",
    "hiddenTitle": true,
    "zoomHotkeysEnabled": false,
    "dragDropEnabled": false
  }
]
```

关键发现：配置中**没有** `closable`、`visible`、`skipTaskbar`、`alwaysOnTop` 等与关闭/退出行为相关的字段。所有窗口关闭行为由 Rust 代码在运行时控制（见第 3 节）。

---

## 3. Rust 端的窗口事件处理

**`/Users/zz/workspace/cc-apps/cc-space-tauri/src-tauri/src/lib.rs`，第 114-132 行**

这是核心的窗口事件拦截逻辑：

```rust
// 窗口事件拦截：红色关闭按钮→隐藏到托盘；Destroyed→清理
let handle = app.handle().clone();
if let Some(window) = handle.get_webview_window("main") {
    let w = window.clone();
    window.on_window_event(move |event| {
        match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = w.hide();
            }
            tauri::WindowEvent::Destroyed => {
                streaming::close_all_sessions();
                agent::shutdown();
                channels::shutdown_fm_serve();
            }
            _ => {}
        }
    });
}
```

行为总结：
- **`CloseRequested`**（红色关闭按钮 / 系统关闭请求）：**阻止真正关闭**，改为隐藏窗口（收到托盘）。
- **`Destroyed`**（窗口销毁时）：清理所有 streaming sessions、关闭 agent、关闭 FM 服务。

**`/Users/zz/workspace/cc-apps/cc-space-tauri/src-tauri/src/menu.rs`，第 59-71 行**

两个 Tauri command 供前端调用：

```rust
#[tauri::command]
pub fn hide_main_window(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    streaming::close_all_sessions();
    agent::shutdown();
    app.exit(0);
}
```

这两个 command 在 `lib.rs` 第 237-238 行注册到 invoke_handler。

**`/Users/zz/workspace/cc-apps/cc-space-tauri/src-tauri/src/tray.rs`**

系统托盘的退出处理（第 30-34 行）：

```rust
"quit" => {
    streaming::close_all_sessions();
    agent::shutdown();
    app.exit(0);
}
```

托盘的 "打开 CC Space" 处理（第 24-29 行）和托盘图标点击处理（第 37-45 行）都是 show + set_focus。

---

## 4. 前端的键盘快捷键监听

### 4a. 核心快捷键分发 -- useShortcuts.ts

**`/Users/zz/workspace/cc-apps/cc-space-tauri/src/composables/useShortcuts.ts`**（完整文件，37 行）

监听 Rust 菜单事件并转为前端行为：

- **`menu:close-tab`**（由 CmdOrCtrl+W 触发）：如果在 workbench 视图且有多个标签，关闭当前标签（有会话时弹确认框）；否则调用 `hide_main_window` 隐藏窗口。
- **`menu:request-quit`**（由 CmdOrCtrl+Q 触发）：弹确认对话框，确认后调用 `quit_app` 退出应用。

### 4b. App.vue 全局键盘监听

**`/Users/zz/workspace/cc-apps/cc-space-tauri/src/App.vue`，第 47-75 行**

全局 `keydown` 监听器处理以下快捷键：
- `Cmd+R`：刷新项目列表
- `Cmd+Shift+M`：切换性能监视 HUD
- `Cmd+=`/`Cmd+-`：缩放
- `Cmd+0`：重置缩放
- `Escape`：取消档案馆选择

### 4c. 其他组件中的键盘监听

以下组件也有各自的 `keydown` 监听器，但与窗口关闭/退出无关：

| 文件 | 快捷键 | 用途 |
|------|--------|------|
| `/Users/zz/workspace/cc-apps/cc-space-tauri/src/components/Toolbar.vue` (第 23-28 行) | `Cmd+F` | 聚焦搜索框 |
| `/Users/zz/workspace/cc-apps/cc-space-tauri/src/components/TitleBarTools.vue` (第 24-29 行) | `Cmd+F` | 聚焦搜索框 (sessions 视图) |
| `/Users/zz/workspace/cc-apps/cc-space-tauri/src/components/QuestionCard.vue` (第 125 行) | 各种按键 | 问题卡片交互 |
| `/Users/zz/workspace/cc-apps/cc-space-tauri/src/components/SessionDetail.vue` (第 1248 行) | 输入快捷键 | 会话输入框 |
| `/Users/zz/workspace/cc-apps/cc-space-tauri/src/components/SlashCommandPanel.vue` (第 62 行) | 方向键/Enter/Esc | 斜杠命令面板导航 |
| `/Users/zz/workspace/cc-apps/cc-space-tauri/src/components/PlanApprovalCard.vue` (第 58 行) | 按键 | 计划审批交互 |
| `/Users/zz/workspace/cc-apps/cc-space-tauri/src/components/PermissionCard.vue` (第 76 行) | 按键 | 权限卡片交互 |
| `/Users/zz/workspace/cc-apps/cc-space-tauri/src/components/settings/PaperSelect.vue` (第 78 行) | 方向键 | 下拉选择导航 |
| `/Users/zz/workspace/cc-apps/cc-space-tauri/src/components/topbar/PermissionModeDropdown.vue` (第 61 行) | 方向键 | 下拉菜单导航 |

---

## 5. Tauri 菜单 / Accelerator 配置

**`/Users/zz/workspace/cc-apps/cc-space-tauri/src-tauri/src/menu.rs`** -- 完整的菜单结构：

| 菜单 | 菜单项 | Accelerator |
|------|--------|-------------|
| **CC Space** (app_menu) | About | (系统默认) |
| | Hide | (系统默认) |
| | Hide Others | (系统默认) |
| | Show All | (系统默认) |
| | 退出 CC Space | `CmdOrCtrl+Q` |
| **File** (file_menu) | 关闭标签页 | `CmdOrCtrl+W` |
| **Edit** (edit_menu) | Undo, Redo, Cut, Copy, Paste, Select All | (系统默认) |
| **Window** (window_menu) | Minimize, Maximize, Fullscreen | (系统默认) |

菜单在 `lib.rs` 第 73-74 行注册：
```rust
.menu(|app| menu::create(app))
.on_menu_event(|app, event| menu::handle_event(app, &event))
```

**菜单事件处理** (`menu.rs` 第 47-57 行)：
- `"close-tab"` --> 向前端 emit `menu:close-tab` 事件
- `"quit"` --> 向前端 emit `menu:request-quit` 事件

---

## 总结：CmdOrCtrl+W / CmdOrCtrl+Q 的完整事件流

**CmdOrCtrl+Q 退出流程：**
1. 用户按 Cmd+Q
2. Tauri 菜单 accelerator 匹配到 id `"quit"`
3. `menu::handle_event` emit `menu:request-quit` 到前端
4. `useShortcuts.ts` 监听到事件，弹出确认对话框
5. 用户确认后，调用 `invoke('quit_app')`
6. Rust 端 `quit_app` 关闭所有 sessions、shutdown agent、`app.exit(0)`

**CmdOrCtrl+W 关闭/隐藏流程：**
1. 用户按 Cmd+W
2. Tauri 菜单 accelerator 匹配到 id `"close-tab"`
3. `menu::handle_event` emit `menu:close-tab` 到前端
4. `useShortcuts.ts` 监听到事件：
   - 若在 workbench 视图且 tabs > 1：关闭当前标签（有会话时先确认）
   - 否则：调用 `invoke('hide_main_window')` 隐藏窗口到托盘

**红色关闭按钮（非快捷键）：**
- 直接触发 `WindowEvent::CloseRequested`，Rust 端 `api.prevent_close()` + `window.hide()`，不经过前端。