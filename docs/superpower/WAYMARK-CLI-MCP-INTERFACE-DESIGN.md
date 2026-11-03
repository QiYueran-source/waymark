# Waymark CLI 与 MCP 核心能力设计说明

> 状态：核心能力边界已确认；具体参数、完整 schema、批量语义和迁移接口仍待细化。
>
> 本文是 CLI/MCP 外部接口设计的工作文档。它记录已经确认的能力边界，不替代 [Waymark Functional Specification](./WAYMARK-FUNCTIONAL-SPEC.md)；在接口细节完成后，再同步主功能规格。

## 1. 目标与原则

Waymark 对外提供 CLI 和 MCP 两种正式接口。两者必须调用同一个共享函数库；核心能力由共享函数定义，CLI 和 MCP 只是不同的适配器。

两种接口的业务能力不人为分叉：

- CLI 和 MCP 都可以访问已确认的项目维护、工作项读取、查询和写入能力；
- 两者执行相同的 schema、关系、owner/operator、归档、并发、批量、迁移和诊断规则；
- CLI 可以提供人类友好的命令层级、文本输出、快捷命令和退出码；
- MCP 使用结构化 tool、请求/响应和业务错误；
- 交互方式和传输表示可以不同，但业务语义不能不同；
- Web 仍然是独立的只读总览，不是 CLI/MCP 写入适配器。

## 2. 共享函数库边界

共享函数库是唯一的业务语义入口。CLI 命令和 MCP tool 不得各自实现一套字段校验、关系规则、授权规则或文件写入规则。

### 2.1 项目与维护能力

核心能力包括：

```text
project_init
project_info
config_read
config_validate
config_update
project_validate
doctor
sync_projection
migration_plan
migration_apply
git_history
git_diff
```

配置修改也属于共享函数能力，并由 CLI/MCP 平等暴露。配置变更必须使用结构化操作，经过配置校验、并发检查和必要的迁移判断；不提供无限制的任意路径 raw patch。

迁移的具体分类和接口仍待单独细化。最终应至少区分兼容版本、配置/schema 变化和存储位置变化，并提供 dry-run/plan、预校验、备份、冲突和恢复报告。

### 2.2 工作项读取与查询能力

```text
work_item_get
work_item_list
work_item_search
work_item_query
work_item_tree
work_item_children
work_item_leaves
work_item_ready
work_item_blocked
work_item_stats
```

集合查询默认只返回活动工作项；显式 archived 范围只选择归档项，不与活动项隐式混合。单项 `work_item_get` 可以按稳定 ID读取活动或归档工作项。

全局关系查询、树、ready、blocked、stats、归档和跨文件关系写入遵守完整且新鲜投影要求。查询结果应携带来源、完整性、新鲜度和诊断元数据。

### 2.3 创建与普通内容编辑

```text
work_item_create
work_item_set_title
work_item_replace_body
work_item_update_custom_fields
```

当前编辑能力按数据区域拆分：

- `title` 是唯一允许通过普通内容编辑表面修改的核心 frontmatter 字段；
- `body` 是 frontmatter 后的 Markdown 正文；
- `custom_fields` 是项目声明的自定义 frontmatter 字段；
- 其他核心字段不允许作为任意 frontmatter patch 修改；
- 状态、owner、类别、parent、dependencies、soft_links 和 archive 使用各自的专用领域操作；
- `id`、`creator`、`created_at`、`updated_at`、`closed_at`、`archived_at` 等系统/身份字段不能由普通编辑修改；
- `ready`、`blocked`、`blocked_by` 等计算结果永远不可写。

这三个编辑函数的参数、clear/unset 规则、CLI 编辑器行为和批量能力仍待细化。当前已确认的是：它们不是一个任意 frontmatter patch 函数。

### 2.4 生命周期、责任与关系能力

```text
work_item_set_status
work_item_transfer_owner
work_item_move_category
parent_set
parent_clear
dependency_add
dependency_remove
soft_link_add
soft_link_remove
work_item_archive
```

这些操作拥有专门业务规则，不能由普通字段编辑绕过：

- status 操作维护 `closed_at` 和 `updated_at`；
- owner 转移只能由当前 owner 发起，并立即改变后续写入边界；
- category 移动保持稳定 ID，并检查目标冲突；
- parent、dependency 和 soft link 操作执行存在性、重复、自引用、对称性和环校验；
- archive 是不可逆的 Waymark 逻辑删除，并执行归档专属的子项、关系清理、投影和批次规则。

## 3. CLI 核心命令面

以下是当前确认的能力映射，命令拼写仍可在接口细化阶段调整。

### 3.1 项目与诊断

```text
waymark init
waymark project info
waymark config show
waymark config validate
waymark config update ...
waymark validate
waymark doctor
waymark sync
waymark migrate plan ...
waymark migrate apply ...
waymark git history ...
waymark git diff ...
```

CLI 负责把人类参数转换成共享函数请求，并将结构化业务错误映射为稳定退出码。破坏性、批量或项目级操作应支持 dry-run 或 plan/apply 机制，具体范围待细化。

