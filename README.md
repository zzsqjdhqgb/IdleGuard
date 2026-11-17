# 无操作自动关机（IdleShutdown）

一个面向 Windows 10/11 的轻量桌面工具：持续监测全局用户输入，在用户长时间无操作后先显示可穿透的桌面悬浮提醒，最后执行正常关机。

项目使用 **Rust + GPUI** 实现，产品需求见 `产品设计.md`。

## 功能

- **全局无操作检测**：仅调用 `GetLastInputInfo` 读取最后一次鼠标/键盘输入的时间，不安装钩子、不拦截或识别输入内容。
- **三个时间设置**：关机时间、提醒时间、倒计时时间。计时起点是“最后一次用户输入”与“本次软件启动”中较晚的一个。
- **悬浮提醒**：达到提醒时间后显示置顶、半透明黑色、扁平化悬浮窗，提示移动鼠标。
- **圆形倒计时**：距离关机还剩“倒计时时间”时开始显示圆形进度条与剩余时间；如果提醒时间更晚，倒计时不会被提醒覆盖或重置。
- **立即取消**：检测到用户重新操作后，悬浮窗立即隐藏、倒计时停止，三项计时从该次操作重新开始。
- **主界面设置**：时间输入、保存并应用、重新载入、开机自启开关，全部采用与悬浮窗一致的暗色扁平风格。
- **系统托盘**：左键切换主界面显示/隐藏；右键菜单提供“显示/隐藏主界面”和“退出”。关闭按钮、Alt+F4 只隐藏主界面，不退出进程。
- **开机自启**：写入/删除当前用户 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 项，不要求管理员权限。
- **配置持久化**：`%APPDATA%\IdleShutdown\config.json`，缺失或无效时自动回退到默认配置并给出提示，原文件会备份为 `config.invalid.json`。
- **正常关机**：优先启用 `SeShutdownPrivilege` 后调用 `ExitWindowsEx`，失败时回退到 `shutdown.exe /s /t 0`；失败会弹出错误对话框。
- **单实例**：命名互斥体阻止重复启动；第二次启动会把已有主界面带到前台。

## 构建与运行

### 环境

- Windows 10 或 Windows 11
- Rust stable（项目使用 edition 2024；`rust-version = 1.88`）
- Windows 推荐使用 MSVC 工具链：`stable-x86_64-pc-windows-msvc`
  - 若使用 GNU 工具链，需要安装 MinGW-w64。
- **发布（release）构建**：GPUI 在 release 下会调用 `fxc.exe` 编译 HLSL 着色器。该工具随 Windows SDK 提供；如果不在 `PATH` 中，可设置环境变量 `GPUI_FXC_PATH` 指向 `fxc.exe`。调试构建使用 GPUI 内的预编译着色器，不需要此步骤。

### 命令

```powershell
cargo build --release
cargo run --release
```

生成文件位于 `target\release\idle-shutdown.exe`。首次运行会自动开始计时，默认设置：

| 设置 | 默认值 |
| --- | --- |
| 关机时间 | 00:30:00 |
| 提醒时间 | 00:05:00 |
| 倒计时时间 | 00:02:00 |
| 开机自启 | 关闭 |

调试构建会保留控制台窗口；发布构建使用 Windows GUI 子系统，不显示控制台。

### 启动参数

- `--autostart`：开机自启时写入的命令行参数。当前版本启动后正常显示主界面；关闭主界面后会驻留托盘。

## 使用说明

1. 在主界面输入三项时间，点击“保存并应用”后立即生效。
   - 输入格式为“时:分:秒”，直接输入数字即可，输入框会自动格式化；获得焦点后输入会替换当前值。
   - 也可用每行的 `−` / `+` 按钮以 1 分钟为步长调整。
   - 时间输入框获得焦点后按 `↑` / `↓` 调整 1 秒，按住 `Shift` 调整 1 分钟。
2. 点击“重新载入”会从磁盘重新读取配置，并覆盖当前未保存的输入。
3. 点击“开机自启”开关立即写入或删除当前用户的注册表启动项。
4. 关闭主窗口（点击 `×` 或 Alt+F4）只会隐藏到托盘，不退出程序。
5. 将鼠标移到托盘图标上，左键点击切换主界面；右键菜单选择“退出”才会结束程序。
6. 到达提醒时间后，桌面底部会出现悬浮窗；移动鼠标或按任意键即可取消自动关机。

### 校验规则

保存前按以下顺序验证：

- 关机时间必须大于 0。
- 提醒时间必须早于关机时间（允许为 0，即启动后立即提醒）。
- 倒计时时间必须大于 0，且不能大于关机时间。

倒计时开始时刻严格为：

```text
倒计时开始时间 = 关机时间 - 倒计时时间
```

## 配置

### 位置

```text
%APPDATA%\IdleShutdown\config.json
%APPDATA%\IdleShutdown\config.invalid.json   （仅在原文件无效时生成）
%APPDATA%\IdleShutdown\idle-shutdown.log     （诊断日志）
```

### 格式

```json
{
  "version": 1,
  "shutdown_seconds": 1800,
  "reminder_seconds": 300,
  "countdown_seconds": 120,
  "auto_start": false
}
```

- 时间统一以秒存储。
- `version` 当前为 `1`；未知版本的配置会回退到默认值。
- 配置语义错误（如提醒时间不早于关机时间）同样会回退到默认值，并在主界面显示提示。
- 启动时开机自启的显示状态以注册表实际状态为准；`auto_start` 字段用于让配置文件保持自描述，并在用户切换开关时同步更新。

