# Waymark CLI 功能清单（提案）

> 状态：需求分析稿，尚未实现。本文基于当前 Wayfinder、Spec、实现 Tickets 的协作需求，以及对 Clove 0.2.0 命令面和源码结构的分析整理。
>
> 暂定工具名：**Waymark**，CLI 命令：`waymark`。含义是标记一条通往目标的路线，贴合 Wayfinder 的规划语义。此名仅为提案，未做商标、包名或命令冲突检索，正式采用前需确认。

## 1. 产品目标

Waymark 是本地优先、Git 友好的项目工作项 tracker，以可读的 Markdown + YAML frontmatter 文件作为权威数据源，并以 Clove 的命令能力为功能基线。它管理项目自定义的工作项类别，支持可配置的 frontmatter schema、负责人和 Agent 的分离、父子层级与独立阻塞关系，并针对多类别、多 Agent 并行工作优化。Wayfinder 地图、决策票、Spec 和实现 Tickets 是推荐的使用方式，不是内置的固定类别集合。

Waymark 不把 issue 输出目录硬编码为 `tickets/`，也不要求所有项目使用同一套目录。`.waymark/config.yml` 中由项目指定存储根目录和各类别的相对路径。例如，下面只是一个**可选的初始化示例布局**：

```text
repo/
  .waymark/
    config.yml
  tickets/
    wayfinder/
      <id>.md
    spec/
      <id>.md
    implementation/
      <id>.md
```

项目可以只配置需要的类别，也可以创建任意数量的自定义类别。例如，只使用 `spec` 和 `implementation`：

```yaml
storage:
  root: work-items

categories:
  spec:
    path: specs
    display_name: Specifications
  implementation:
    path: implementation
    display_name: Implementation
```

也可以增加项目自己的类别：

```yaml
categories:
  research:
    path: research
  decision:
    path: decisions
  spec:
    path: specs
  implementation:
    path: tickets
```

项目也可以配置为其他布局，例如：

```text
repo/
  .waymark/
    config.yml
  planning/
    maps/
      <id>.md
    specs/
      <id>.md
  engineering/
    tickets/
      <id>.md
```

配置至少分别定义：

- `storage.root`：相对仓库根目录的存储根目录，可配置。
- `categories.<name>.path`：类别相对于 `storage.root` 的输出目录，可配置。
- 类别名称与 frontmatter 中类别字段的映射。

如果 `storage.root` 和类别路径都允许配置为任意相对路径，则不能把 `tickets/{wayfinder,spec,implementation}` 称为固定默认布局。它只能作为 `waymark init` 生成的示例布局，用户可以在初始化时选择预设，或直接修改配置。`.waymark/config.yml` 本身的位置保持固定，便于项目发现；issue 数据目录不固定。每张票的 frontmatter 写入 `frontmatter_version`，供校验和迁移使用。

配置变更本身不自动增加版本号。`config_version` 表示配置文件语法/语义版本，只有工具发生不兼容配置格式变更时增加；`frontmatter_version` 表示 issue frontmatter 格式版本，在数据格式发生迁移时按明确规则增加，而非每次编辑 issue 都递增。

## 2. 设计原则和边界

1. **文件是事实来源**：索引、缓存和 Web 数据都可重建，不能成为第二份权威状态。
2. **关系语义分离**：类别/存储路径、parent-child 层级、depends-on 阻塞分别表示，目录嵌套不隐含票据关系。
3. **可配置但语义稳定**：用户可以配置字段和类别；核心关系字段必须有明确定义，或者由配置显式映射到核心语义。
4. **统一写入与校验**：CLI 和 MCP 调用同一领域写入服务，不各自实现字段校验和关系规则；未来 Web 首期只读，不提供写入服务。
5. **只读进度视图**：Web 是后续阶段能力，首期不实现；未来 Web 也必须只读，不能只隐藏 UI 按钮。
6. **权限能力诚实标注**：本地文件模式下，CLI 的负责人检查属于流程/工具约束，不能阻止直接改文件。真正强制权限需要可信身份和受控写入入口。
7. **渐进交付**：第一阶段完成 CLI + MCP + 文件格式 + 校验，专注核心数据和协作能力；后续再加只读 Web、索引、TUI 和外部同步，避免首版照搬 Clove 的全部子系统。

