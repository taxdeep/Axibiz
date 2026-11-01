# Changelog

## 0.25.0 - 2026-11-01

- Add Setup > Users with company-local accounts, optional password sign-in, administrator/user management and protected current-password confirmation. Existing companies remain passwordless by default; active users all need passwords before sign-in is enabled. Preserve at least one active administrator.
- Store independent salted Argon2id verifiers and login policy in company schema 21; never copy password verifiers into financial/audit JSON. Add company-bound revision-checked sessions, persisted retry throttling, guarded CLI entry and File > Lock / Switch User.
- Authenticate protected companies before normal startup backup/migration/desktop construction; open protected legacy files in place before relocation. Keep company content hidden while signed out. User drafts join existing exit/restore protection.
- Restore accounts/policy with the company backup and invalidate the previous session. Protected-company restore requires an administrator. Preserve ordinary financial workflows and add signed-in identity to row-change audit records. Raw files remain unencrypted and retain existing OS protection.

## 0.24.0 - 2026-11-01

- Separate machine-local software Settings from Setup > Company Settings. Preserve both drafts across navigation and protect unsaved changes on exit/restore.
- Add configurable backup root, daily startup backup toggle and retention (1–365), retaining required pre-upgrade protection. Manual backup dialogs and restore safety backups use the configured root; existing backup files stay in place.
- Save software preferences in the computer user's separate JSON file with validation, directory write checks and atomic replacement. Company restore does not replace these preferences. No database schema change (20).
- Add 12/24-hour clock and Vancouver/system/UTC display preferences, applied to Audit Log timestamps with UTC offsets and DST-aware conversion. Keep original event timestamps and business document dates unchanged.
- Rotate only recognized generated backup files, in timestamp order, so cross-version filename sorting cannot discard the newest upgrade backup. Skip directories and links; retain manual and restore backups.

## 0.23.0 - 2026-11-01

- Rebuild Company Settings around grouped contact, legal, identification, reporting and workspace fields using shared form/section/footer controls. Reserve save actions outside the scrollable body; confirm discards and retain drafts through navigation and failed saves.
- Add optional structured company metadata, identifier/contact validation and atomic audited saves. Schema 20 preserves legacy freeform settings and leaves unknown metadata unspecified. Existing CLI settings edits retain the profile.
- Freeze structured company details in new invoice snapshots and augment their printable contact/registration references. Preserve issued invoices unchanged. Fiscal/tax year and form choices are reference preferences; existing report dates, period locks and tax setup remain authoritative.

## 0.22.0 - 2026-10-31

- Replace separate expanded/compact route lists with one icon navigation registry. Both modes expose Tasks, Invoices, Customers, Vendors, Products, Bills, Journal, Ledger, Accounts and Tax in the same order, with Settings fixed at the bottom.
- Add distinct vector icons, tooltip/accessible names, keyboard focus, a subtle selected background and green indicator. Compact width is 56 pixels; icon and toggle anchors remain stable.
- Persist expanded/collapsed preference in a company-side `.ui.json` file; corrupt/missing preferences fall back to expanded mode. Navigation retains queries, current drafts and existing business windows; financial schema remains 19.

## 0.21.3 - 2026-10-31

- Unify expanded and compact navigation at 13-point regular text with a shared 36-pixel row component. Left-align expanded labels and widen the compact rail from 72 to 88 pixels to avoid squeezing labels.
- Reduce the brand heading to 16 points and use 11-point captions. Preserve menu toggle anchoring and module navigation; form/table typography and schema 19 are unchanged.

## 0.21.2 - 2026-10-31

- Align Transactions menu text with fixed icon slots and vector Add/Payment symbols. Remove mixed full-width/ASCII plus prefixes from menu labels and group creation/payment commands.
- Use native icon/text atoms for shared labelled commands, replacing space-based alignment while retaining action labels and behavior. Schema remains 19.

## 0.21.1 - 2026-10-31

