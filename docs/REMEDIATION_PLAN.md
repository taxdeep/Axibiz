# 分阶段整治与复验计划

适用范围：加拿大 BC 服务业、单机内部账务和开票工具。用户于 2026-10-28 确认。此计划不是 CPA 审计意见或法规认证；法规要求、代码控制、人工判断分开记录。

## 第二轮：截止日对账

用户确认继续后，先补充既有手工期初/控制科目核对能力；不自动调整旧账。

| 阶段 | 实施 | 复验门禁 |
| --- | --- | --- |
| 6 收款历史 | 记录收款撤销业务日期；按截止日计算发票应收；仅凭原 GL 冲销恢复旧日期，未知项明确标注 | 截止日之前/之后、重复撤销、锁账、历史缺失、只读回归；fmt/Clippy/全测 |
| 7 截止日对账 | 按币种显示原币应收；来源快照账面应收与相应 GL 控制科目比较；CAD 税务子账与控制科目比较；不完整数据和外币不可比项不得显示通过 | 手工差异、外币账面值、控制科目变更、历史期初、税款撤销、只读及 GUI 回归；fmt/Clippy/全测 |
| 8 发布 | CSV 明细、文档、版本、隔离副本升级及真实 PDF | 发布门禁、原业务字段比较；不实际发纸 |

应收本币比较使用原过账账面值及收款账面冲销值，不使用当前汇率重估。历史资料或期初来源无法核实时不输出“对账相符”。税务余额为 CAD；GL 本币非 CAD 时不直接比较两种币种。对账差异是调查线索，不自动认定错账或做调整。

阶段 6：已通过。收款撤销日期、截止余额、锁账/重复/早于原交易保护回归；100 自动测试通过，fmt/Clippy 通过。

阶段 7：已通过。截止日原币应收、来源账面值与 GL、CAD 税务净控制余额、非 CAD 不可比、科目变更及重叠映射、手工差异、未知历史日期和 GUI 只读测试；105 自动测试通过，fmt/Clippy 通过。

阶段 8：已通过。发布 0.13.0 / 结构 15；补充重记收款倒填导致历史负数应收的回归，最终 106 自动测试通过、真实 PDF 单独通过，fmt/Clippy/发布构建通过。结构 12 和 14 的隔离副本分别升级到 15，原 41/42 张表的原字段比较通过，源文件哈希未变。CLI 对账与 8 列 CSV 实测通过；没有实际发纸。

第二轮实证：

- `target/verify-upgrade-1793217858988665500-7e6b8a35/result.json`（12 → 15）。
- `target/verify-upgrade-1793217859002287000-09d708ec/result.json`（14 → 15）。
- `target/verify-upgrade-1793217858988665500-7e6b8a35/reconciliation-result.json`（2026-10-28 CLI 与 CSV）。该隔离副本的应收与税务合并控制余额在已记录范围内相符；资料核对提醒 2 项仍须人工处理。
- 并行验证时发现仅用系统时间命名的验收目录可能碰撞，脚本已加入随机隔离标识，两次并行复验通过；不覆盖任何已有目录。

本轮应收比较以来源快照账面值对照实际 GL，不是独立重算每次收款汇率分摊；原币余额单独从发票与收款记录计算。税务采用合并净控制余额，不能证明各机构及销项/进项分别正确，仍需 Sales Tax 逐机构核对。未知期初不被默认为零，也不自动回填交易日期。

## 阶段与通过条件

| 阶段 | 本轮实施范围 | 必须通过的复验 |
| --- | --- | --- |
| 1 状态隔离 | 集中恢复后的 UI 重置；清空旧价格、模板、往来和任务草稿；隔离旧后台结果；删除只写不读的旧价格字段 | 恢复后不显示旧草稿、不接收旧后台结果；完整回归 |
| 2 会计期间 | 检查借贷平衡、锁账、冲销、汇兑、税务权责发生时点；修复税款撤销导致历史余额被改写的问题；明确旧数据缺少撤销日期的边界 | 原交易日和撤销日分别列示；历史截至余额稳定；锁定/未来日期失败原子回滚；升级、恢复回归 |
| 3 单据与合规检查 | 新发票冻结付款约定；增加只读合规检查入口，检查资料缺失及已知账务控制；检查保管与不可篡改边界 | 改模板不改新发票既有约定；历史数据不虚构补填；检查不写业务数据；资料缺失不冒充合规通过 |
| 4 一致性与维护 | 客户/供应商采用行尾 Edit 弹窗；整理通用金额命名和状态职责；发票列表汇总查询/限制展示，避免每帧逐票重复查询 | 实际按钮交互、取消/失败保留、搜索及发票余额与业务校验结果一致；完整回归 |
| 5 发布复验 | 发布版本与数据库版本明确对应；更新变更、审查和范围说明；真实 PDF、构建和整套回归 | 前四阶段全部通过；真实浏览器 PDF；不向打印机实际发纸；不修改真实公司业务数据 |