## 3. 配置能力

`waymark init` 生成 `.waymark/config.yml`。配置至少描述：

- `config_version`：配置文件格式版本。
- `frontmatter_version`：新建票据期望使用的 frontmatter 格式版本。
- `operator`：当前通过 Waymark 命令操作票据的人类身份。每个项目配置一个 operator，用于与目标票据的 `owner` 做一致性检查；它是本地工作流护栏，不是可信认证凭据。
- 项目自定义的类别集合及其目录映射。类别数量没有固定限制，可以是两类、三类、四类或更多。
- 每个类别的稳定名称、显示名称、说明、相对输出路径和默认字段值。
- 类别是否允许创建、是否归档，以及是否参与默认列表/统计视图。
- 可选的类别预设，例如 `wayfinder`、`spec`、`implementation`，预设只是初始化辅助，不是系统保留类别。
- 类别默认值与类别别名（可选）。
- 字段名称、类型、必填/可选、默认值、枚举值、集合元素类型。
- 类别内 name 唯一性规则；不同类别允许同名，由 `category/name` 组成可读引用。
- `id`、类别、可变 `name`、显示标题、状态、创建者、负责人、Agent 执行者、parent、依赖等核心字段映射；ID 不允许重命名或改写。
- 全局核心状态的项目显示名称及其固定语义组映射，例如 open、in_progress、closed；ready/blocked 逻辑依据固定语义组，核心语义不可由项目重定义。
- `category_status` 作为可选的统一字段名；每个类别可配置自己的枚举值集合。它是可读写的普通类别状态，不映射或替代核心状态，不参与 ready/blocked、依赖解除、通用统计或核心状态迁移逻辑；其枚举值不得与全局核心状态名称重合。
- 项目级 ID 前缀等运行设置；Web 绑定地址/端口属于后续 Web 阶段，不纳入第一阶段配置。

`operator` 首版直接声明在 `.waymark/config.yml` 中，不支持每次命令通过 `--actor` 或环境变量覆盖。这样规则简单、可审查，也与当前“operator 是流程护栏而非认证”的定位一致。多人使用同一仓库时，应使用各自工作副本或在操作前明确修改该配置。

首版字段类型建议：`string`、`boolean`、`integer`、`number`、`date`、`datetime`、`enum`、`list`、`reference`。`reference` 仅接受已存在的 Waymark ID 或可唯一解析的 `<category>/<name>`，写入时规范化为稳定 ID。首版不支持内嵌脚本或任意用户函数。若配置中某个已有自定义字段的定义被删除、改名或改成不兼容类型，且票据数据因此与当前 schema 冲突，Waymark 仍允许读取和展示，但拒绝对该票据执行任何写操作，直到显式修复或迁移。直接编辑 Markdown 不受 Waymark 限制。

同票并发写入采用乐观并发检测：Waymark 读取票据时记录文件版本/哈希，提交修改前确认文件未被其他写者改变；如果已变化，则拒绝本次写入并要求重新读取后重试，不自动合并、不静默覆盖。该策略适用于 CLI 和 MCP 等通过统一写入服务的入口。

路径配置使用相对仓库根目录的路径；解析后拒绝逃逸仓库根目录的路径、绝对路径和不安全的符号链接。一个 issue 的稳定 ID 不因类别或路径移动而改变。目录迁移必须有显式命令及 dry-run。

核心字段使用固定 YAML key，不通过配置重命名；这样 CLI、MCP、迁移、诊断和导入导出可以共享稳定数据模型。普通扩展字段可自定义，不自动改变工作流；如需参与查询或展示，应被 schema 声明并通过校验。

## 4. Issue 数据与行为要求

每条 Issue 至少支持：

- 稳定且全局唯一、创建后不可变的 `id`。ID 是父子关系、依赖、导入导出和自动化引用的规范身份，不编码可变类别或名称。首版不提供评论实体。
- `frontmatter_version`。
- 类别 `category`、可变的用户命名 `name`、显示标题 `title`、状态、正文 Markdown。

