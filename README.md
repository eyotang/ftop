# ftop · GPUI Kit

使用 Rust / GPUI Kit 复刻 [Nongfsq/ftop](https://github.com/Nongfsq/ftop) 的浮动系统监视面板。使用 GPUI Kit Base 自绘最外层透明圆角窗口，保留深色玻璃底色、蓝色胶囊核心柱、绿色内存条、紧凑进程列表和 Sea / Graphite / Warm 配色。原版 MIT 许可证保留于 LICENSE。

当前是开发中的原生实现，还未达到原版功能和视觉完全一致。采集器按目标平台隔离，UI 只依赖共享数据模型：

- **Ubuntu/Linux**：直接读取 `/proc/stat`、`/proc/meminfo`、`/proc/<pid>/stat`、`/proc/net/dev` 和 sysfs；只枚举进程，不把线程当成应用，不读取磁盘与命令行。CPU 温度每五个采样更新；GPU 按 AMD sysfs、Intel PMU/DRM 和 NVIDIA NVML 分别采集。
- **macOS**：复用原版 MIT C 采集层，接入 IOReport 核心频率/分组、GPU、SMC 功耗和温度；Mach/sysctl 按原版口径计算内存、压缩、Swap 和压力。进程和网络使用 sysinfo 的原生 Mach/libproc 后端。私有硬件接口失败时显示不可用。
- **Windows**：使用 sysinfo 的原生 CPU/进程/网卡实现与 Win32 内存接口；`GetPerformanceInfo` 的 Commit 单独标注，不能当作 Swap。接入 WDDM GPU/显存、ACPI 温度、驱动报告 CPU 频率和核心分组，以及电池/Energy Meter 功耗；NVIDIA NVML 补充其可用传感器。没有通过 WMI 轮询温度，也不拿标称频率伪装实时核心频率。

进程 CPU 按整机归一化；同名进程聚合，可点击查看详情。设置存储在系统配置目录 `ftop-gpui/config.json`。当前接口无法读取的指标明确显示不可用，可悬停查看来源和限制；这不表示系统一定不存在该数据。macOS 硬件桥接的来源和边界见 [说明](native/macos/README.md)。

默认每秒采样一次，仅在数据到达时重绘，不运行持续 30 FPS 整窗动画；可选 Motion 使用有界短过渡。`--no-motion` / `--motion` 仅覆盖本次启动。窗口不可见时等待系统可见性通知并暂停采集，实际遮挡检测取决于平台后端。

## 开发

需要 Rust 1.98+。执行：

```bash
cargo run
cargo test
cargo fmt --check
```

Ubuntu 24.04 构建依赖：

```bash
sudo apt-get install build-essential pkg-config clang cmake libfontconfig1-dev libfreetype6-dev libxkbcommon-dev libwayland-dev libx11-xcb-dev libxcb1-dev libxkbcommon-x11-dev libvulkan-dev
```

macOS 需要 Xcode Command Line Tools；Windows 使用 MSVC Rust 工具链与 Visual Studio C++ Build Tools。GPUI 使用原生 GPU 渲染，不依赖 WebView。

## 操作

点击右上角 `•••` 打开模块开关、浅色模式和配色；进程上方两个圆形徽章切换 CPU / 内存排序；点击进程查看聚合指标。窗口可缩放，宽窗口并排显示采集指标与进程列表，窄窗口纵向排列。

## 更新和发布

参考本机 Metra 项目的更新策略：启动 20 秒后、之后每 24 小时读取本仓库公开 Release 的 `latest.json`；HTTPS、15 秒超时、64 KiB 上限，拒绝非稳定版号与降级。调试构建不自动检查。当前发行形态为手动下载，发现新版本后打开固定版本 Release 页面，不执行未验签安装包。

Metra 的 Windows NSIS 应用内签名更新依赖 Tauri，本项目尚未实现对应的 GPUI 安装器与签名更新通道。不能将空 `platforms` 清单视为应用内更新可用。

构建、验证安装包后，运行 `python3 scripts/release-manifest.py VERSION ASSET_DIRECTORY` 生成 `latest.json` 和 `SHA256SUMS`。发布描述使用 [发布模板](docs/releases/RELEASE_TEMPLATE.md)，其中包含 macOS `xattr` 命令。此脚本不会上传或发布。

## 待验证与差异

目标平台为 macOS Apple Silicon / Intel、Windows x86_64、Ubuntu 24.04 x86_64；各平台编译与桌面验收状态见 [验证记录](docs/VALIDATION.md)。原版系统材质、核心分组、原版完整动画、吸附尺寸、菜单栏/托盘、置顶与隐藏、系统主题自动跟随、文件热重载尚未完整复刻。没有使用演示数据代替真实采集。

本机 Ubuntu 26.04 的 X11 输入法需 `XMODIFIERS=@im=none` 临时启动；Wayland 不需要此参数。

在目标系统运行 `python3 scripts/package.py` 构建原生包：macOS `.app.zip`、Windows Portable ZIP、Ubuntu `.deb`。尚未验证安装流程。Ubuntu 24.04 发行包必须在 Ubuntu 24.04 构建；脚本按构建宿主的 glibc 版本声明最低依赖，避免把更新系统产物误标为兼容旧版。

性能测量使用 `cargo build --release` 后运行 `python3 scripts/benchmark.py --binary target/release/ftop --seconds 15 --backend x11`。脚本区分单核与整机 CPU，并记录 RSS；详见 [性能记录](docs/PERFORMANCE.md)。

采集覆盖和限制见 [平台指标矩阵](docs/METRICS.md)。指标图标可悬停查看名称、传感器来源及不可用原因；设置中的“采集状态”汇总能力。`--diagnostics` 输出硬件指标诊断（Windows GUI 正式包无控制台）。

### Language / 语言

Open **•••** and choose **Language / 语言**: System, 简体中文, English, 日本語, or 한국어. The selection takes effect immediately and is saved. System follows the OS language, with English as the fallback. Controls, metric tooltips, update status and sensor explanations are localized; process names, units and driver identifiers stay unchanged. This follows Metra's four-language policy using a native Rust catalog.

See [UI comparison](docs/UI-COMPARISON.md) for the upstream design audit and remaining differences.
