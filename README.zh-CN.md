# Waymark

**工单就是仓库里的 Markdown 文件。没有数据库，没有后台服务——`git clone`
就是全部安装。**

[English](README.md) | 简体中文

![Waymark 横幅](assets/banner-readme.png)

```sh
$ waymark init
initialised Waymark project at /home/you/demo

$ waymark create --title "Ship the first screen" --category task --owner "You"
created wm-f3c4djp65z9e

$ sed -n '1,5p' work/task/wm-f3c4djp65z9e.md
---
schema: 1
id: wm-f3c4djp65z9e
title: Ship the first screen
category: task
```

- **文档就是事实来源。** 一个工单一份带 YAML frontmatter 的 Markdown 文件，放在
  `work/` 下，配置放在 `.waymark/`。历史由 Git 保管，所以工单可以像代码一样被
  评审、diff 和回滚。
- **查询由内存投影作答，而且会说明来源。** 集合查询来自一份从文件重建、按内容
  指纹校验的内存投影。长期运行的视图会增量刷新，字节没变的文档不会重新解析。
  每个响应都记录 `source`、`completeness` 和 `freshness`；视图不完整时返回
  `INCOMPLETE_PROJECTION`，而不是发布半份结果。
- **agent 和你用同一个项目。** `waymark-mcp` 是走 stdio 的 Model Context
  Protocol 服务器，Claude Code、Cursor 或任何 MCP 客户端读写的都是同一批工单。
- **运行时什么都不用装。** 四平台预编译二进制；没有数据库、没有后台进程；除只读
  Web 视图外没有任何网络监听，且只绑定回环地址。

Pre-1.0：文档格式与 CLI 仍可能在版本之间变化；macOS 和 Windows 二进制没有签名，
运行前请用 `.sha256` 校验每个压缩包。

**不适合你的情况**：你需要看板、通知或多人实时协作——这些是刻意不做的，不是
路线图上的缺口。

## 两种包

两种包由同一个 tag 构建，读写同一种项目格式：

- **Basic** —— `waymark` 命令行工具，以及 `waymark-mcp` 这个 Model Context
  Protocol 服务器。
- **Web** —— 同样的命令，外加由 `waymark serve` 提供的只读 Web 视图。

预编译的 Waymark 不需要 Rust 工具链，Web 包也不需要 Node.js 或 npm。两种版本在
运行时都不会下载或安装任何东西。

## 安装

预编译包发布在 `https://github.com/QiYueran-source/waymark` 的 GitHub Releases
里。下载对应平台的 basic 包，校验、解压，然后把解压出的目录加入 `PATH`。

有 Rust 工具链时，同样的二进制也可以直接从 crates.io 安装：

```sh
cargo install waymark-cli   # 安装 waymark 这个二进制
cargo install waymark-mcp   # 安装 MCP 服务器
```

CLI 的 crate 名是 `waymark-cli` 而不是 `waymark`：crates.io 上的 `waymark`
是另一个无关的 crate。

压缩包命名如下：

| 平台 | 压缩包 |
| --- | --- |
| Linux x86_64 | `waymark-v<version>-linux-x86_64.tar.gz` |
| macOS Apple 芯片 | `waymark-v<version>-macos-aarch64.tar.gz` |
| macOS Intel | `waymark-v<version>-macos-x86_64.tar.gz` |
| Windows x86_64 | `waymark-v<version>-windows-x86_64.zip` |

每个 Unix 压缩包都没有顶层目录：根目录下直接是 `waymark`、`waymark-mcp`、
`README.md`、`README.zh-CN.md`、`SECURITY.md`、`LICENSE-MIT`、`LICENSE-APACHE`
和 `THIRD_PARTY_LICENSES.txt`。Windows 压缩包内容相同，可执行文件名为
`waymark.exe` 与 `waymark-mcp.exe`。
`THIRD_PARTY_LICENSES.txt` 是该构建的依赖许可证清单。

下面的示例使用版本 `0.1.0`。安装其他版本时请修改 `VERSION`。

### Linux x86_64

```sh
VERSION=0.1.0
ARCHIVE="waymark-v${VERSION}-linux-x86_64.tar.gz"

curl -LO "https://github.com/QiYueran-source/waymark/releases/download/v${VERSION}/${ARCHIVE}"
curl -LO "https://github.com/QiYueran-source/waymark/releases/download/v${VERSION}/${ARCHIVE}.sha256"

sha256sum --check "${ARCHIVE}.sha256"

mkdir -p "$HOME/.local/waymark"
tar -xzf "${ARCHIVE}" -C "$HOME/.local/waymark"
export PATH="$HOME/.local/waymark:$PATH"
```