类别 `category` 由项目配置定义；移动类别或修改类别路径不改变 ID。`name` 是用户可设置和修改的可读短名，与自然语言显示用的 `title` 分开：name 用于短引用，title 用于描述。同一 category 内的 name 必须唯一，不同 category 可以使用相同 name；创建时由用户显式提供，不自动从 title 生成。类别移动或重命名后，新的 `<category>/<name>` 查找引用随之变化。稳定 ID 始终是规范引用，不保证改名后旧引用仍可用。第一版不维护历史别名。
- `created_by`：创建者身份，创建后保留来源。
- `owner`：必填的人类负责人。Waymark 写操作时要求配置中的 `operator` 与此字段相等。
- `executor`：可选 Agent 执行者，和 owner 分离，可清空、改派；Agent 身份动态变化，不要求列入固定 operator 配置。
- 可选的类别自定义状态字段：按类别 schema 定义为枚举，可读取、展示并通过通用字段设置命令修改；名称不得与核心状态冲突，不参与核心状态计算或迁移。
- `parent`：可选父项 ID。
- `depends_on`：零个或多个硬阻塞依赖 ID。
- 配置声明的扩展字段。
- 创建/更新时间；如需要关闭时间，状态变更时保持一致。

核心状态名称可由项目配置自定义显示名称，但每个状态必须映射到固定语义组（如 open、in_progress、closed），通用行为依据语义组工作；项目不能重新定义核心语义。类别自定义状态统一写入 `category_status` 字段，类别之间可使用各自枚举集合，该字段只是普通可编辑状态，不做隐式映射。配置校验要求 category_status 枚举值不与核心状态名称冲突。父子边与依赖边分别做环检测；依赖目标、父项和跨类别引用需校验存在性及项目范围。

负责人/Agent 改派、状态变更、字段更新应可选记录审计事件。Git 历史是基础审计来源，应用层变更日志是后续增强，不应首期伪造不可篡改保证。

### 操作人检查与写入边界

项目配置在 `.waymark/config.yml` 中声明一个 `operator`，每张票据 frontmatter 声明 `owner`。所有经 Waymark 提供的票据写操作，必须先比较 `config.operator` 与目标票据的 `owner`：相等才执行，不相等则拒绝并报告两者。批量操作先校验所有目标及并发版本，再统一提交；任一项失败时全批次不产生修改。每个项目配置一个 operator，因此同一工作区同时面向不同人类操作者时，应分别使用各自配置/工作副本，不通过多套 `--actor` 参数伪装身份。

创建票据时，`created_by` 默认记录当前配置的 `operator`，`owner` 必须明确指定或由类别默认值提供。父票负责人可以创建子票，但对子票的后续修改仍按子票自身 owner 检查，父票 owner 不继承对子项或后代的权限。

检查覆盖所有会改变票据数据的入口：字段编辑、状态变更、owner/executor 变更、类别移动、parent/dependency/软关联调整、归档和删除。首版无评论写入入口。owner 改派本身也必须由当前 owner 执行，且更新后后续写操作以新 owner 为准。纯只读命令不要求 operator 与 owner 匹配。

该规则只约束 Waymark CLI/MCP 等受控写入入口。用户仍可直接编辑 Markdown、修改配置中的 operator 或绕开工具，因此这是防误操作的流程护栏，不是认证、不可伪造的身份系统或文件访问安全边界。

## 5. CLI 命令清单

全局约定：命令支持 `--format human|json`；机器输出使用稳定版本化 JSON envelope；错误包含明确代码、上下文和非零退出码。所有读命令允许 `--project`（默认当前项目发现），必要时支持 `--config` 显式配置路径。破坏性或批量命令提供 `--dry-run`。

### 5.1 初始化、配置与诊断

