# Axibiz Desktop UI 设计语言与实施方案

日期：2026-10-31。基线：0.19.0 / 数据结构 19。

状态：**设计参考，部分目标已在 0.20.0 落地**。逐模块实现、共用组件、验证证据和待完成目标见 [UI interaction audit](UI_INTERACTION_AUDIT.md)。本文件中的全部尺寸、菜单、快捷键与布局不能一概视为已上线功能；以审查表和程序实际行为为准。

## 1. 设计定位

Axibiz 面向需要长时间查账、录单和核对金额的桌面用户。设计语言定为：**紧凑、清晰、可核对的桌面业务界面**。英文作为界面语言，客户名称、地址、备注等用户数据保留原文。

核心规则：

1. 用户进入任一模块，都能辨认当前公司、业务对象、记录状态和下一步动作。
2. 同类页面复用同一布局；颜色、尺寸、按钮顺序由共用规范决定。
3. 稳定地保留数据区域，把空间优先给业务行；不能靠缩小字体解决布局问题。
4. 读取、编辑、保存、过账、收付款、冲销各有明确含义。
5. 页面变窄时先收起辅助信息，关键金额与提交动作始终可达。
6. 每个独立业务窗口能完成自己的任务，并维持原有单实例与未保存保护。

## 2. 对用户截图的归纳

本轮 12 张图片并非 12 个独立案例；图 6、7 内容重复。下表按布局与交互归类，并结合之前的 Sage 50、Invoice、Receive Payments 截图。

| 截图 | 观察 | Axibiz 采用方式 |
| --- | --- | --- |
| 图 1：Enter Bills | 上方业务命令；紧凑单头；明细占主要区域；右侧往来背景；底部完成录入 | 建立统一单据模板，背景栏可收起，金额与提交区固定 |
| 图 2、6、7、8：Vendor Center | 全局导航、往来名单、当前对象、交易历史构成稳定层次 | Customer / Vendor 同一主从模板，顶部信息高度受限 |
| 图 3：Chart of Accounts | 整屏表格适合连续查找；表头、金额列与选中行容易辨认 | 科目和分录列表使用列表模板，保留列对齐与键盘操作 |
| 图 4、5、8、11：菜单 | 全局入口、业务域入口和当前对象命令有不同作用域 | 菜单与工具栏引用同一命令；不再分别维护判断逻辑 |
| 图 9：导航折叠 | 用户主动换取横向工作空间，主体结构保持 | 全局折叠偏好保留；不能切模块就自动改变导航密度 |
| 图 10：Sales Tax | 机构余额、税码设置、调整动作分区明确 | 税务总览与设置分层，已支持的动作按机构/期间进入 |
| 图 12：Journal Entry | 日期/币种/汇率靠近；借贷表格主导；底部完成操作 | 以借贷核对为中心，明确 Save Draft 与 Post 的区别 |
| 之前的 Invoice / Receive Payments | 单据明细与总额分区；付款分配与付款金额互相对应 | 共用行表格、金额摘要与提交前复核模板 |
| 之前的 Sage 50 报表/多窗口 | 工作空间与独立业务/报告窗口并存 | 保留可跨屏的原生窗口与追溯入口，不改回父窗口内受限的 MDI |

截图也暴露了不适合直接继承的部分：巨大的空白资料区、深层菜单、过小标签、亮绿色整行、纹理底图、过长图标工具带，以及少量记录时铺满窗口的空白条纹。Axibiz 保留其工作结构，重新制定视觉尺度。

截图中的历史税码、机构名称、借贷符号和税率仅是布局背景，不作为 Axibiz 的会计配置或默认税率来源。

## 3. 补充案例与取舍