`sha256sum --check` 必须只对压缩包输出 `OK`。校验值不匹配说明下载已损坏，或者
拿到的不是发布的文件，此时不要运行它。

### macOS Apple 芯片

```sh
VERSION=0.1.0
ARCHIVE="waymark-v${VERSION}-macos-aarch64.tar.gz"

curl -LO "https://github.com/QiYueran-source/waymark/releases/download/v${VERSION}/${ARCHIVE}"
curl -LO "https://github.com/QiYueran-source/waymark/releases/download/v${VERSION}/${ARCHIVE}.sha256"

shasum -a 256 -c "${ARCHIVE}.sha256"

mkdir -p "$HOME/.local/waymark"
tar -xzf "${ARCHIVE}" -C "$HOME/.local/waymark"
export PATH="$HOME/.local/waymark:$PATH"
```

### macOS Intel

```sh
VERSION=0.1.0
ARCHIVE="waymark-v${VERSION}-macos-x86_64.tar.gz"

curl -LO "https://github.com/QiYueran-source/waymark/releases/download/v${VERSION}/${ARCHIVE}"
curl -LO "https://github.com/QiYueran-source/waymark/releases/download/v${VERSION}/${ARCHIVE}.sha256"

shasum -a 256 -c "${ARCHIVE}.sha256"

mkdir -p "$HOME/.local/waymark"
tar -xzf "${ARCHIVE}" -C "$HOME/.local/waymark"
export PATH="$HOME/.local/waymark:$PATH"
```

### Windows x86_64

Windows 压缩包是用 MSVC 工具链构建的 `.zip`。在 PowerShell 中：

```powershell
$Version = "0.1.0"
$Archive = "waymark-v$Version-windows-x86_64.zip"
$Base = "https://github.com/QiYueran-source/waymark/releases/download/v$Version"

Invoke-WebRequest "$Base/$Archive" -OutFile $Archive
Invoke-WebRequest "$Base/$Archive.sha256" -OutFile "$Archive.sha256"

$Expected = (Get-Content "$Archive.sha256") -split '\s+' | Select-Object -First 1
$Actual = (Get-FileHash $Archive -Algorithm SHA256).Hash.ToLower()
if ($Actual -ne $Expected) { throw "checksum mismatch for $Archive" }

Expand-Archive $Archive -DestinationPath "$HOME\waymark"
$env:Path = "$HOME\waymark;$env:Path"
```

`Get-FileHash` 的结果必须与 `.sha256` 文件的第一段一致；不一致说明下载已损坏，
或者拿到的不是发布的文件，不要运行它。`$env:Path` 的修改只在当前会话生效；要
长期生效，请在**系统属性 → 环境变量**里添加该目录。

Windows 二进制**没有代码签名**。首次启动时 SmartScreen 可能提示「Windows 已
保护你的电脑」；在校验值匹配之后，选择**更多信息 → 仍要运行**。不要为了运行
Waymark 而全局关闭 SmartScreen。

### macOS：Gatekeeper 与未签名二进制

macOS 压缩包是预编译的，但**没有用 Apple Developer ID 签名**，也**没有经过
公证**。因此 macOS 会把 `waymark` 和 `waymark-mcp` 当作来自未识别开发者的软件。
这是预期行为，不代表下载已损坏。

如果用浏览器下载并解压，首次启动会被 Gatekeeper 拦截，提示类似：

> “waymark” cannot be opened because the developer cannot be verified.

在较新的 macOS 上则是：

> Apple could not verify “waymark” is free of malware that may harm your Mac
> or compromise your privacy.

在确认其 SHA-256 校验值匹配之后，可以这样放行：

1. 打开**系统设置 → 隐私与安全性**。
2. 找到被拦截的 `waymark` 提示，点击**仍要打开**。
3. 完成认证，然后重新运行命令。

Gatekeeper 会逐个检查可执行文件，所以 MCP 客户端第一次启动 `waymark-mcp` 时，
可能也需要用同样的方式放行。

如果隔离属性仍然存在，而你又更习惯命令行，可以从已校验的二进制上移除它：

```sh
xattr -d com.apple.quarantine "$HOME/.local/waymark/waymark" \
  "$HOME/.local/waymark/waymark-mcp"
```