| 命令 | 能力 |
|---|---|
| `waymark init` | 初始化 `.waymark/`、示例配置及按配置生成的分类目录。重复运行不覆盖用户配置。可选 `--name`、`--id-prefix`、`--operator`。 |
| `waymark config show` | 展示解析后的有效配置及来源。 |
| `waymark config validate` | 校验配置版本、字段定义、保留字段映射、路径安全、状态语义、类别冲突，以及自定义字段与核心字段重名或语义冲突。 |
| `waymark validate` | 校验全库 frontmatter、版本、字段值、ID 唯一性、引用完整性、关系环、路径归属、自定义字段冲突及核心字段完整性。支持 `--category`、`--id`。 |
| `waymark doctor` | 检查文件布局、损坏票据、悬空引用、缓存状态和潜在迁移问题；识别自定义字段与核心字段冲突并报错；仅自动修复安全、可逆的非权威缓存。 |
| `waymark version` | 输出工具版本、支持的配置版本和 frontmatter 版本范围。 |
| `waymark completion <shell>` | 生成常用 shell 补全（后续）。 |

### 5.2 创建、读取、编辑、归档

| 命令 | 能力 |
|---|---|
| `waymark new <title>` | 按类别创建票据并生成不可变稳定 ID；支持 `--category`、必填的 `--name`、`--owner`、`--executor`、`--parent`、`--depends-on`、`--field key=value`、`--body`。`name` 在类别内唯一，不自动从标题推导；非交互时缺必填值应报错。 |
| `waymark show <ref>` | 按稳定 ID 或唯一的 `<category>/<name>` 别名展示票据详情、配置定义的字段、父子/依赖关系及 readiness。支持 `--format json`、`--fields`；歧义别名拒绝解析并提示使用 ID。 |
| `waymark edit <ref>` | 交互式或结构化编辑票据。先检查 `operator == owner`，再通过 schema 校验。 |
| `waymark set <ref> <key=value...>` | 非交互式字段修改，可用于 Agent 自动化；同样执行 operator/owner 检查。 |
| `waymark rename <ref> --name <name>` | 修改类别内可读短名，ID 和既有关联不变；校验同类别内唯一性。旧名称不保留为别名。 |
| `waymark move <ref> --category <category>` | 移动票据到其他配置类别目录，保持 ID 和既有关联不变；校验目标类别内 `name` 不冲突；支持 `--dry-run`。移动后 `<category>/<name>` 查找别名随之变化。 |
| `waymark archive <id>` | 按配置归档/标记退役，不默认物理删除。 |
| `waymark delete <id>` | 删除前检查子项、依赖和引用；必须显式确认，支持 `--force` 但不得静默破坏引用。首期可以不提供。 |

### 5.3 状态、角色与关系管理

| 命令 | 能力 |
|---|---|
| `waymark status <id> <status>` | 修改状态；支持对多个 ID 批量操作及 `--reason`。每个目标票据均须通过 operator/owner 检查，状态变化遵守配置的状态语义。批量操作默认全有或全无，任一目标校验失败则不写入任何票据。 |
| `waymark start <id...>` | 快捷设置为配置中映射的进行中状态。 |
| `waymark close <id...>` | 快捷设置为完成状态；若配置没有完成态则拒绝。 |
| `waymark reopen <id...>` | 重新打开已完成项。 |
| `waymark owner <id> <person>` | 由当前 owner（即 operator）设置/更换必填的人类负责人，记录改派信息；改派成功后新 owner 才能继续执行写操作。 |
| `waymark executor <id> <agent>` | 由当前 owner（即 operator）设置可选 Agent 执行者；`--clear` 解除分配。executor 是协作分配信息，不代替人类 owner 的写入授权。 |
| `waymark parent set <id> <parent-id>` | 设置父项；校验项目范围和层级环。 |
| `waymark parent clear <id>` | 移除父项关系。 |
| `waymark dep add <id> <blocking-id>` | 增加硬阻塞依赖；检测自依赖、重复边、跨项目错误和循环。 |
| `waymark dep remove <id> <blocking-id>` | 移除依赖。 |
| `waymark link add/remove` | 管理不影响 readiness 的软关联（后续）。 |