### 3.2 工作项读取与查询

```text
waymark show <id>
waymark list
waymark search <query>
waymark query ...
waymark tree [<id>]
waymark children <id>
waymark leaves
waymark ready
waymark blocked
waymark stats
```

查询需要显式表达活动/归档范围、排序、分页和投影字段；具体参数尚未定稿。

### 3.3 工作项和编辑

```text
waymark create ...
waymark title set <id> <title>
waymark body replace <id> --body <text>
waymark body replace <id> --body-file <path>
waymark field update <id> --set <key=value>... --clear <key>...
```

当前设计将三类普通编辑能力分别暴露：

- `title set` 修改 title；
- `body replace` 完整替换或清空正文，`--body` 与 `--body-file` 互斥；
- `field update` 一次更新一个工作项的多个自定义字段，支持设置和清除。

是否恢复 Clove 风格的 CLI `$EDITOR`、是否提供人类友好的复合 `edit` 命令、以及跨领域复合编辑是否原子，仍待编辑契约确认。

### 3.4 专用核心操作

```text
waymark status set <id> <status>
waymark owner transfer <id> <new-owner>
waymark move <id> --category <category>
waymark parent set <id> <parent-id>
waymark parent clear <id>
waymark dependency add <id> <blocking-id>
waymark dependency remove <id> <blocking-id>
waymark link add <id> <other-id>
waymark link remove <id> <other-id>
waymark archive <id> [--reason <text>] [--dry-run]
```

`start`、`close`、`reopen` 等快捷命令可以作为 CLI 对 `status set` 的适配，不构成新的领域能力。是否保留这些别名待命令树定稿时决定。

## 4. MCP 核心 tool 面

MCP tool 直接映射共享函数能力，不启动编辑器，不接受未经约束的 frontmatter 原文 patch。

### 4.1 项目与维护

```text
waymark_project_init
waymark_project_info
waymark_config_read
waymark_config_validate
waymark_config_update
waymark_validate
waymark_doctor
waymark_sync_projection
waymark_migration_plan
waymark_migration_apply
waymark_git_history
waymark_git_diff
```

CLI 与 MCP 对这些能力不做产品级区别；确认、dry-run、plan/apply、锁、冲突和恢复语义由共享函数统一定义。

### 4.2 读取与查询

```text
waymark_get_work_item
waymark_list_work_items
waymark_search_work_items
waymark_query_work_items
waymark_get_tree
waymark_get_children
waymark_get_leaves
waymark_get_ready_items
waymark_get_blocked_items
waymark_get_stats
```

所有 tool 使用结构化请求和版本化响应 envelope。集合查询的 active/archived 范围、分页、排序和投影参数待细化。

### 4.3 创建、编辑和专用写入

```text
waymark_create_work_item
waymark_set_title
waymark_replace_body
waymark_update_custom_fields
waymark_set_status
waymark_transfer_owner
waymark_move_work_item
waymark_set_parent
waymark_clear_parent
waymark_add_dependency
waymark_remove_dependency
waymark_add_soft_link
waymark_remove_soft_link
waymark_archive_work_item
```

MCP 的编辑 tool 与 CLI 的三个编辑命令共享对应函数：

- `waymark_set_title`；
- `waymark_replace_body`；
- `waymark_update_custom_fields`。

MCP 不启动 `$EDITOR`。它使用结构化字符串和字段变更请求，具体 schema、clear 语义、批量形式和返回内容待细化。

## 5. 统一业务契约

CLI 和 MCP 共享以下约束：

- 受控写入检查当前配置的 operator 与 work item owner；
- archived item 拒绝普通写入；
- 写入使用 fingerprint 冲突检测；
- 单文件更新经过校验后原子替换；
- 批量操作先整体预校验，按操作类型执行，默认 all-or-nothing；
- 归档、迁移和配置变更使用其专属完整性和恢复规则；
- 系统时间由 Waymark 生成，普通请求不能伪造；
- 机器响应使用版本化 envelope；
- 错误使用共享稳定 code、结构化 details、location 和 retryable；
- CLI 只把共享错误映射为退出码，MCP 不创建第二套业务错误分类。

## 6. 暂未定稿的接口问题

本文不提前决定以下内容：

- 每个命令和 tool 的最终名称；
- title、body、自定义字段操作的精确参数和 clear/unset 语义；
- CLI 是否支持 `$EDITOR`；
- 是否提供复合 `edit` 命令，以及跨领域编辑是否原子；
- 自定义字段 CLI 值如何解析为 integer、boolean 和 list；
- 读取查询的分页、排序、投影和 archive 参数；
- 批量操作的完整请求形状；
- 配置修改和迁移的具体 kind、plan/apply schema；
- Git 能力是否在两个适配器中使用完全相同的参数。

这些问题属于下一轮接口细化，不改变本文已经确定的共享函数库、CLI/MCP 能力平等和编辑领域分拆原则。