请只在 `.sha256` 校验通过、并且你信任来源之后才这么做。不要为了运行 Waymark
而全局关闭 Gatekeeper。

### 第一批命令

先检查安装，然后创建项目和第一个工单：

```sh
waymark version
# waymark 0.1.0 (cli, mcp)

mkdir -p "$HOME/waymark-demo"
cd "$HOME/waymark-demo"

waymark init
# initialised Waymark project at /home/you/waymark-demo
#   configuration  /home/you/waymark-demo/.waymark/config.toml
#   local settings  /home/you/waymark-demo/.waymark/user.toml
#   category task  work/task

waymark create --title "Try Waymark" --category task --owner "Your Name"
# created wm-...
#   /home/you/waymark-demo/work/task/wm-....md

waymark list
# wm-...  open  ready  Try Waymark
```

`waymark init` 会创建 `.waymark/config.toml`，以及 `work/` 下配置好的分类目录。
`waymark create` 要求提供标题、已声明的分类和负责人。`waymark list` 读取当前
活跃的工单。

`waymark --help` 列出本版本的全部命令；`waymark agent-doc` 会以 Markdown 能力
文档的形式打印同一份命令参考，供 agent 使用。

## Web 视图

Web 视图是项目的只读浏览器视图，只存在于 **Web 包**中；basic 包没有编译它。

### Basic 版本的行为

`waymark serve` 在两种包里都存在，所以这个命令永远不会让人意外：

```sh
waymark serve
# error: <提示应安装哪个 Web 包的说明>
```

basic 构建会以已注册的 `WEB_NOT_INCLUDED` 错误退出（校验类退出码，进程状态 3），
并提示应安装哪个 Web 包。它不会下载、安装或修改任何东西，也不会启动服务器。

### 安装 Web 包

从同一个 release 下载对应平台的 Web 包：

- `waymark-web-v<version>-linux-x86_64.tar.gz`
- `waymark-web-v<version>-macos-aarch64.tar.gz`
- `waymark-web-v<version>-macos-x86_64.tar.gz`
- `waymark-web-v<version>-windows-x86_64.zip`

用与 basic 包相同的方式校验它的 `.sha256`，然后解压到已经存放 basic 安装的目录：

```sh
VERSION=0.1.0
WEB_ARCHIVE="waymark-web-v${VERSION}-linux-x86_64.tar.gz"   # 或对应的 macOS 包
curl -LO "https://github.com/QiYueran-source/waymark/releases/download/v${VERSION}/${WEB_ARCHIVE}"
curl -LO "https://github.com/QiYueran-source/waymark/releases/download/v${VERSION}/${WEB_ARCHIVE}.sha256"
sha256sum --check "${WEB_ARCHIVE}.sha256"
tar -xzf "${WEB_ARCHIVE}" -C "$HOME/.local/waymark"
waymark version
# waymark 0.1.0 (cli, mcp, web)
```

在 Windows 上，下载 `waymark-web-v<version>-windows-x86_64.zip` 及其 `.sha256`，
用上面同样的 `Get-FileHash` 比对方式校验，然后对 basic 安装目录执行
`Expand-Archive $WEB_ARCHIVE -DestinationPath "$HOME\waymark" -Force`。

Web 包替换的是安装目录中的可执行文件，不会触碰任何项目文件。项目的 `.waymark/`
目录和每一份工单文档都保持不变，不需要迁移或重新初始化：basic 包和 Web 包读写
的是同一种项目格式。

### 运行服务器

在项目目录内：

```sh
cd "$HOME/waymark-demo"
waymark serve
# serving http://127.0.0.1:7331
```

打开打印出的 URL。服务只绑定 `127.0.0.1`，没有在局域网或公网接口上暴露的选项。
默认端口是 `7331`。

常用参数：

- `--port PORT` —— 绑定其他端口。`--port 0` 会让操作系统分配一个空闲端口，并
  打印实际选中的端口。
- `--open` —— 服务绑定后在默认浏览器中打开该 URL。Linux 上使用 `xdg-open`，
  macOS 上使用 `open`；如果浏览器无法启动，只会给出警告，服务器继续运行。不加
  `--open` 就不会打开浏览器。
- `--project DIR` —— 从 `DIR` 开始做项目发现，而不是当前目录。这个全局参数放在
  子命令前后都可以。