操作人检查以项目配置的单一 `operator` 与每张票的 `owner` 比较为准。父项负责人可以创建子项，但不自动获得修改、分配或推进不同 owner 子项的权限。每次写入应记录配置中的 operator 作为 actor。该配置值可由本地用户修改，所以此机制是流程约束，不是可信认证或安全边界。

### 5.4 列表、筛选、前沿与搜索

| 命令 | 能力 |
|---|---|
| `waymark ls` | 列出票据；按类别、状态、owner、executor、创建者、标签、父项和字段过滤、排序、分页。 |
| `waymark ready` | 列出未完成且所有硬依赖已完成、未被环或损坏数据排除的票据。 |
| `waymark blocked` | 列出尚未满足的依赖及阻塞原因。 |
| `waymark children <id>` | 列出直接子项；支持递归树形展示。 |
| `waymark tree [id]` | 展示父子层级树，并标记状态、owner、executor、阻塞、进度。 |
| `waymark search <query>` | 搜索标题、正文和配置允许搜索的字符串字段。 |
| `waymark query` | 支持 JSON 或可预测的过滤表达式，筛选扩展字段；过滤表达式须有版本化语法。 |
| `waymark frontier <map-id>` | Wayfinder 专用视图：开放、未阻塞、未领取的决策票；领取语义由 owner/executor 与状态配置定义。 |

### 5.5 历史与差异

首版不支持评论，也不支持评论回复或跨票评论关联，避免引入独立评论实体、评论授权和线程关系。需要记录的决策答案应写入票据正文或配置定义的 resolution 字段，并通过 Git 历史追踪。

| 命令 | 能力 |
|---|---|
| `waymark history <id>` | 展示 Git 历史或应用可识别的变更事件；明确数据来源及可用性。 |
| `waymark diff [id]` | 展示工作区或某票据与 Git 基线之间的差异（后续）。 |

### 5.6 统计、导入导出与缓存

| 命令 | 能力 |
|---|---|
| `waymark stats` | 按类别、状态、owner 展示数量、完成度、ready/blocked 及趋势。 |
| `waymark export json|jsonl` | 导出完整 schema、正文、ID 和关系，带格式版本。 |
| `waymark import json|jsonl <file>` | 导入/恢复；默认不覆盖 ID 冲突，可 `--dry-run` 预览。 |
| `waymark migrate` | 按显式迁移步骤升级 frontmatter/config；支持 `--from`、`--to`、`--dry-run`、备份；不允许静默降级。 |
| `waymark reindex` | 重建可选 SQLite 索引；索引可删可重建，不是权威数据。首版若规模允许可省略索引。 |

### 5.7 只读 Web 与 Agent 接口

| 命令 | 能力 |
|---|---|
| `waymark web` | 后续阶段启动只读进度页面；默认 loopback 绑定。首期不实现。 |
| `waymark mcp` | 第一阶段提供 Agent 读取、搜索及经统一校验写入的 MCP 工具；写入工具执行和 CLI 相同的授权、schema、关系和并发校验。 |
| `waymark agent-doc` | 第一阶段生成机器可读/面向 Agent 的命令、字段 schema、关系语义与安全边界说明。 |

Web 第一阶段不提供任何服务或写路由。未来 Web 必须是只读，MCP 写操作必须使用与 CLI 相同的 `operator == owner` 检查，不得自行绕过领域写入授权。

## 6. 建议交付阶段

### Phase 1：核心 CLI、MCP 和数据约束

实现 `init`、配置加载/校验、`new`、`show`、`edit/set`、`rename/move`、状态、owner/executor、parent、dep、`ls`、`ready`、`blocked`、`tree`、`validate`、`doctor`、乐观并发检测、`export/import json`、`migrate`、`mcp` 和 `agent-doc`。采用单 Markdown + YAML frontmatter 文件作为每票权威数据源，正文为 Markdown；首版不支持评论，不实现 Web、daemon、SQLite 索引、TUI 和外部同步。所有 CLI/MCP 写入都经过同一领域服务，执行 operator/owner、schema、关系、批量原子性和乐观并发校验。

### Phase 2：协作可观察性和只读 Web