每阶段运行 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`。失败必须修复并重跑，再进入下一阶段。阶段结果记录在文末；最终用 `build.ps1` 执行发布门禁。

## 核对依据

- [CRA：ITC documentary requirements](https://www.canada.ca/en/revenue-agency/services/tax/businesses/topics/gst-hst-businesses/calculate-prepare-report/input-tax-credit.html)：付款条款等资料要求分交易金额及情形，资料可由发票、合同等共同支持；软件检查提示不等于抵扣批准。
- [CRA：GST/HST invoices and place of supply](https://www.canada.ca/en/revenue-agency/services/tax/businesses/topics/gst-hst-businesses/charge-collect-which-rate.html)：供应地点影响税率；销项税申报通常按发票日期而非实际收款日期。保留发票日期，移除显示时间不删除内部记录时间。
- [CRA：Electronic records](https://www.canada.ca/en/revenue-agency/services/forms-publications/publications/15-2/electronic-records.html)：电子记录须保持可读，通常在相关年度结束后保留六年；14 份每日备份不等于六年归档。
- [BC 政府 2026-09-18 公告](https://archive.news.gov.bc.ca/releases/news_releases_2024-2028/2026PREM0034-001089.htm)：原计划 2026-10-01 扩大到部分专业服务的 PST 政策暂停。不得依据较早预算公告，把所有 BC 专业服务默认设置为 GST + PST；既有应税服务仍须按实际规则判断。核查日期 2026-10-28。
- [CRA：Out-of-pocket expenses](https://www.canada.ca/en/revenue-agency/services/forms-publications/publications/gi-197/out-of-pocket-expenses.html)：代理垫付与自身费用转收不同；本程序没有自动判断代理关系的功能。

## 需要人工判断或另立功能项目的范围

- 按实际供应性质和供应地点选择税码；BC PST 并非所有服务统一适用，不能仅按公司在 BC 自动收税。
- ITC 商业用途、单据真实性、供应商注册、餐饮/车辆/混合用途及其他限制，需要证据和人工审查；当前费用自动计算不能代替资格确认。
- 正式信用票据、退货退款、客户预收款/递延收入、供应商付款核销、银行对账、完整报表和申报期工作流尚非本轮补齐的功能。
- 历史 GL 期初和子账控制科目对账由使用者核对；试算平衡只能证明借贷相等，不能证明分类或事实正确。
- 同一 Windows 账户可直接修改 SQLite；本轮不声称实现不可篡改审计或权限分离。
- 外部归档、备份恢复演练和凭证保管属于运行制度；不会自动删除六年历史记录或按 14 份备份轮换代替归档。

## 执行记录

- 基线：前一轮审查 93 测试通过，1 浏览器测试按需执行；格式和 Clippy 通过。
- 阶段 1：已通过。恢复重置集中化，价格草稿和后台接收端清空，删除旧价格只写字段；新增回归。94 测试通过，1 浏览器测试跳过；fmt / Clippy 通过。
- 阶段 2：已通过。税款撤销记录业务日期，原交易与反向变动分期显示；未知历史日期不猜填；新增锁账/早于原交易/重复撤销/截至余额回归。95 测试通过，1 浏览器测试跳过；fmt / Clippy 通过。借贷平衡、外币收款分摊、费用冲销和事务回滚的既有测试均通过。
- 阶段 3：已通过。新发票付款条款和说明冻结于快照；历史缺失不自动套入新条款。新增预览资料提醒、Report → Accounting Checks 和 CLI accounting-check，只读检查明确提示人工边界。96 测试通过，1 浏览器测试跳过；fmt / Clippy 通过。另核查 BC 专业服务 PST 扩围暂停公告，未自动更改用户税码。
- 阶段 4：已通过。客户/供应商行尾 Edit 弹窗及实际点击/失败/取消回归；Task UI 按创建、列表、编辑拆分；金额函数改为 parse_money / format_money 并独立成模块。发票概览单次汇总查询分页、Unicode 搜索，与完整余额验证结果对照。98 测试通过，1 浏览器测试跳过；fmt / Clippy 通过。
- 阶段 5：已通过。发布为 0.12.0 / 结构 14；新增历史撤销日期迁移回归，99 自动测试通过、1 按需浏览器测试另行通过，fmt / Clippy / 发布构建通过。真实公司库在线备份的隔离副本从 12 升到 14，原 41 张表的原字段数据逐表比较通过，原审计历史保留，完整性和外键检查通过，原库文件哈希未变。未实际发送纸质打印；未进行全界面人工逐像素验收。

### 发布复验产物

- 可复现脚本：`python scripts/verify_upgrade.py`。先保护隔离目录，在线备份源库，仅对隔离副本替换身份以避免与正在运行的公司冲突，然后升级、检查并比较原列数据。脚本不修改真实公司身份或业务数据。
- 本轮摘要：`target/verify-upgrade-1793215227969926300/result.json`。
- 真实公司副本检查提醒 2 项：`PAYMENT_TERMS`（历史付款条款缺失）、`TAX_UNSPECIFIED`（历史税码未指定）。这些是需要原合同/原始资料核对的事项，未修改历史单据。
- 后续独立项目：如需完整供应商付款、信用票据、预收款、申报期控制、独立服务隔离，应另设数据模型和验收场景。剩余大型 CLI/迁移函数和部分复杂网格可继续分批整理，不以本轮局部重构声称全部代码架构已重写。

## 0.19.0 integration follow-up (2026-10-30)

The earlier local Vendor redesign diverged from Customer and application navigation. The integration follow-up replaces the two workspace paths with shared UI/read/report components, unifies typography and profile fields, centralizes settlement date/bank/FX checks and adds reviewed idempotent customer receipts. Old financial records are retained; unknown historical reconciliation remains explicit.

The implementation and residual work are tracked in [ARCHITECTURE_REVIEW.md](ARCHITECTURE_REVIEW.md), with release evidence in [AUDIT.md](../AUDIT.md). Earlier statements that supplier settlement is unimplemented describe those historical phases; Pay Bills was implemented in 0.18.0. Credits, advances and arbitrary legacy settlement import remain separate work.

## 0.20.0 interaction audit follow-up (2026-10-31)

Reviewed all registered modules and supporting interaction families against the UI design language. Shared commands, search, registers, financial review and unsaved-input guards now supplement the existing shared party/document/settlement components. Removed obsolete party creation paths, protected inline settings drafts and added transactional reviewed Bill/tax/task expense paths and journal post/reverse review. Schema remains 19.

The authoritative module matrix, component boundaries, automated evidence and remaining design/physical acceptance targets are in [UI_INTERACTION_AUDIT.md](UI_INTERACTION_AUDIT.md). Final release and isolated company-copy evidence are in [AUDIT.md](../AUDIT.md).

## 0.21.0 designer review implementation (2026-10-31)

Implemented shared vector icons and row-sized targets, separated party tabs/commands, consolidated Bill review continuations, added optional advanced Bill columns, tightened receipt layout and standardized workflow footer guidance/reset confirmation. Financial review/commit transactions and singleton/dirty guards remain in place. See [UI_DESIGN_REVIEW.md](UI_DESIGN_REVIEW.md) for implementation status and [AUDIT.md](../AUDIT.md) for release evidence.

The additional request for an expandable Invoice template system is addressed by [DOCUMENT_TEMPLATE_PLAN.md](DOCUMENT_TEMPLATE_PLAN.md). It scopes a practical Invoice library/editor first, with versioned adapter/rendering contracts and historical appearance protection. Future Deposit/PO integration is deferred until those business functions are requested.