端口被占用时会以已注册的 `PORT_IN_USE` 错误失败，提示中会包含端口号并建议使用
`--port 0`。Waymark 不会悄悄换到别的端口。

就绪信息在进程阻塞之前打印到 **stderr**：`serving http://127.0.0.1:<port>`。加上
`--json` 时，同一个 URL 会作为版本化信封的 `data` 字段打印到 stdout，进程依旧
阻塞着提供服务：

```sh
waymark serve --json
# {"v":1,"ok":true,"data":{"url":"http://127.0.0.1:7331"},"_meta":{...}}
```

用 `Ctrl-C` 停止服务器。

### 核对 Web 构建的来源

Web 构建内嵌了一份生成的 `assets-manifest.json`。这样查看：

```sh
waymark version --json | jq '.data.assets'
```

清单格式如下：

```json
{
  "version": "0.1.0",
  "git_sha": "<被打 tag 的提交>",
  "build_id": "<前端构建 ID>",
  "files": [{ "path": "index.html", "sha256": "<哈希>", "size": 1234 }]
}
```

清单里的 `version` 必须等于发布版本（`0.1.0`，tag 是 `v0.1.0`），`git_sha` 必须
等于该 tag 指向的提交，`index.html` 条目的 `sha256` 必须等于服务器返回的
`index.html` 字节的 SHA-256。服务器运行中时：

```sh
curl -s http://127.0.0.1:7331/ | sha256sum
```

把结果与 `index.html` 条目的哈希比较。macOS 上用 `shasum -a 256` 代替
`sha256sum`。

basic 构建没有 `assets` 字段：它没有内嵌前端可报告。没有设置 `WAYMARK_GIT_SHA`
的本地构建没有 `git_sha`；发布构建一定有。

## 通过 MCP 连接 agent

两种包都包含 `waymark-mcp`，这是一个走 stdio 的 Model Context Protocol 服务器。
把 MCP 客户端指向该二进制；如果客户端不在项目目录内启动它，再指向项目目录：

```sh
waymark-mcp --project /path/to/project
```

Claude Code、DSH 以及通用 JSON 客户端的可用配置示例见
[`docs/mcp-clients.md`](docs/mcp-clients.md)。

## 从源码构建

仓库的 Rust workspace 位于 `waymark/` 下。basic 构建需要 Rust 工具链（workspace
声明了 `rust-version = "1.85"`）：

```sh
cd waymark
cargo build --release -p waymark-cli
cargo build --release -p waymark-mcp
```

Web 构建会在编译期内嵌前端。先构建前端；Rust 构建本身不会运行 npm：

```sh
cd waymark/crates/waymark-web/ui
npm ci
npm run build
```

`npm run build` 会写出 `crates/waymark-web/ui/build` 及其带哈希的资源。Web 构建
还需要在该目录旁生成的 `assets-manifest.json`；缺少这个文件时，内嵌构建会带着
说明失败，而不是发布一个空的或过期的前端。发布工作流只构建一次前端、生成清单，
并在每个 Web 产物中复用这棵目录树。它设置 `WAYMARK_SKIP_UI_BUILD=1`，让后续
任务复用该目录树；目录树和清单仍然是必需的，这个变量从不会用占位符替代它们。

然后用默认关闭的 `web` feature 构建 CLI：

```sh
cd waymark
cargo build --release -p waymark-cli --features web
```

得到的 `waymark` 是 Web 版本；不带 `--features web` 时是 basic 版本。没有
`full` 这个别名。源码发布包是 `waymark-src-v<version>.tar.gz`，配套一个
`.sha256` 文件；它已经带有预构建的前端，所以不需要下载资源。

## 校验发布

每个发布压缩包都有配套的 `.sha256` 文件，源码包也有。要校验下载的产物，逐个
核对校验值、确认压缩包内容、运行 `waymark version`，并按上文所述检查 Web 构建
的清单与所服务 `index.html` 的哈希。

## 安全

漏洞请勿公开提交 issue。报告渠道、支持版本范围和响应目标见
[`SECURITY.md`](SECURITY.md)。

## 许可证

Waymark 采用 MIT 许可证或 Apache License 2.0 双许可，任选其一。见
[`LICENSE-MIT`](LICENSE-MIT) 与 [`LICENSE-APACHE`](LICENSE-APACHE)。每个发布包
的第三方依赖许可证见压缩包内的 `THIRD_PARTY_LICENSES.txt`。