统计、历史展示和只读 Web 树状进度页。用真实多 Agent 场景试跑，并验证不同票据并行编辑及同一票据乐观并发冲突恢复。评论不纳入当前计划，只有明确需求后再单独设计。

### Phase 3：可选扩展

索引/daemon、交互式 TUI、插件/外部 tracker 同步、复杂查询、通知和团队审计服务。按实际规模和使用痛点逐项引入，不视为必需能力。

## 7. 非目标

首期不做通用项目管理平台、多人 SaaS、实时协同编辑、任意脚本型 schema、任意工作流 DSL、双向外部 tracker 同步、复杂角色权限矩阵、评论/评论回复/跨票评论关联，也不把个人 glossary 的编辑权限混入票据系统。Glossary 可由独立文档管理，tracker 只需支持链接及只读展示。

## 8. 验收场景

1. `waymark init` 后得到有效配置；改类别目录和扩展字段后，工具能按新配置创建、读取和校验数据。
2. ID 在类别移动和 name 修改后保持不变；所有 parent/dependency 引用继续解析，规范引用不受 category/name 变化影响。
3. 同一类别中创建重名票据或改名造成冲突时被拒绝；不同类别可以使用相同 name；`<category>/<name>` 唯一解析，歧义时拒绝并提示使用 ID。
4. 创建 Wayfinder map、decision ticket、Spec 和实现票，建立 parent 链及独立 dependency 边；ready/blocked 结果正确。
5. 每张票都有 `created_by` 和 `owner`；`executor` 可选且可改派，改派不覆盖 owner。
6. 配置字段缺失、类型错误、未知类别、无效引用、关系环和不支持的 `frontmatter_version` 都给出可定位错误；自定义字段与 schema 冲突时允许只读查看并拒绝该票据的所有 Waymark 写操作，直到修复/迁移，直接编辑文件不受约束。
7. 对配置/数据迁移先运行 dry-run，可查看将修改的文件和字段；失败时不留下半迁移状态。
8. 第一阶段不启动 Web 服务；未来只读 Web 准确呈现层级与阻塞，POST/PATCH/DELETE 等写请求被服务端拒绝。
9. 并行修改不同票据可独立提交；若票据自读取后被其他写者修改，Waymark 检测版本/哈希变化、拒绝本次保存并要求重新读取后重试，不静默覆盖。
10. JSON/JSONL 导出再导入保留 ID、字段、正文、类别、关系和 frontmatter 版本。
11. 配置 operator 与票据 owner 相同时，Waymark 写命令允许修改；不同时拒绝所有票据写操作，但只读查询仍可用。
12. owner 改派只能由原 owner/operator 发起；成功后旧 owner 后续写入被拒绝，新 owner/operator 匹配后可继续操作。
13. 父票 owner 可以创建子票，但不能仅凭父子关系修改 owner 不同的子票或后代。
14. `executor` 可动态设置、改派和清空，但不改变 `owner`，也不授予人类操作授权。
15. 验证文档和错误提示明确说明配置 operator 检查可以被直接编辑文件或修改配置绕过，不是认证安全边界。

## 9. 已确定设计与实现基线

以下设计已确定，并作为实现基线：

- 工具暂定名为 Waymark，正式实现前检查命令、包名和商标冲突。
- 存储根目录和类别输出目录完全配置化，类别数量不固定；`.waymark/config.yml` 固定用于项目发现。
- 核心字段使用固定 YAML key，内部语义稳定；首版不支持核心字段重命名映射。自定义字段不得与核心字段重名或产生语义冲突，诊断命令必须报错。
- 核心状态允许项目自定义状态标识符和显示名称，但每个状态必须映射到固定语义组；类别状态统一使用可选的 `category_status` 字段，各类别配置自己的枚举值，不映射核心状态。
- `frontmatter_version` 与 `config_version` 分离；旧格式可读但写入阻止，schema 不兼容通过 `migrate --dry-run` 和显式迁移解决，不自动猜测语义。迁移必须先生成变更计划，确认后原子写入，并保留迁移前备份。
- ID 使用全局唯一、创建后不变的后缀，不编码类别和 name；推荐采用 ULID 风格 ID，文件名使用稳定的 `<id>.md`，不把可变 name 放进文件名。
- `operator` 直接声明在 `.waymark/config.yml`，不支持 `--actor` 或环境变量覆盖；它只是流程护栏，不是认证。
- 首版 `category_status` 可选，统一字段名，可读写，不参与核心流程逻辑；默认值和具体枚举由类别配置决定。
- 第一阶段专注 CLI、MCP、文件格式、校验、迁移、并发检测和核心关系能力，不做 Web。
- 首版不提供评论及评论回复。

