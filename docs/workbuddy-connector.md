# 把 Waymark 连接器提交到 WorkBuddy

这份教程从零走到"已提交，等审核"。它只讲能确认的事：平台文档写明的内容，以及从开放平台前端文案里核对到的实际流程。控制台里的按钮文字可能随版本变化，描述和字段名以文档为准。

读之前先知道两件事，它们决定了后面每一步为什么要那样做：

- **WorkBuddy 的个人开发者不能创建连接器。** 控制台对个人主体的原文是「个人开发者后续无法创建/更新连接器，如需创建/更新，请进行企业认证，成为企业开发者」。所以必须走企业认证，或者用一家已有 Tencent Cloud / 微信公众号实名的主体。
- **现在这个包还不能提交。** `connector/cli.json` 的安装命令指向一个不存在的 npm 包，`status` 和 `unAuth` 又依赖两个还不存在的参数。第 1 步就是把它们补上，否则第 4 步上传后跑不起来。

## 第 0 步：准备主体（半天到几天，取决于资质是否现成）

登录 [open.workbuddy.cn](https://open.workbuddy.cn)，点右上角「立即入驻」。

**企业认证**需要：

- 营业执照。系统用 OCR 识别企业名称、统一社会信用代码和法定代表人姓名，核对后填一个对外展示的开发者昵称。
- 管理员实名：本人中国大陆二代居民身份证号、已实名的手机号、能收验证码的邮箱，以及能做人脸识别的手机。
- 第三步验证，三选一：
  - 企业法人本人的扫码人脸核身（法人要配合）；
  - 腾讯云账号快速认证（这家企业已在腾讯云完成企业实名）；
  - 微信公众号 / 小程序账号快速认证。

认证成功后账号成为该企业主体的超级管理员。每个账号最多认证 10 个企业主体，同一家企业最多可以被 50 个账号认证。

如果你个人有中国大陆身份证，也想先看看流程，个人认证也能走通入驻，但**无法创建连接器**，只能用来熟悉界面。别把时间花在这上面。

## 第 1 步：补齐包里的缺口

两个缺口都在仓库里，改完用检查脚本验：

```sh
cd ~/projects/waymark
bash scripts/workbuddy-connector/check.sh
```

**一、安装命令。** `connector/cli.json` 的 `init` 要一条在干净机器上能成功、且不需要 Rust 工具链的命令。WorkBuddy 只托管 Node.js 和 Python 运行时，所以现在写的 `npm install -g waymark-mcp` 是占位，那个包不存在。可选做法：

- 项目托管一个安装脚本（发布文档里常写成 `curl … | sh`），`init` 调用它；
- 或者发布一个 npm 包装包，安装时下载对应平台的二进制。

两样都还没有，这是提交前必须做的决定。

**二、两个参数。** `status` 和 `unAuth` 分别调用 `waymark-mcp --ready` 和 `waymark-mcp --forget`，二进制里还没有。Waymark 没有登录态，这两个命令的实际含义是：

- `--ready`：当前目录之上有 Waymark 项目就输出 `OK` 并以 0 退出，否则非 0；
- `--forget`：什么都不做，直接以 0 退出（幂等，登出时被调用）。

顺带一提，`cli.json` 里没有配 `auth`。Waymark 不需要登录，文档允许省略，但平台审核时可能追问授权方式，准备好一句解释。

## 第 2 步：打包

平台收的是一个**连接器能力包**，不是编译好的程序。上传页的原文是「上传压缩包，平台自动开包解析并生成连接器 ID」，解析的就是你包里那几份描述文件：`connector-meta.json`、`cli.json`、`icon.svg`、`skills/`。

所以发布页里 `waymark-v0.1.3-linux-x86_64.tar.gz` 这类归档**不能直接上传**。它里面是 `waymark`、`waymark-mcp`、README 和许可证，没有 `connector-meta.json`，平台开包会报「未找到 connector-meta.json」。

把二进制塞进包里也没用。CLI 连接器的流程是「用户点连接 → WorkBuddy 执行 `init` 安装 → 执行 `status` 判断状态」，它不知道包里躺着什么二进制。`init` 写的是 `npm install -g …` 这种无人值守命令，平台只会执行它。

只有 `.zip`，连接器不超过 20MB，包内必须有 `connector-meta.json`。

```sh
cd ~/projects/waymark/connector
zip -r ../waymark-connector-0.1.3.zip . -x '.*' -x '*/.*'
```

打出来检查一遍结构：

```text
connector-meta.json
cli.json
icon.svg
skills/waymark-work-items/SKILL.md
```

压缩包里的路径必须是相对根目录的，不能多一层 `connector/` 目录。把上面那条 `zip` 在 `connector/` 目录里执行就是对的。这个包只有几 KB，离 20MB 上限很远。

如果你原本想的是「把我编译好的东西传上去」，那条路在 CLI 连接器里是 `init` 命令去下载。这里有两个数据点：release 里的 linux 二进制压缩前分别是 5.1MB（`waymark`）和 7.7MB（`waymark-mcp`），整个归档压缩后 4.4MB。一个 zip 装三平台两个二进制大约十几 MB，**勉强低于 20MB，但平台不认**：连接器包里放二进制没有意义，WorkBuddy 不会去执行它。

## 第 3 步：进控制台创建连接器

登录 [open.workbuddy.cn](https://open.workbuddy.cn)，进入开放平台控制台（左侧「发布管理」或其下的「连接器」）。

点「创建」，选择「连接器」。平台会让上传压缩包，**它自己开包解析，生成连接器 ID**，不需要你填写 JSON。

## 第 4 步：上传与分发范围

在配置页拖入 `waymark-connector-0.1.3.zip`。上传时会校验：

- 文件类型、扩展名与内容一致（伪造后缀会被拒）；
- 压缩包能解开；
- 必填入口文件存在（连接器是 `connector-meta.json`，认证配置读不出来会单独报错）；
- 不超过 20MB。

接着选分发范围，可多选：

- **WorkBuddy 市场**：公开市场，任何用户能安装；
- **WorkBuddy 企业市场**：面向指定企业客户。

两种渠道审核方式相同：安全扫描 + AI 审核 + 人工抽检。

## 第 5 步：完善展示信息

展示信息大部分由能力包解析，不可改的字段只供核对，带 `*` 的要补。连接器这几个字段要提前准备好：

| 字段 | 要求 |
| --- | --- |
| 市场展示名称 | 必填，市场里显示的名字 |
| 市场展示分类 | 必选 1–3 个 |
| 版本号 | 包内版本，这里是 `0.1.3` |
| 头像 | 512×512px，不超过 500KB，仅 JPG/PNG |
| 类目 | 必选；某些类目要上传资质材料 |
| 厂商名称 | 提供该连接器的厂商 |
| 帮助文档地址、定价页地址 | `https://` 开头 |

**头像这一项要从 `connector/icon.svg` 转出来**：平台要的是 512×512 的 PNG/JPG，不是 SVG。仓库里的 `scripts/workbuddy-connector/build-icon.sh` 能渲染，但那台机器上需要有 `rsvg-convert`、`inkscape` 或带 SVG 支持的 ImageMagick：

```sh
sudo apt install librsvg2-bin          # 提供 rsvg-convert
bash scripts/workbuddy-connector/build-icon.sh --render   # 生成 64 和 400 像素的 PNG
# 512×512 供平台用：
rsvg-convert -w 512 -h 512 connector/icon.svg -o icon-512.png
```

## 第 6 步：连接自测

连接器如果是远程「服务端认证」的 MCP，要完成一次授权跳转自测才能进下一步。**我们的包是 CLI 连接器，不需要认证判定，属于可跳过类型**：平台文案写的是「无需认证 / 客户端认证」「用户自填凭证」与 stdio 本地进程可跳过自测。页面会显示「当前连接器无需联通自测」。

如果页面坚持要求自测，说明包的类型没被识别成 CLI，检查 `connector-meta.json` 里的 `"type": "cli"`。

## 第 7 步：提交审核

信息补完后点提交。之后状态依次是：

```text
草稿 → 审核中 → 待发布 → 已发布
                ↘ 已驳回
```

审核分四段，前三段并行、全部通过后才进最后一段：

| 环节 | 内容 |
| --- | --- |
| 基础安全 | 代码与包结构的安全扫描 |
| 内容安全 | 文案与示例的合规检查 |
| 类目 | 类目是否匹配 |
| 质量评测 | 需要授权配置做端到端能力验证；平台会用你提供的账号实际调用一次 |

**质量评测这一段要注意。** 平台会要求你提交授权配置，用一个真实账号跑端到端验证。Waymark 的连接器没有登录，授权是「无需授权」，但仍然需要你提供一个 WorkBuddy 账号登录，让它实际跑一次 CLI。验证会产生积分消耗，凭证在验证完成后删除。CLI 连接器的评测需要一个能真实跑通的安装命令，这也是第 1 步必须先做完的原因。

驳回时状态变「已驳回」，页面会给失败原因。

## 第 8 步：发布

审核通过后连接器进入「待发布」。这时你点「发布」，平台提示：

> 确认发布后，连接器将由运营同学人工上架，上架后将公开展示在 WorkBuddy 市场中，所有用户均可发现和使用。

也就是说，审核通过不等于已经上架，还要过一道**人工上架**。操作由运营完成，你等结果就行。

## 更新版本

已经上架的连接器要发新版：改包内文件，把 `connector-meta.json` 的 `version` 和 `cli.json` 里引用的版本一起递增，重新打包上传，再走一遍审核。Waymark 每次发版，这个包的 `version` 都要跟着改。`scripts/workbuddy-connector/check.sh` 目前只校验文档里写明的字段规则，不比对包内版本和仓库版本，改版本时要自己确认。

## 整条路的时间预估

| 步骤 | 时间 |
| --- | --- |
| 企业认证 | 资质现成的话当天，否则看审核 |
| 补 `--ready` / `--forget` | 几十行代码，一次发版 |
| 安装命令 | 取决于你想怎么做，这是最大的一块 |
| 打包、上传、填表 | 一小时 |
| 平台审核（安全 + 质量评测） | 几天 |
| 人工上架 | 看运营排期 |

## 遇到的问题往哪问

- 平台问题：`openworkbuddy@tencent.com`（[联系我们](https://open.workbuddy.cn/docs/contact)）
- 连接器开发规范：[open.workbuddy.cn/docs/connector](https://open.workbuddy.cn/docs/connector)
- 认证规则：[入驻指南](https://open.workbuddy.cn/docs/onboarding)

## 只做一件事的替代方案

上架是分发，不是唯一的路。如果你只是想让 WorkBuddy 能用上 Waymark，客户端本身支持手动配置自定义连接器：用户把自己的 `waymark-mcp` 路径填进去就行，不需要审核，也不需要等这个包完成。做法和 [Connecting MCP clients](mcp-clients.md) 里通用 JSON 客户端的配置一样，`command` 填 `waymark-mcp` 的绝对路径，`args` 里给 `--project`。