- Replace Collapse Menu / Menu text buttons with a shared borderless hamburger toggle. Keep a stable position, circular hover/press feedback, keyboard focus indication and accessible Show/Hide main menu labels.
- Retain existing navigation state, widths and module behavior. Schema remains 19.

## 0.21.0 - 2026-10-31

- Unify row and document commands with vector icons, readable labels, tooltips and semantic names; row delete/edit/copy targets fit the 28-pixel grid.
- Replace three Bill review buttons with one Review Bill action and an explicit after-recording outcome. Preserve the original reviewed save and transaction paths.
- Tighten single-invoice receipt tables, align payment summaries, protect payment reset and supplier-allocation clearing, and explain disabled workflow actions.
- Separate party view tabs from commands, show active filter counts, align numeric headings and add an optional advanced Bill column view that retains financial input.
- Document the future Invoice template library/editor/versioning/archive design. Deposit and PO are not implemented. No database migration.

## 0.20.0 ? 2026-10-31

- Audit all 16 modules and 11 business-window categories against the UI design language; document source coverage, tests and deferred targets.
- Share page metadata, navigation, registers, compact search, primary actions, financial review and discard guards. Retain separate domain validation and transactions.
- Require Bill review, journal posting/reversal review and tax cash review; recheck relevant source inputs and locked periods before writes. Task cancellation no longer saves first; task-created expenses require review.
- Protect edited numbering, tax and GL settings on exit/restore. Preserve numbering drafts and module searches during navigation. Confirm bulk legacy currency labelling.
- Remove obsolete Customer/Vendor paths from the generic create dialog and unify invoice cancellation with the business-window close route. Correct manual FX guidance and translate modified action labels to English.
- Keep database schema 19; existing data, document snapshots and multi-window singleton rules remain authoritative.

## 0.19.0 - 2026-10-30

- Customer and Vendor share navigation, workspace layout, profile field controls, transaction filters/registers and CSV export. Root and native forms share typography. Empty registers show relevant actions without artificial rows.
- Added reviewed, idempotent customer receipts with atomic payment/number/journal/audit commits and immutable request history. Shared settlement checks protect dates, locked periods, original sources, quote/remaining/account changes and bank currencies.
- Clean supported editors close directly; changed profiles/accounts retain save/discard/return. Closing a manual journal can save its draft without posting. Payment close requires review before recording.
- Extracted typed batch party read models, invalidated caches, shared FX provenance and CSV encoding. Removed redundant queries, center orchestration and unreachable wrappers; spreadsheet formula prefixes in untrusted report text are neutralized.
- Schema 19 adds receipt requests without inventing historical receipts. Final release gates, real PDF/historical FX checks and isolated schema-16/18 upgrades passed. See AUDIT.md and docs/ARCHITECTURE_REVIEW.md for evidence and remaining boundaries.

## 0.13.0 — 2026-10-28

- 新增 Report → Reconciliation，按截止日查看分币种应收、来源账面值与总账控制科目差异、CAD 税务净控制余额，支持 CSV 和 CLI `reconcile YYYY-MM-DD --output report.csv`。
- 数据不完整、期初来源缺失、控制科目映射重叠或 GL 本币非 CAD 的税务比较不显示“相符”；不自动调整账目，不用当前汇率重估外币应收。
- 收款撤销保存业务日期，历史截止日不会被之后的撤销抹去；结构升至 15，旧撤销日期仅根据已有 GL 冲销恢复，未知日期保留并提醒。
- 收款日期和 GL 冲销关联纳入完整性验证；日期、外币、手工差异、未知历史、映射变更、CSV 和窗口尺寸回归通过。

## 0.12.0 — 2026-10-28

