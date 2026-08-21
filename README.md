# ntfy-client-gui

用 **Rust + egui** 重写的 [ntfy-pusher-Windows](https://github.com/H2O-MERO/ntfy-pusher-Windows) 客户端。

一个轻量级 ntfy.sh 兼容推送通知客户端，支持：

- 多服务器、多话题订阅
- WebSocket 与 Long HTTP JSON 两种接收协议
- 公开/私有话题（HTTP Basic 认证）
- 系统托盘运行（`tray-icon`）
- 应用内通知弹窗（可配置超时、进度条、深色模式、提示音）
- 设置持久化（`settings.json` / `topics.json`）
- 开机自启动（Windows 注册表 Run 键）
- 启动参数：`--start-in-tray` / `--allow-multiple-instances`
- GitHub Releases 更新检查

## 构建

```bash
cargo build --release
```

生成的可执行文件位于 `target/release/ntfy-client-gui.exe`（Windows）或 `target/release/ntfy-client-gui`（macOS）。

详细的 Windows / macOS 编译步骤见 [BUILD.md](BUILD.md)。

## 使用

```text
-h, --help                  显示帮助
-t, --start-in-tray         启动后隐藏到托盘
-m, --allow-multiple-instances  允许多实例（当前版本接受该参数，未强制单实例检查）
```

## 数据文件

默认保存在**应用目录**（可执行文件旁边），不写入 AppData：

- `settings.json` — 设置
- `topics.json` — 已订阅话题

这两个文件已加入 `.gitignore`，不会被提交到版本库。  
如果检测到旧版 `topics.txt`，程序会自动迁移为 `topics.json` 并删除旧文件。

## 目录结构

```text
src/
  main.rs         入口、命令行参数
  app.rs          egui 主应用、界面、弹窗、托盘事件
  settings.rs     设置模型与持久化
  topics.rs       话题模型与持久化
  ntfy.rs         ntfy 协议监听（HTTP/WebSocket）
  updater.rs      GitHub 更新检查
  notification.rs 通知/弹窗辅助
```

## 说明

- 界面目前以中文为主；设置中的“语言”字段会随 `settings.json` 保存，但尚未实现完整多语言资源切换。
- 程序启动时会尝试加载 Windows 自带中文字体（黑体/雅黑），以便正确显示中文界面。
- “原生 Windows 通知”通过 `notify-rust` 发送系统 toast；如果系统环境不支持，会自动回退到应用内弹窗。
- 自定义通知弹窗绘制在主窗口内；收到自定义通知、连接失败或需要展示更新结果时会自动显示并聚焦主窗口。
- 自动更新当前实现为“检查更新并打开 GitHub Release 页面”，未实现原版的全自动替换升级。