| 案例及官方依据 | 采用的原则 | 不直接搬入的部分 |
| --- | --- | --- |
| [QuickBooks Desktop：快捷键](https://quickbooks.intuit.com/learn-support/en-us/help-article/multi-user-mode/keyboard-shortcuts-quickbooks-desktop/L7xynnaf3_US_en_US) | 高频业务可用键盘；命令与窗口跳转有固定位置 | 普通 Enter 不直接触发财务记账；不复制未实现的业务入口 |
| [Sage 50 加拿大版：Home Window](https://help-sage50.na.sage.com/en-ca/core/2027/Content/CommonTasks/HomeWindow.htm) | 模块内集中任务、关键记录和报告；支持最近报告与上下文命令 | 不复制推广入口，不把美版/其他 Sage 产品的能力当作加拿大版现状 |
| [Business Central：页面类型](https://learn.microsoft.com/en-us/dynamics365/business-central/dev-itpro/developer/devenv-page-types-and-layouts) | 明确区分 List、Card、Document、Worksheet；单据由单头接明细 | 不要求采用其技术栈或将资料自动保存机制搬到财务录入 |
| [SAP Fiori：布局与业务对象页面](https://www.sap.com/design-system/fiori-design-web/v1-151/discover/sap-products/sap-s4hana-only/best-practices-for-designing-sap-fiori-apps) | 列表到对象的连续上下文；按可用空间调整列数 | 辅助余额/备注不单独冒充一个业务对象列；不在小窗口强行三列 |
| [Windows：命令栏](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/command-bar) | 常用命令可见；宽度不足时低频命令进入 More；图标带文字 | 不用过高 Ribbon 挤压录入区；不在每个页面改变命令位置 |
| [Windows：可访问性](https://learn.microsoft.com/en-us/windows/apps/develop/accessibility)与[窗口/DPI 指南](https://learn.microsoft.com/en-us/windows/apps/get-started/best-practices) | 可读对比度、可见焦点、键盘可达、多尺寸及不同 DPI 验证 | 保持 Rust/egui 实现；设计借鉴不代表改用 WinUI 已自动获得其能力 |

以上是案例提炼。下文的配色、尺寸、菜单分组和操作规则是 Axibiz 的设计建议。

## 4. 应用外壳与信息架构

主窗口建议保留三层，避免再叠加产品名、页面大标题、说明段、卡片标题等重复内容：

1. 原生标题栏：`Axibiz Desktop Beta — Company`。
2. 应用菜单与可折叠导航：管理公司、模块和窗口。
3. 当前工作区：一个短标题/命令栏，直接进入数据。

导航建议按实际工作分组：

| 分组 | 常驻导航 |
| --- | --- |
| Work | Tasks、Invoices、Customers、Vendors、Products & Services |
| Accounting | Bills、Journal Entries、General Ledger、Chart of Accounts、Sales Tax |
| 底部工具 | Setup、Reports、Window |

顶部菜单目标：`File · Lists · Transactions · Reports · Setup · Window · Help`。Lists、Transactions 是便利入口，和导航/页面按钮调用同一命令；不另建第二套功能。

- File：已有公司文件操作、备份、验证、恢复、退出。
- Lists：客户、供应商、商品服务、会计科目。
- Transactions：Task、Invoice、Bill、Receipt、Pay Bills、Journal 的已有入口。
- Reports：已有总账/试算平衡、截止日对账、税务、Accounting Checks、Audit Log。
- Setup：公司资料、汇率、税务设置、编号、GL 配置。银行币种配置可在后续共享设置页集中暴露，仍复用现有银行元数据。
- Window：当前已打开的业务窗口、激活窗口、移回当前屏幕。

当前 Receive Payment 需先确定发票；全局入口应打开现有发票选择/列表流程，再进入单票收款，不能展示一个实际上尚不支持的多票分配界面。

导航展开宽度建议 208，折叠宽度 64；用户选择跨模块保持。页面的主列表或对象栏不再重复一套应用导航。

## 5. 视觉基础与尺度

尺寸为 egui 逻辑点，随每屏 DPI 缩放，不等于截图中的物理像素。以下是目标基线，需要通过渲染与实际设备验收。

| 项目 | 规范建议 |
| --- | --- |
| 字体 | Segoe UI / 系统无衬线；中文数据使用现有字体回退；不为同类控件单独指定字体 |
| 正文/输入/按钮/金额 | 13；舒适模式可统一调整至 14 |
| 页/单据标题 | 20；分区标题 14；辅助说明最低 11 |
| 金额摘要 | 与明细同字号；Total / Balance Due 用字重和分隔线强调 |
| 间距 | 4 的基准；控件间 8，分区间 16，页面边距 16 |
| 紧凑模式 | 控件高 28，表格行高 28，标题命令区约 40 |
| 舒适模式 | 控件高 32，表格行高 32；同一会话各模块统一 |
| 图标 | 统一线性图标 16/20；命中区域至少 28×28；主要动作同时显示短文字 |
| 圆角/边框 | 圆角 4；边框 1；只用少量分隔线，不层层嵌套卡片 |

| 色彩语义 | 建议色值 | 用途 |
| --- | --- | --- |
| 正文 | `#20303A` | 标题、数据 |
| 辅助文本 | `#5C6B76` | 标签、说明，不能浅到难读 |
| 工作背景 / 内容面 | `#F5F7F9` / `#FFFFFF` | 页面层级 |
| 导航 | `#1B2A34` | 继承现有深色侧栏 |
| 主要业务动作 | `#127E55` | 每个操作区最多一个主要按钮 |
| 选中 / 交替行 | `#E4F1EB` / `#F0F5FA` | 选中行再加标记；条纹轻于选中态 |
| 输入边界 / 分区线 | `#7A8893` / `#D6DEE4` | 区分交互边界与装饰分隔 |
| 焦点 | `#2563EB` | 清晰焦点描边，与选中状态区分 |
| 警示 / 错误 | 琥珀 / 暗红，并带文字 | 不单凭颜色表示待核对或错误 |

计算示例：白字在主绿上对比度约 5.07:1；辅助文本在白底约 5.50:1，在工作背景约 5.12:1。正常文字目标不低于 4.5:1，关键控件边界/焦点目标不低于 3:1；完整状态仍须实际验证。

金额使用可对齐的数字字形、右对齐、两位小数和千位分隔；币种单独对齐。汇率按业务允许精度显示。日期统一 `YYYY-MM-DD`。`0.00`、空值、`Unavailable`、`Needs review` 必须有不同含义。

## 6. 七类页面模板

结构示意见 页面模板图 (internal reference artwork; not distributed)。示意图展示区域职责，不是已上线页面。

### A. Center：Customer / Vendor

顶部对象命令；左侧名称/币种余额、搜索和 Active 状态；右侧紧凑对象摘要；下方交易/联系/备注等已有标签。

- 名称左对齐，余额右对齐。供应商 Recorded gross 与 Open A/P 不能混用标签。
- 摘要默认只显示名称、Active、币种、余额和必要联系方式；更多资料在 Contacts。
- 摘要建议 112–160 高，可按内容收缩，不让空字段占大面积。
- 主从分隔条可调；对象列表建议 224–280 宽，长名字悬停显示完整内容。
- 不把普通 Notes 做成常驻大块空白；无备注时提供一个简短入口。

### B. Register：Tasks / Invoices / Bills / Chart of Accounts / Journal 列表

标题/新建命令 → 搜索与过滤 → 表头与数据 → 行数/金额口径。

- 所有列表支持明确的选中态、排序/筛选反馈；分页/虚拟化按现有能力渐进统一。
- 单击选择；单据编号点击或键盘激活打开；右键是补充入口，不是唯一入口。
- 操作列保留少量动作，选中记录的更多操作进入上下文命令。
- Chart of Accounts 建议列：Code、Name、Type、Balance (GL base)、Status。只有已取得可靠余额时才显示；不能从科目名称中的 CAD/USD 推导真实账户币种。
- 按已有 Type 分组可做界面折叠；父子科目树需要数据模型支持，不用缩进伪造层级。

### C. Document：Task / Invoice / Bill / Journal

原生标题 → 当前单据命令 → 紧凑单头 → 主要明细表 → 金额摘要 → 固定完成动作。

- 单头使用有边界的字段宽度；超宽窗口优先扩展 Description/Memo 和表格，不拉长日期/币种。
- Bill/Invoice 可有右侧 Context：往来余额、近期真实交易、备注。建议宽 248–280，可收起；只读取现有记录，不出现未支持的邮件历史或附件模块。
- Invoice：Customer、Date、Currency、Reference；明细保持 Item / Qty / Description / Unit / Rate / Amount / Tax。
- Bill：Vendor、Date、Currency、Reference、Terms、Due；明细保留 Account、Amount、Memo、Tax、Recovery、Task。低频字段可以由 Columns 显示，相关金额口径仍须可见。
- Journal：Date、Currency、Rate、Memo；Account / Debit / Credit / Description / Party / Tax reference；底部固定 Debit、Credit、Difference。
- 表格尾部可以有一个可编辑的新行，不能把装饰行当作有效数据。输入区有限的空行网格可选；只读列表不铺满空条纹。

### D. Settlement：Receive Payment / Pay Bills

往来对象 → 金额/日期/方式/银行背景 → 单据与分配表 → Applied / Remaining → 复核。

- Receive Payment 保留当前单发票范围；Pay Bills 保留单供应商/币种、多 Bill 分配。
- 显示付款前余额、本次金额、付款后余额；不得将 Recorded gross 当作待付款。
- Review 中固定对齐名称、日期、金额、银行/币种和分配摘要；内容可滚动，确认按钮不能滚出窗口。
- 复核初始焦点落在 Keep Editing；只有明确点击 Confirm & Record 才提交。
- 后台余额、源分录、汇率或银行变化使复核失效，保留原输入并要求重新复核。

### E. Report：GL / Trial Balance / Reconciliation / Tax 报表 / Audit

报告标题与口径 → 日期/期间/币种过滤 → 结果 → 可追溯来源与输出命令。

- `As of` 与 `From / To` 不混用；每列说明原币、GL 本币或 CAD 税务口径。
- 来源单据/分录可追溯，返回报告保留过滤条件和位置。
- 表格只读，金额按报告借贷约定显示；不从 QuickBooks 截图抄入负号规则。
- Export CSV 复用当前过滤；PDF/Print 只在已有可靠输出能力的页面提供，不用占位按钮冒充全局能力。

### F. Settings：Customer / Vendor 编辑及公司、税码、编号、GL 设置

左侧分类 → 右侧有上限宽度的表单 → 固定 Save 区。

- 主数据和配置使用 Save；财务记录使用下面的专门动作语言。
- 必填字段显示 `*`；帮助靠近相关字段，长说明按需展开。
- 固定标签列 104–120，常用输入宽 240–320；宽窗口可两列，窄窗口单列。
- 支付卡号、CVV、批量导入等未支持能力不出现为可操作控件。

### G. Tax Overview：Sales Tax

顶部：机构/税务账户、余额 CAD、截止日和已有报告入口。

下方：按当前机构/期间列示税款及调整记录。Tax Setup、Assign Tax Codes 进入现有设置；退款/调整必须使用已有受校验业务入口。

不要在总览同时展开全部税码编辑器。UI 复用已有 summary/detail/review 报表，不承诺网上申报、异常报告新算法、Quick Method 或自动 PST 佣金。

## 7. 命令语言、状态与完成操作

| 场景 | 当前能力 | 目标标签/流程 | 需要注意 |
| --- | --- | --- | --- |
| Customer / Vendor / Account | 保存主数据 | Save、Save & Close | 不表示记账 |
| Task | 保存业务任务、变更状态 | Save Task；单独 Complete | Complete 不等于 Issue Invoice |
| 新 Invoice | 预览后确认生成，自动产生来源分录 | Preview Invoice → Confirm & Issue | 未持久化的输入不能标成 Saved Draft |
| 已开 Invoice | 财务内容冻结、收款、PDF、打印 | Receive Payment、Save PDF、Print、View Journal | 不提供误导性的 Save/Delete |
| Bill | 保存费用单并自动记入 A/P | 目标 Review Bill → Record Bill；可提供续录选项 | 现有 Save 已有财务后果；统一标签/复核接入需实施验证 |
| 手工 Journal | 草稿保存、独立过账 | Save Draft；Review / Post | 关闭时保存草稿不能触发 Post |
| Receipt / Supplier Payment | 已有复核与原子提交 | Review Receipt / Review Payment → Confirm & Record | 不发起真实银行转账 |
| 已记账付款/手工分录 | 有日期与原因的撤销/冲销 | Void Receipt / Void Payment / Reverse Entry | 原记录保留；与 Delete 分开 |
| 税务现金/调整 | 记录、撤销、来源分录 | Record Tax Payment / Record Adjustment | 不等于税局付款或申报 |

操作区规定：顶部用于查找、创建、输出、追溯等工具；底部用于完成当前输入。主要财务提交动作只设一个权威入口，避免上下各有含义不同的绿色 Save。

Save & New 表示同一次验证/事务成功后再开始下一笔；失败时留在当前记录。新建前已有草稿时先激活原窗口，不直接覆盖。不能把清空输入称为 Delete；建议搜索框内 `×` 只清筛选，垃圾桶用于删除未保存行，已过账业务使用带文字的 Void/Reverse。

关闭行为：

- 原生 X、Alt+F4、Escape 使用同一关闭路由；有下拉/弹出层时 Escape 先关闭最内层。
- 干净的只读/已保存窗口直接关闭；未保存内容显示横向动作：Return to editing、可用的 Save Draft/Save & Close、Discard and close。
- 收付款关闭时的 Record 动作仍进入复核，不绕过 Confirm & Record。
- 首次焦点是 Return / Keep Editing；长提示可滚动；不提供 Enter 即丢弃的默认行为。
- 各窗口标明 Unsaved changes / Saved / Issued / Posted / Recorded；主状态栏不能在存在未保存输入时笼统宣称所有数据已保存。

## 8. 表格、错误、键盘规范

### 表格与字段

- 表头固定，滚动只发生在数据区；合计/提交区固定。
- 编号、日期、币种、金额有稳定最小宽度；Description/Memo 弹性扩展。
- 窄窗口允许表格内部横向滚动；固定识别列和金额摘要，不能把税码、金额或行操作裁到页面外。
- 只有完成候选选择才绑定客户/供应商/科目 ID；未选定文本有可见提示，不自动取第一项。
- 无数据、过滤无结果、加载中、失败、待核对分别显示：No records yet、No matching records、Loading、Unavailable、Needs review。
- 错误靠近字段；财务阻断另有简短摘要和可执行的修复入口。详情可展开，输入不清空。
- 一般提示不堆栈弹窗；模态窗口只用于破坏性决策、选择必要前置项和财务确认。

### 键盘目标

快捷键是后续统一目标，先审计现有绑定再实施，不能仅在菜单写上快捷键文字。

| 按键 | 目标行为 |
| --- | --- |
| Tab / Shift+Tab | 按阅读顺序移动；表格按可编辑列前进 |
| 方向键 / Enter | 在候选与列表中选择/激活；普通 Enter 不直接记账 |
| Ctrl+F | 聚焦当前列表/当前范围搜索 |
| Ctrl+N | 当前模块新建；已有编辑窗口时激活它 |
| Ctrl+S | 主数据保存；Journal 保存草稿；需要财务确认时进入既有预览/复核，不直接记账 |
| Ctrl+P | 当前页面已支持的打印；未支持时不绑定 |
| Ctrl+W / Alt+F4 | 请求关闭活动业务窗口并执行未保存保护 |
| Escape | 先关闭下拉，再返回复核，再请求关闭业务窗口 |

图标有 accessible name 和 tooltip；焦点可见；错误/状态不只依赖红绿。自定义表格与图标必须验证屏幕阅读器可访问性，不能因视觉接近 Windows 就假定已支持。

## 9. 多窗口、多显示器与响应式

### 保留现有约束

当前是主工作区加 11 类原生业务/报告窗口，编辑同类业务只激活一个窗口。首轮设计不放宽单实例规则；单实例不是数据库锁、复核和事务校验的替代品。

当前 Bill 与 Account 编辑共用 Accounting 槽；Invoice 创建/查看也共用其业务槽。设计不能假装这些已完全解耦。若后续要从录单过程中另开科目编辑器或固定多份只读报告，必须单独调整注册键和回归测试，不能在布局改造中顺带改掉。

### 窗口规则

- 业务窗口标题包含业务类型、记录号/往来和公司；不复制应用侧栏到每一个录入窗口。
- Windows 的拖动、最小化、最大化和贴靠负责窗口管理。软件只提供打开窗口列表、激活、移回当前屏幕。
- 恢复位置是创建时提示；不能每帧重新下发位置/尺寸。拔掉显示器后检测不可见窗口，恢复到当前工作区。
- 页面断点以窗口客户区的逻辑宽度为准，不能用显示器分辨率或截图像素决定。

| 客户区可用宽度 | 布局目标 |
| --- | --- |
| ≥1280 | 主从列表可见；单据可显示可选右侧 Context；明细优先占空间 |
| 960–1279 | 保留适合的主从结构；Context 默认折叠；低频命令进 More |
| 720–959 | Center 使用选择器/返回列表；单头重排；表格内横向滚动 |
| <720 | 单列表单与局部滚动；保留金额与完成动作；不得再靠缩字适配 |

验证目标包含 900×650 和 640×480 客户区。较小尺寸提供可用的降级布局，不承诺同屏显示所有列。一般高度分配先扣除标题/命令、摘要、完成区，剩余全部给表格；短窗口先折叠地址等低频内容。

多屏示例：主屏保留 Vendor Center，副屏使用 Pay Bills 与现有 GL/对账窗口进行核对。若所选编辑业务已打开，所有入口都激活已有窗口。更进一步的多份只读报告比较属于后续窗口能力扩展。

## 10. 对当前代码结构的落实

保持 Rust/egui 与现有会计命令、数据结构和校验，不因换皮改写财务模型。

| 现有模块 | 计划职责 |
| --- | --- |
| `theme.rs` | 颜色、字号、间距、密度、图标与焦点 token 的唯一来源 |
| `windows.rs` | 单实例注册、原生标题、关闭路由、创建时位置恢复 |
| `party_ui.rs` / `people_ui.rs` | Center、Profile 编辑、共用复核模板 |
| `party_data.rs` / `party_transactions.rs` | 查询/缓存/过滤/导出的既有契约；渲染不反复重算余额 |
| `invoice_sheet.rs` | 录入网格与只读 register 的共同表格基础，避免逐页画另一套网格 |
| 拟新增 `commands` / `document_layout` / `report_layout` | 共用命令描述与布局；只有在多个页面出现真实重复时再抽取 |
| `receipts` / `ap` / `gl` / `tax` / `storage` | 继续负责业务与事务校验；按钮禁用不是财务权限或安全边界 |

命令描述建议包含作用域、英文标签、图标、可用条件、说明、快捷键、是否需要复核；多个入口使用同一处理函数。不能为统一 UI 建立一个绕开领域校验的万能 save 函数。

## 11. 分阶段实施与验收

| 阶段 | 交付 | 进入下一阶段的条件 |
| --- | --- | --- |
| 1：规范与样板 | 冻结 token、命令词汇、Center + Bill/Invoice + Journal 三组实际桌面样板 | 空/少/多数据、长名称、只读/未保存/错误、900×650 对照评审；修正后再铺开 |
| 2：共用组件 | 统一命令栏、完成区、金额摘要、网格、焦点与错误样式；整理菜单入口 | Customer/Vendor 不分叉；同一入口行为相同；旧单实例和保存失败保留回归通过 |
| 3：模块迁移 | 先 Task/Invoice/Bill/Journal，再付款和资料编辑，再 Accounts/GL/Tax/Setup | 每种业务正确使用 Save/Issue/Record/Post/Reverse；过滤与输出一致；没有假功能 |
| 4：多屏与发布 | 原生窗口、混合 DPI、贴靠、断屏、键盘与辅助功能验收；更新使用说明 | 自动检查、物理设备验收、隔离升级比较和版本构建全部通过 |

明确验收项：

1. 任意页面切换，导航密度保持；同类标题、行高、按钮高与金额字号一致。
2. 0、1、100、1000 行场景可读；只有真实数据有行数，不因少量记录出现无限空网格。
3. 长英文、中文名称、负数、大金额和多币种不会让金额列/标签错位；必要信息不只藏在 tooltip。
4. 搜索/切标签/切对象无财务写入；回到原列表保留上下文；无结果与查询失败明确区分。
5. 所有入口复用同一单实例及关闭逻辑；同一个 JE 编辑器不会重复打开或覆盖原输入。
6. 125%/150%/200% 与 100% 显示器之间拖动，X 可正常点击，无窗口抖动、控件裁切或异常重排。
7. 关闭/重开、最小化/恢复、负坐标显示器、断开显示器后窗口仍可找回。
8. 外币报价缺失、银行不匹配、锁账、余额变化、重复点击提交均保留既有阻断与事务安全。
9. 财务确认内容可读、初始焦点安全，长错误不遮住 Return/Confirm；普通 Enter 不产生财务写入。
10. `fmt`、Clippy、相关交互与财务回归、隔离升级检查通过；不以网页原型替代实际 egui 和实体屏幕验收。

本轮建议先完成第 1 阶段样板评审，再推广到全软件。0.19.0 已有的共用工作区、复核、事务和窗口安全作为实施基线保留。