- 恢复后统一清空业务编辑草稿和后台接收状态，修复旧价格弹窗可能修改恢复后同 ID 数据的风险，删除遗留价格编辑字段。
- 修复税款撤销导致历史报表余额被抹去的问题，记录撤销业务日期并分期列示原交易与反向变动；旧数据只根据已有 GL 冲销凭据恢复日期，未知项明确提醒。
- 新发票冻结付款条款和说明，重印不套用后来修改的约定；新增 Report → Accounting Checks、CLI accounting-check 及预览资料提醒，不声称替代人工税务判断。
- Customer、Vendor、Products & Services 采用行尾 Edit 弹窗，保存失败保留输入；公共编辑按钮统一。
- 发票概览改为汇总查询和每页 100 条，保留 Unicode 搜索；详情及记账完整验证。Task 界面按创建、列表、编辑拆分；金额解析和格式化独立成模块。
- 发票大尺寸详情、保存 PDF、交互打印和英文模板正式发布；移除界面 HTML 保存入口，保留发票日期、不显示开票时间。
- 默认数据库与备份接入 Windows 文件权限隔离，同一 Windows 账户仍可改库，不是数据库加密或独立服务隔离。
- 数据结构升至 14，升级前自动备份；首次从原 0.11.0（结构 12）升级包含模板和税款撤销日期变更。旧程序不得打开升级后数据库。
- 发布门禁、真实浏览器 PDF、隔离公司库升级和原表数据比较见 AUDIT.md；完整分阶段计划见 docs/REMEDIATION_PLAN.md。

## 0.11.0 — 2026-10-27

- Invoices 页面新增 New Invoice，直接选择客户、日期及币种，支持价格单和自定义业务、多行数量/单价/税码，无需 Task。
- 先预览客户、明细、税额和总额，确认后原子保存发票、税务及 GL；重新核对预览、保护重复提交，沿用收款和导出流程。
- 价格单按发票日期查询本地汇率，支持本张发票改价；客户币种自动带入，自定义行切换币种后要求重新填写价格。
- 数据库结构升至 12，发票 Task 关联改为可选；事务迁移保留历史编号、快照、付款、来源和 GL，升级前自动备份。直接发票导出显示发票日期，不显示空 Task 信息。

## 0.10.2 — 2026-10-27

- JE 明细表改为自适应列宽，宽窗口充分利用剩余空间，Debit / Credit 金额框保持至少 150 逻辑像素并右对齐，Description 加宽；小窗口及外币额外列保留横向滚动。
- 选择框作为完整单元分配空间，统一输入框与箭头按钮高度，避免表格挤压和错位；只读历史行沿用相同列宽。
- 增加实际渲染的列宽、表格占宽及选择框对齐回归验证。

## 0.10.1 — 2026-10-27

- 加固 JE 及复用选择框的下拉交互：箭头使用固定控件 ID，列表锚定完整输入框，焦点转移不再替换已打开列表。
- 键盘 ↓ 可从关闭状态展开候选，保留前缀过滤、回车选择及外部点击关闭；修正选择“无关联”后被误判为未选定输入的问题。
- 增加 JE 多尺寸网格的实际箭头点击及候选选择、焦点转移和键盘展开回归测试。

## 0.10.0 — 2026-10-26

- 新增 Journal Entries、General Ledger、Trial Balance 和 GL Setup，支持手工草稿、过账、冲销、可自定义 JE 编号、外币及附件。
- 新 Invoice、收款、Expense 与税款记录自动生成借贷分录，同事务保存；修改费用冲销重记，撤销收款或税款保留反向分录。历史单据不追溯。
- 原币及本币双重平衡校验、汇率快照、换算尾差处理、部分收款应收余额分摊及汇兑差额；费用抵扣本币金额与税务快照对齐。
- 已过账分录与附件不可改写，新增本币固定、期间锁定、GL 科目类型保护、来源及冲销一致性验证；备份包含分录和附件。
- 分录界面采用顶部元信息、多行业务表、备注附件及固定底部合计/保存操作；导航分组可滚动、侧栏收窄、标题与表格间距统一，长文本截断后可悬停查看。
- 结构版本升至 11；升级前自动备份，新增总账设置、分录、行、来源及冲销表与审计，标准公司额外增加汇兑损益和税款调整科目。

## 0.9.0 — 2026-10-26