## 架构

| 模块 | 职责 |
| --- | --- |
| `src/main.rs` | Windows/非 Windows 入口，模块声明 |
| `src/app.rs` | 启动流程、窗口创建、托盘命令桥、计时循环、退出清理 |
| `src/platform/idle.rs` | `GetLastInputInfo` 低开销空闲检测 |
| `src/timer.rs` | 纯函数计时规则与单元测试 |
| `src/config.rs` | 配置序列化、校验、加载、保存、单元测试 |
| `src/main_view.rs` | 主窗口和设置界面 |
| `src/overlay.rs` | 悬浮窗视图、提示内容、圆形进度条 |
| `src/ui.rs` | 主题、时间输入控件、通用按钮 |
| `src/platform/tray.rs` | 原生 `Shell_NotifyIcon` 托盘图标、菜单和消息循环 |
| `src/platform/window.rs` | 窗口显示/隐藏、置顶、鼠标穿透、窗口图标、错误对话框 |
| `src/platform/autostart.rs` | HKCU Run 注册表读写 |
| `src/platform/shutdown.rs` | 关机权限和正常关机调用 |
| `src/platform/single_instance.rs` | 命名互斥体与重复启动处理 |
| `src/platform/icon.rs` | 将内嵌 ICO 转为 Win32 `HICON` |
| `src/state.rs` | GPUI 主线程共享状态 |
| `src/assets.rs` | 内嵌图标资源与 ICO 解析 |

### 同步模型

- 所有 GPUI 窗口和视图状态只在 UI 前台线程修改。
- 计时循环是 GPUI 前台任务，用 `background_executor().timer()` 让出线程；查询 `GetLastInputInfo` 本身是极轻量的 user32 调用。
- 悬浮窗可见时约 150 ms 轮询一次，以便输入后快速隐藏；不可见时最长约 750 ms 轮询一次，并会在临近提醒/倒计时/关机阈值时提前醒来，避免短倒计时被睡过。
- 托盘使用独立 Win32 线程和原生消息循环，通过异步通道向 UI 线程发送“切换主界面”或“退出”命令，不直接访问 GPUI 状态。
- 到达关机阈值后会再次读取全局空闲时间；若期间有新的输入，则取消本次关机并发起新一轮计时。

### 悬浮窗的非交互实现

创建悬浮窗后立即添加：

- `WS_EX_TOPMOST`（通过 `SetWindowPos` 设置）
- `WS_EX_TRANSPARENT`：鼠标穿透
- `WS_EX_NOACTIVATE`：不获取键盘焦点
- `WS_EX_TOOLWINDOW`：不出现在任务栏和 Alt+Tab

窗口以 `SW_SHOWNOACTIVATE` 显示，显示时带 `SWP_NOACTIVATE`。此外通过窗口过程子类化让 `WM_NCHITTEST` 返回 `HTTRANSPARENT`、`WM_MOUSEACTIVATE` 返回 `MA_NOACTIVATE`，把点击明确交给下层窗口。GPUI 使用 DirectComposition 的预乘 Alpha 绘制半透明内容，因此不会因普通窗口切换或失去焦点而隐藏。

## 开机自启

注册表项：

```text
HKCU\Software\Microsoft\Windows\CurrentVersion\Run
名称：IdleShutdown
值："<当前 exe 绝对路径>" --autostart
```

- 打开开关：创建/覆盖上述值。
- 关闭开关：删除该值；键或值不存在时视为成功。
- 读取状态：值不存在时返回“关闭”，不会报错。
- 如果移动了 exe，需要重新切换一次开关以刷新路径。

## 关机行为

1. 优先打开当前进程令牌的 `SeShutdownPrivilege`，调用 `ExitWindowsEx(EWX_SHUTDOWN | EWX_FORCEIFHUNG, ...)`。`EWX_FORCEIFHUNG` 只避免挂起程序无限阻塞系统正常关机流程。
2. 如果权限或 API 调用失败，回退到 `shutdown.exe /s /t 0`。
3. 两种方式都失败时弹出错误对话框并记录日志。
4. 关机流程只在到达关机阈值、且再次确认没有新的用户输入后发起。用户活动会取消当前倒计时，不会留下延迟关机任务。

> 关机需要当前用户具备关机权限，并且系统策略允许关机。若被组策略禁止，程序会显示失败原因。

## 资源

- `assets/app.ico`：多尺寸应用/托盘图标（16、24、32、48、64、128、256）。
- `assets/app.png`：256×256 预览图。
- `assets/generate_icon.py`：无第三方依赖的图标生成脚本，可重新生成上述资源。
- 图标在编译期通过 `include_bytes!` 嵌入，运行时不依赖外部文件。

## 许可证

本项目采用 **GPL-3.0-or-later**，见 `LICENSE`。

主要依赖许可证：

- GPUI（Apache-2.0）
- windows / windows-core（MIT OR Apache-2.0）
- serde / serde_json（MIT OR Apache-2.0）
- flume（MIT OR Apache-2.0）

这些许可证与 GPLv3 兼容。

## 验证

参见 [`docs/verification.md`](docs/verification.md)。当前开发环境为 Linux，因此 Windows 桌面运行期行为（托盘、透明悬浮窗、DPI、注册表、关机）无法在本机实测；已完成 Windows 目标的类型检查、Clippy、测试与交叉构建。
