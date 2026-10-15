# ftop · GPUI Kit

使用 Rust / GPUI Kit 复刻 [Nongfsq/ftop](https://github.com/Nongfsq/ftop) 的浮动系统监视面板。保留深色玻璃底色、蓝色胶囊核心柱、绿色内存条、紧凑进程列表和 Sea / Graphite / Warm 配色。原版 MIT 许可证保留于 LICENSE。

当前是开发中的原生实现，还未达到原版功能和视觉完全一致。CPU、内存、交换空间、网络速率和进程来自 sysinfo，后台线程采集。CPU 进程占比按整机归一化；同名进程聚合，可点击查看详情。设置存储在系统配置目录 `ftop-gpui/config.json`。CPU 频率为系统提供的采样值，不能等同于原版 Apple Silicon 私有接口的实时频率。温度读取系统可用传感器，悬浮可查看实际传感器名称。GPU、压缩内存和功耗尚未接入，显示 `—`。

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

点击右上角 `•••` 打开模块开关、浅色模式和配色；CPU / Memory 切换进程排序；点击进程查看聚合指标。窗口可缩放，宽窗口并排显示采集指标与进程列表，窄窗口纵向排列。

## 更新和发布

参考本机 Metra 项目的更新策略：启动 20 秒后、之后每 24 小时读取本仓库公开 Release 的 `latest.json`；HTTPS、15 秒超时、64 KiB 上限，拒绝非稳定版号与降级。调试构建不自动检查。当前发行形态为手动下载，发现新版本后打开固定版本 Release 页面，不执行未验签安装包。

Metra 的 Windows NSIS 应用内签名更新依赖 Tauri，本项目尚未实现对应的 GPUI 安装器与签名更新通道。不能将空 `platforms` 清单视为应用内更新可用。

构建、验证安装包后，运行 `python3 scripts/release-manifest.py VERSION ASSET_DIRECTORY` 生成 `latest.json` 和 `SHA256SUMS`。发布描述使用 [发布模板](docs/releases/RELEASE_TEMPLATE.md)，其中包含 macOS `xattr` 命令。此脚本不会上传或发布。

## 待验证与差异

目标平台为 macOS Apple Silicon / Intel、Windows x86_64、Ubuntu 24.04 x86_64；各平台编译与桌面验收状态见 [验证记录](docs/VALIDATION.md)。原版系统材质、核心分组、应用图标、原版完整动画、吸附尺寸、菜单栏/托盘、置顶与隐藏、系统主题自动跟随、文件热重载尚未完整复刻。没有使用演示数据代替真实采集。

本机 Ubuntu 26.04 的 X11 输入法需 `XMODIFIERS=@im=none` 临时启动；Wayland 不需要此参数。

在目标系统运行 `python3 scripts/package.py` 构建原生包：macOS `.app.zip`、Windows Portable ZIP、Ubuntu `.deb`。尚未验证安装流程。Ubuntu 24.04 发行包必须在 Ubuntu 24.04 构建；脚本按构建宿主的 glibc 版本声明最低依赖，避免把更新系统产物误标为兼容旧版。
