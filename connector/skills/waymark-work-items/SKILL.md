---
name: waymark-work-items
description: 在仓库里读写 Waymark 工作项 —— 查看可以开工和被阻塞的任务、阅读依赖与层级结构、创建和修改工作项。所有操作通过 waymark 命令行执行。
version: 0.1.3
author: QiYueran-source
---

# Waymark 工作项

Waymark 把工作项存成仓库里的 Markdown 文档，每个项目在根目录有一个 `.waymark/config.toml`。
工作项有稳定 ID（形如 `wm-bc4wad5k69bv`）、标题、类别、状态、负责人，以及依赖、父项、软链接关系。

所有操作通过 `waymark` 命令行执行。命令已由连接器安装，直接调用即可，不需要再安装。

## 先确认项目在哪

```bash
waymark project info
```

返回项目根目录、配置路径、工作项数量和格式版本。如果提示找不到项目，说明当前工作目录不在 Waymark 项目里：请用户在目标仓库执行
`waymark init`，或在命令后追加 `--project <仓库路径>`。

**所有命令都可以加 `--json`**，输出带版本号的结构化信封，解析时优先用 JSON：

```json
{"v":1,"ok":true,"data":{...},"_meta":{...}}
```

失败时 `ok` 为 `false`，`error.code` 是稳定的错误码（如 `ITEM_NOT_FOUND`、`PROJECT_NOT_FOUND`、
`INCOMPLETE_PROJECTION`），`error.retryable` 表示重试是否有意义。命令的退出码按错误类别区分。

## 读：先看能做什么

```bash
# 现在可以开工的任务（依赖都已关闭）
waymark ready --json

# 被阻塞的任务，以及各自卡在什么上
waymark blocked --json

# 全部活跃工作项，按状态和类别过滤
waymark list --json
waymark list --status open --category task --json

# 单个工作项
waymark show wm-bc4wad5k69bv --json

# 层级树（限制层数）与依赖树
waymark tree --depth 2 --json
waymark dep tree wm-bc4wad5k69bv --json

# 搜索标题、frontmatter 和正文
waymark search "登录" --json

# 只要计数
waymark stats --json
```

`ready` / `blocked` / `list` 的 `data.items[]` 里，`item` 是文档本身（`id`、`title`、`category`、
`status`、`owner`），同级字段给出 `ready`、`blocked`、`blocked_by`、`dependents`、`children`。

## 写：改动工作项

| 目的 | 命令 |
| --- | --- |
| 新建 | `waymark create --title "标题" --category task --owner frank` |
| 改标题 | `waymark title set <id> "新标题"` |
| 改正文 | `waymark body replace <id> --body-file notes.md` |
| 改状态 | `waymark status set <id> in_progress` |
| 快捷状态 | `waymark start <id>` / `waymark close <id>` / `waymark reopen <id>` |
| 换负责人 | `waymark owner transfer <id> bob` |
| 换类别 | `waymark move <id> bug` |
| 加依赖 | `waymark dependency add <id> <blocking-id>` |
| 设父项 | `waymark parent set <id> <parent-id>` |
| 加软链接 | `waymark link add <id> <other-id>` |
| 归档 | `waymark archive <id>` |

新建会返回一个稳定 ID 和文件路径，记下这个 ID，后续操作用它。

## 使用规则

- **先读后写。** 改动前用 `show` 确认 ID 和当前状态；不要凭标题猜测 ID。
- **改正文会整体替换。** `body replace` 覆盖整段正文，先 `show` 读出原文、改好再写回，不要只写增量。`--body -` 从标准输入读取。
- **依赖方向。** `dependency add A B` 表示 A 等 B 完成。方向搞反会让 `ready` 和 `blocked` 给出相反答案。
- **一次一个改动。** 批量修改时分次执行并检查每条的结果，失败时停下来说明哪一条出错。
- **不要绕开 CLI 直接改文件。** 文档的 frontmatter 有格式约束，手写会破坏项目，`waymark validate` 会报错。
- **写入是本地的。** 只改动当前仓库里的文件，不会推送到远端；是否提交 Git 由用户决定，需要时先问。

## 常见错误

| `error.code` | 含义 | 处理 |
| --- | --- | --- |
| `PROJECT_NOT_FOUND` | 当前目录之上没有 `.waymark/config.toml` | 让用户 `waymark init`，或加 `--project` |
| `ITEM_NOT_FOUND` | ID 不存在 | 用 `waymark list --json` 重新找 |
| `INCOMPLETE_PROJECTION` | 有文档损坏，整体视图不可信 | 运行 `waymark validate --json` 报给用户，不要自行修文件 |
| `LOCK_TIMEOUT` | 另一个进程正在写 | 稍后重试；持续失败时告诉用户有并发写入 |