## 10. 剩余决策的推荐方案

以下方案建议作为实现基线，除非后续原型验证发现实际问题再调整。

### 10.1 ID：固定 `wm_` 前缀加 Crockford 大写 ULID

采用 `wm_<ULID>`，例如：

```text
wm_01JQ7M9Y2K4V8T6R3P5N1C0HAB
```

规则：

- ULID 使用 Crockford Base32 大写，固定 26 个字符。
- `wm_` 是工具固定前缀，不由项目配置，不允许后续修改。
- 不把类别、name 或路径编码进 ID。
- 文件名固定为 `<id>.md`，例如 `wm_01JQ7M9Y2K4V8T6R3P5N1C0HAB.md`；文件名必须与 frontmatter 的 `id` 一致。
- ID 创建后不可变，导入导出、parent、依赖和 MCP 引用都使用完整 ID。
- 不使用自增数字、纯时间戳或可变项目/类别前缀。

这样可以避免 ID 前缀配置变更导致引用和脚本变化，同时保留时间排序和足够的唯一性。项目名称或类别名称只出现在配置、frontmatter 和展示层，不进入身份 ID。

### 10.2 核心状态：固定语义组，首版固定三组

首版固定三个核心语义组：

```text
open        # 尚未完成，可推进或等待
in_progress # 正在推进
closed      # 已完成，不再阻塞依赖
```

项目可以为核心状态配置显示名称和数量，例如 `draft` 映射到 `open`，`reviewing` 映射到 `in_progress`，`approved` 映射到 `closed`，但不能重新定义语义。

`blocked` 不作为可写状态。它是派生结果：票据本身不是 `closed`，且至少一个硬依赖未处于 `closed` 语义组时，才显示为 blocked。这样避免同时维护 `status` 和 `blocked` 两个可能互相矛盾的字段。

### 10.3 `category_status`：统一字段、类别可选、默认省略

推荐配置形状：

```yaml
categories:
  spec:
    path: specs
    category_status:
      values: [draft, review, approved]
      required: false
  implementation:
    path: implementation
    category_status:
      values: [coding, testing, verified]
      required: false
```

规则：

- frontmatter 字段名固定为 `category_status`。
- 类别可以不声明该字段，或者声明自己的枚举集合。
- `required` 默认 `false`，首版不强迫所有类别拥有额外状态。
- 没有值时省略该字段，不写入空字符串。
- `category_status` 可以通过 `set` 读写，但不影响核心状态、ready、blocked、依赖解除和统计。
- 字段名不能被类别重命名。
- 枚举值采用小写标识符，不能与核心状态标识符 `open`、`in_progress`、`closed`、`blocked` 冲突，比较时按小写规范化。

### 10.4 配置 schema：核心字段固定，扩展字段可配置

建议收回“核心 YAML key 可以任意映射”的自由度。CLI、MCP、导入导出和诊断都依赖核心字段，首版让核心字段使用固定名称，扩展字段才由用户定义：

```yaml
config_version: 1
frontmatter_version: 1
operator: alice

storage:
  root: work-items

categories:
  spec:
    display_name: Specifications
    path: specs
    category_status:
      values: [draft, review, approved]
      required: false
  implementation:
    display_name: Implementation
    path: implementation

statuses:
  draft:
    group: open
    display_name: Draft
  reviewing:
    group: in_progress
    display_name: Reviewing
  approved:
    group: closed
    display_name: Approved

fields:
  acceptance_criteria:
    type: list
    items: string
    required: false
  source_spec:
    type: reference
    required: false
```

