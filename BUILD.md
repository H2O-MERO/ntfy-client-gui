# 编译指南

本项目使用 **Rust + egui** 编写，Cargo 默认构建出的就是**单文件可执行程序**：

- Windows：`ntfy-client-gui.exe`
- macOS：`ntfy-client-gui`（无扩展名的 Mach-O 单文件）

## 1. 安装 Rust

推荐使用 [rustup](https://rustup.rs/) 安装：

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Windows 也可以下载 `rustup-init.exe` 安装。

安装后确认：

```bash
rustc --version
cargo --version
```

本项目已在以下环境验证通过：

```text
rustc 1.97.1
cargo 1.97.1
stable-x86_64-pc-windows-msvc
```

## 2. 安装系统依赖

### Windows

- 默认使用 MSVC 工具链，需要安装 [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)。
- 安装时勾选“使用 C++ 的桌面开发”工作负载。
- 如果不想安装 VS Build Tools，也可以安装 GNU 工具链：
  ```bash
  rustup toolchain install stable-x86_64-pc-windows-gnu
  rustup default stable-x86_64-pc-windows-gnu
  ```

### macOS

- 安装 Xcode Command Line Tools：
  ```bash
  xcode-select --install
  ```

## 3. 编译

在项目根目录执行：

```bash
cargo build --release
```

Windows 下已验证成功生成：

```text
target\release\ntfy-client-gui.exe
```

这是可直接分发的单文件程序，不需要复制项目里的其他文件。

### Windows 一键脚本

项目根目录提供了 `build.bat`，双击或执行：

```bat
build.bat
```

脚本会设置 Git 自带的 OpenSSL 后端拉取 crates.io 索引，适用于 Schannel 报 `SEC_E_NO_CREDENTIALS` 的环境。

### 编译失败：文件被占用

如果编译时提示 `拒绝访问` / `os error 5`，通常是旧版本程序还在运行，先关闭进程再重新编译：

```powershell
Stop-Process -Name ntfy-client-gui -Force
cargo build --release
```

## 4. 可选：减小体积

`Cargo.toml` 的 `[profile.release]` 已默认开启：

```toml
[profile.release]
opt-level = 2
lto = "thin"
strip = "symbols"
```

重新编译即可生效：

```bash
cargo build --release
```

## 5. 为 macOS 交叉编译

Rust **不能直接从 Windows 交叉编译出 macOS 可执行文件**（需要 macOS SDK 和对应 linker）。  
要生成 Mac 版本，请在 macOS 上执行：

```bash
# Apple Silicon
rustup target add aarch64-apple-darwin
cargo build --release --target aarch64-apple-darwin

# Intel Mac
rustup target add x86_64-apple-darwin
cargo build --release --target x86_64-apple-darwin
```

产物：

- `target/aarch64-apple-darwin/release/ntfy-client-gui`
- `target/x86_64-apple-darwin/release/ntfy-client-gui`

## 6. 使用 GitHub Actions 同时构建 Windows / macOS

如果本机不方便分别构建，可以使用 CI。在仓库创建 `.github/workflows/build.yml`：

```yaml
name: Build

on:
  workflow_dispatch:
  push:
    tags:
      - "v*"

jobs:
  build:
    strategy:
      matrix:
        os: [windows-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo build --release
      - uses: actions/upload-artifact@v4
        with:
          name: ntfy-client-gui-${{ matrix.os }}
          path: |
            target/release/ntfy-client-gui.exe
            target/release/ntfy-client-gui
```

## 7. 配置文件说明

程序启动后会在**可执行文件所在目录**生成：

- `settings.json`
- `topics.json`

这两个文件已加入 `.gitignore`，不会提交到版本库。  
如果检测到旧版 `topics.txt`，程序会自动迁移为 `topics.json` 并删除旧文件。