- 增加服务业默认 Chart of Accounts 模板，新公司包含 30 个科目，涵盖资产、负债、权益、收入和费用。
- 首次升级在同一事务中补充模板科目，编号冲突或同名同类型科目保留已有记录，默认费用和税务关联不变；用户改名、改编号或停用后不重复添加。
- 结构版本升至 10，记录模板应用状态；Chart of Accounts 页面增加模板说明，文档列出完整科目。

## 0.8.0 — 2026-10-26

- Customer 和 Vendor 增加必填 Currency，新增、编辑及列表均支持币种；默认 CAD，CLI 支持 `--currency`。
- 结构版本升至 9，现有客户与供应商自动补 CAD；币种限定为程序支持的币种，历史单据不换算，升级前自动备份。
- CLI 编辑省略 `--currency` 时保留原币种，新增默认 CAD；新增迁移及历史发票不变的回归验证。

## 0.7.2 — 2026-10-26

- 开票选择窗口仅列出通过开票校验的 completed Task，按 Customer 排序；移除不合格任务的“待处理”列表。

## 0.7.1 — 2026-10-26

- 开票工作台改按 Customer 名称排序，同客户内按币种、日期排列；客户列前置，保留 Vendor 筛选及开票预览。

## 0.7.0 — 2026-10-26

- Task 开票改为“选择任务 → Preview → Confirm”，主列表无需预先勾选也可进入；Task 编辑页统一进入该流程。
- 开票工作台按 Vendor 排序，支持客户、Vendor、关键词筛选，第一条选择锁定客户与币种，多 Vendor 任务只列一行。
- 预览展示来源任务、客户、业务、税码、分项税额、未税与含税金额；仅确认时分配编号、保存发票和锁定任务。
- 确认事务重算并比对预览，数据变化需重新预览；待处理任务显示配置问题，失败保留选择，防止重复开票。
- 结构版本升至 8，新 Task 业务行保留供应商来源；历史业务不猜测 Vendor，历史发票快照不变。

## 0.6.2 — 2026-10-26

- Days 改为按列表 Date 计算至今天，2026-10-23 至 2026-10-26 显示 3；旧任务不再因缺少创建日志而显示空白。
- 创建时间继续保留作审计，Days 只在任务日期缺失或无效时显示“—”。

## 0.6.1 — 2026-10-26

- Task 列表拆分 Task#、Date、Task name、Days、Status，保留选择框、Edit 和内容预览，支持横向滚动。
- 新增独立创建时间，Days 按温哥华日历天数计算；旧任务仅从创建审计记录恢复时间，没有记录时显示“—”。数据库结构升级至 7。

## 0.6.0 — 2026-10-26

- 新增 Setup → Sales Tax、Accounting → Sales Tax 和 Report → Sales Tax，支持机构/注册税号、关联科目、生效日期税率、自定义税码和默认税码分配。
- 支持 GST/HST、BC PST、零税率与免税；Task 逐行选择税码，Invoice 保存未税、税额及含税总额，付款与 HTML 导出使用含税余额。
- Expense 支持含税拆分、可抵扣比例及不可抵扣税计入成本，税额与 CAD 汇率保存快照。
- 增加税款缴纳、退款与手工调整记录，提供汇总、明细、待核对报表及 CSV；税务流水纳入审计和备份校验。
- 结构版本升至 6，旧发票及旧费用不追溯计税，默认 NONE 与未注册状态避免升级后自动加税。

## 0.5.2 — 2026-10-25

- 检查其他界面并统一保存操作：Expense/科目增加 Save，保留 Save & Close，Expense 保留 Save & New；移除重复标题栏关闭按钮。
- Customer/Vendor/Price List 行内编辑增加 Save & Close，Settings/Numbering 保存关闭后返回 Tasks，均仅成功后关闭编辑。
- 新增弹窗取消重复关闭入口；修正连续保存新费用或新科目时必须沿用已保存 ID，避免重复新增。

## 0.5.1 — 2026-10-25

- Task 编辑页移除重复关闭按钮，统一底部 Save / Save & Close；明细修改、新业务和费用在同一事务保存，失败保留草稿，成功后按按钮决定是否关闭。
- 切换明细保留价格与数量草稿；标记完成、取消或开票前先保存当前输入。