建议固定且保留的核心 frontmatter 字段：

```yaml
id:
frontmatter_version:
category:
name:
title:
status:
created_by:
owner:
executor:
parent:
depends_on:
category_status:
created_at:
updated_at:
```

自定义字段必须满足：

- 使用小写 snake_case 标识符。
- 不得与核心字段同名。
- 不得与核心语义冲突，例如自定义 `blocked`、`owner_name`、`status_group` 或其他会伪装核心字段含义的名称；诊断规则维护一份保留名称表。
- 同名自定义字段在所有类别中必须使用兼容的 schema 定义；首版不提供类别专属字段命名空间。若不同类别需要同名但不同类型，必须使用不同字段名。
- `config validate`、`validate` 和 `doctor` 都必须在配置阶段或扫描票据时发现并报错。

这比允许核心字段任意映射更适合首版。用户仍能自由定义额外字段，但核心数据模型、JSON 输出、MCP 工具和迁移逻辑保持稳定。

### 10.5 CLI/MCP 输出：统一 envelope，错误码先冻结小集合

CLI 的 JSON 输出建议固定为：

成功：

```json
{
  "v": 1,
  "ok": true,
  "data": {},
  "meta": {
    "command": "show",
    "project": "."
  }
}
```

失败：

```json
{
  "v": 1,
  "ok": false,
  "error": {
    "code": "TICKET_NOT_FOUND",
    "message": "ticket was not found",
    "details": {
      "ref": "spec/missing"
    }
  },
  "meta": {
    "command": "show"
  }
}
```

规则：

- 成功响应只包含 `data`，失败响应只包含 `error`，避免同时出现空字段。
- `message` 面向人类，`code` 稳定供脚本判断，`details` 可扩展。
- JSON 模式下 stdout 只输出一份 JSON；日志和进度写 stderr。
- 建议首版错误码至少包括：`INVALID_ARGUMENT`、`CONFIG_INVALID`、`SCHEMA_CONFLICT`、`TICKET_NOT_FOUND`、`NAME_CONFLICT`、`REFERENCE_INVALID`、`RELATION_CYCLE`、`OWNER_MISMATCH`、`WRITE_CONFLICT`、`MIGRATION_REQUIRED`、`IO_ERROR`。
- 退出码与错误码分离：退出码表示错误大类，JSON `error.code` 表示具体原因。
- MCP 使用同一错误码和 details，不要求伪装成 CLI 文本输出。

建议错误退出码：`0` 成功，`2` 参数/配置错误，`3` 数据校验失败，`4` owner 流程拒绝，`5` 并发冲突，`6` IO 或外部系统错误。

首版错误码建议冻结为以下最小集合：

| 错误码 | 含义 | 退出码 |
|---|---|---:|
| `INVALID_ARGUMENT` | 命令参数、引用格式或字段赋值无效 | 2 |
| `CONFIG_INVALID` | 配置文件无法解析或违反配置 schema | 2 |
| `SCHEMA_CONFLICT` | 票据字段与当前 schema 或核心字段规则冲突 | 3 |
| `TICKET_NOT_FOUND` | ID 或 `<category>/<name>` 找不到票据 | 3 |
| `NAME_CONFLICT` | 同一 category 内 name 已存在 | 3 |
| `REFERENCE_INVALID` | parent、depends_on 或 reference 字段指向不存在/不合法目标 | 3 |
| `RELATION_CYCLE` | parent 或 depends_on 形成环 | 3 |
| `OWNER_MISMATCH` | operator 与目标票据 owner 不一致 | 4 |
| `WRITE_CONFLICT` | 乐观并发检测发现文件已变化 | 5 |
| `MIGRATION_REQUIRED` | 票据或配置需要显式迁移后才能写入 | 3 |
| `IO_ERROR` | 文件系统或其他外部 IO 失败 | 6 |

`details` 必须尽量提供机器可定位信息，例如 `path`、`id`、`field`、`expected`、`actual`、`conflicts` 和 `retryable`。错误码是稳定 API，不应把人类 message 作为脚本判断依据。
