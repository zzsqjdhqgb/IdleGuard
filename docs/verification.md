# 验证记录

## 环境

| 项 | 值 |
| --- | --- |
| 运行环境 | Linux（Debian 12，WSL2 容器），无 Windows 桌面会话 |
| Rust | rustc 1.100.0 / cargo 1.100.0（stable） |
| Windows 检查目标 | `x86_64-pc-windows-gnu` |
| 交叉工具链 | MinGW-w64（`x86_64-w64-mingw32-gcc`） |
| 项目 | `idle-shutdown` 0.1.0（Rust 2024 edition） |

当前环境不是 Windows，因此无法实际启动 GPUI 的 Windows 后端、托盘图标、DirectComposition 透明窗口或执行关机。以下记录区分“已执行并通过”和“只能在 Windows 上验证”的部分。

## 已执行的命令与结果

### 1. 格式化

```text
$ cargo fmt --all
$ cargo fmt --all -- --check
```

通过。

### 2. 单元测试（与平台无关的计时、配置、资源解析）

```text
$ cargo test
running 21 tests
...
test result: ok. 21 passed; 0 failed
```

覆盖内容：

- 配置 JSON 解码、版本校验、语义校验、保存/加载往返。
- 缺失 `auto_start` 使用默认值。
- 无效配置和非法时间关系被拒绝。
- 启动时间与全局空闲时间的基准合并（`effective_idle`）。
- 提醒阈值、倒计时开始阈值、关机阈值。
- 提醒晚于倒计时开始时，倒计时优先且不被覆盖。
- 空闲时间接近阈值时的下一次唤醒延迟（短倒计时不会被跳过）。
- 剩余秒数向上取整、时间格式化。
- 内嵌 ICO 目录解析和尺寸选择。

### 3. Windows 目标类型检查

```text
$ cargo check --target x86_64-pc-windows-gnu
Finished `dev` profile ... 
```

通过，无 Rust 警告。

### 4. Clippy（Windows 目标，包含测试代码）

```text
$ cargo clippy --target x86_64-pc-windows-gnu --all-targets
Finished `dev` profile ...
```

通过，无 Clippy 警告。

### 5. Windows 目标完整交叉构建（含链接）

```text
$ cargo build --target x86_64-pc-windows-gnu
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 21s
```

通过，生成 PE 可执行文件：

```text
target/x86_64-pc-windows-gnu/debug/idle-shutdown.exe
```

最终产物大小为 469,834,290 字节（debug + 完整调试信息）。该产物使用 MinGW GNU ABI，只用于确认 Windows API 绑定和最终链接可行。正式发布建议在 Windows 上使用 MSVC 工具链构建：

```powershell
cargo build --release --target x86_64-pc-windows-msvc
```

MSVC 目标的静态交叉检查受限于当前 Linux 容器（缺少 MSVC `lib.exe`），因此未作为验证命令执行。

## 构建说明

- 上述交叉构建是 **debug** 构建，使用 GPUI 内置的预编译着色器。
- Windows 的 **release** 构建会让 GPUI 调用 Windows SDK 的 `fxc.exe` 编译 HLSL；请确保 Windows SDK 已安装（或设置 `GPUI_FXC_PATH`）。
- 本容器缺少 MSVC 的 `lib.exe`，因此未执行 `x86_64-pc-windows-msvc` 的静态检查；Windows API 部分已通过 GNU 目标的完整编译与链接验证。
- 构建输出中唯一剩余的提示是第三方依赖 `proc-macro-error2` 的 future-incompat 警告，与本项目代码无关。

## Windows 专属、无法在当前环境验证的行为

以下行为已实现并在 Windows 目标上完成编译/链接，但需要在实际 Windows 10/11 上手工验收：

1. **托盘**：`Shell_NotifyIcon` 图标出现；左键切换主界面；右键菜单显示/隐藏与退出；Explorer 重启后图标恢复。
2. **悬浮窗样式**：DWM + DirectComposition 下实际呈现半透明黑色；鼠标穿透（点击下方窗口）；键盘不获焦；不进入 Alt+Tab；普通窗口切换时不隐藏。
3. **圆形进度条**：1 秒粒度更新、圆弧方向、临近关机的红色状态和文字排版。
4. **主窗口**：较小屏幕下正文可滚动、固定底栏、动态窗口高度。
5. **多显示器 / 高 DPI**：悬浮窗位于主显示器工作区底部；主窗口在 125%/150% 缩放下大小合适。
6. **计时**：真实鼠标/键盘输入后，悬浮窗在 150 ms 轮询周期内消失；OS 空闲时间与启动基准合并正确。
7. **开机自启**：HKCU Run 写入/删除、值不存在时的读取、移动 exe 后重新切换。
8. **关机**：`SeShutdownPrivilege`、`ExitWindowsEx`、`shutdown.exe` 回退以及组策略禁止关机时的错误对话框。
9. **单实例**：第二次启动将已有隐藏/最小化窗口带到前台。
10. **退出清理**：从托盘菜单“退出”后进程结束，图标消失，悬浮窗不残留。

## 建议的 Windows 手工验收清单

在**可丢弃的测试机或虚拟机**上执行（关机测试会实际关闭系统）：

1. 使用默认配置启动，确认主窗口、任务栏图标、托盘图标出现。
2. 设置关机 `00:01:30`、提醒 `00:00:40`、倒计时 `00:00:20`，保存；保持无操作。
   - 约 40 秒出现提醒悬浮窗。
   - 约 70 秒切换为圆形倒计时。
   - 约 90 秒发起关机。
3. 在提醒或倒计时出现时移动鼠标：悬浮窗应在 150 ms 级别内消失，倒计时取消；再次等待，提醒/倒计时从本次输入重新开始。
4. 设置提醒晚于倒计时（例如关机 90 秒、提醒 80 秒、倒计时 60 秒），确认倒计时先出现且在 80 秒时不被提醒覆盖。
5. 尝试点击悬浮窗所在区域：点击应落到其下方窗口；悬浮窗不应获得键盘焦点。
6. 左键点击托盘图标两次，确认主界面隐藏/显示；关闭主窗口，确认进程仍在托盘；右键菜单退出，确认进程结束且无残留图标。
7. 切换开机自启，检查注册表并注销/登录验证；关闭开关后确认值被删除。
8. 修改 `%APPDATA%\IdleShutdown\config.json` 为非法 JSON，重启后确认显示提示、使用默认值，并生成 `config.invalid.json`。
9. 同时启动两个实例，确认第二个实例不创建新窗口，只将第一个窗口带到前台。

## 已知限制

- 当前容器没有 Windows 桌面，因此所有 GUI/系统集成验证仅到“Windows 目标编译、链接通过”为止。
- GPUI 的 Windows 后端依赖 DirectComposition；远程桌面、旧显卡驱动或特殊安全软件环境下透明和置顶表现可能不同，需要在目标机器确认。
- 托盘和悬浮窗的图标与字体使用 Windows 自带资源（Microsoft YaHei UI）；如果系统精简掉该字体，GPUI 会尝试系统字体回退。
- exe 文件本身未嵌入图标资源（项目在运行时从内嵌 ICO 创建窗口/托盘图标）；这不影响功能，若需要 Explorer 中的 exe 图标，可在 Windows 构建时额外添加 `.rc` 资源。