## 0.5.0 — 2026-10-25

- Expense 改为 Bill 式表头和多行录入，支持参考号、可选到期日、整单备注、多行费用科目与可选 Task，合计及保存并关闭/保存并新增。
- 增加费用单表头/行关联，编号按整单分配；旧费用保留原编号转换为单行费用单，修改与删行在同一事务内更新任务成本。
- 默认数据库改为 database/tasks.db，自动建目录并安全检测、迁移根目录旧库；保留原文件作迁移备份，显式 --db 路径不变。
- 数据库结构版本升至 5，备份恢复增加费用单与明细一致性校验。

## 0.4.0 — 2026-10-25

- SQLite 结构升级至 4，增加结构化发票表头、明细、应收余额视图及查询索引；历史 JSON 快照保持不变。
- 费用以 ledger_expenses 为主要记录，提供 expense_register 视图；保留旧表兼容同步，Task 成本优先读取登记记录。
- 增加 Vendor/金额、付款限额、付款不可改写或删除、编号和发票快照固定等数据库约束。
- Setup 增加 Audit Log 独立页面，记录业务数据修改前后内容、UTC 时间，支持 audit-list；日志随事务提交或回滚。
- 增加固定公司 UUID 和对应运行锁，改名、备份、恢复保持公司身份。
- 每个启动日自动备份并验证恢复，保留 14 份每日备份、5 份升级前备份；File 增加 Verify Backup，恢复前校验结构化发票和费用兼容记录。

## 0.3.1 — 2026-10-25

- Numbering 从公司设置移至 Setup 菜单下的独立页面，保留现有编号配置与自动分配规则。

## 0.3.0 — 2026-10-25

- 同一客户、相同结算币种的多个 completed Task 可合并成一张 Invoice；全部校验通过后原子保存，保留来源明细并锁定所有来源 Task。
- 增加 Invoice 独立页面、部分付款、付款撤销及未付余额；HTML 导出显示来源 Task、已收与未付。
- Settings 增加 Task、Invoice、Expense、Payment 编号规则，支持自定义前缀、最少位数和起始序号，自动分配并避免重复。
- 数据库结构升级至 3；为旧单据补编号，旧已支付标记迁移为日期未知的付款，保持历史发票快照不变。
- 备份恢复增加付款余额校验，CLI 增加合并开票、付款及编号管理命令。

## 0.2.1 — 2026-10-25

- 同数据库与同公司名称的进程互斥检查，覆盖启动、公司改名、备份恢复和 CLI。
- 重复启动显示提示；使用 Windows 运行锁，退出或崩溃后自动释放。
- Task 悬停内容预览，汇率和公司设置统一移到 Setup。
- Products & Services 移除成本显示。
- Expense 使用单据式布局，供应商必填，Task 关联可选。

## 0.2.0 — 2026-10-25

- 统一桌面主题：分组深色导航、绿色主操作、白色工作区、紧凑表格和状态标签。
- Task 状态筛选及通用列表搜索；保留简洁列表、Edit 弹窗和并排开票按钮。
- 默认费用科目改为固定 ID 关联，修复科目改编号后费用登记失败的问题。
- 费用币种统一大写，旧币种补充同步费用记录；金额显示与汇总避免浮点丢失精度。
- 缺失汇率在桌面后台查询，不再在 Task/Expense 保存事务中等待网络。
- 业务行使用稳定控件 ID，避免删除行后搜索草稿串行。
- 发票导出与备份恢复增加明细、数量、金额和合计校验；统一 HTML 发票版式。
- 增加数据库结构版本检查，拒绝更新结构的数据库。
- 清理静态检查警告，重写 README，并提供发布版构建脚本。

## 0.1.0

初版功能累计：Rust + SQLite 桌面界面、客户和供应商、价格单、单据式 Task、四种状态、批量 Invoice、公司设置、加拿大央行本地汇率缓存、Expense 与五类会计科目、顶部菜单与在线备份恢复。
