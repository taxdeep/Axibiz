# 默认会计科目模板

Axibiz 内置服务业模板 `axibiz-services-v1`。新公司首次启动自动建立以下 30 个科目；已有公司首次升级时补充缺失项。科目可在 Chart of Accounts 页面修改编号、名称及启用状态。

| 编号 | 名称 | 类型 |
| --- | --- | --- |
| 1000 | Cash on Hand · 库存现金 | Asset |
| 1010 | Bank - CAD · 加元银行账户 | Asset |
| 1020 | Bank - USD · 美元银行账户 | Asset |
| 1100 | Accounts Receivable · 应收账款 | Asset |
| 1200 | GST/HST Recoverable · 可抵扣 GST/HST | Asset |
| 1500 | Equipment · 设备 | Asset |
| 1590 | Accumulated Depreciation · 累计折旧 | Asset |
| 2000 | Accounts Payable · 应付账款 | Liability |
| 2100 | Credit Card Payable · 信用卡应付款 | Liability |
| 2200 | GST/HST Payable · 应缴 GST/HST | Liability |
| 2210 | BC PST Payable · 应缴 BC PST | Liability |
| 2300 | Loans Payable · 应付贷款 | Liability |
| 3000 | Owner Capital · 业主投入 | Equity |
| 3100 | Retained Earnings · 留存收益 | Equity |
| 3200 | Owner Drawings · 业主提款 | Equity |
| 4000 | Service Revenue · 服务收入 | Income |
| 4100 | Other Revenue · 其他收入 | Income |
| 5000 | General Expenses · 一般费用 | Expense |
| 5100 | Subcontractor Expenses · 外包费用 | Expense |
| 5200 | Registry and Filing Fees · 注册及申报费 | Expense |
| 5300 | Software and Subscriptions · 软件及订阅 | Expense |
| 5400 | Professional Fees · 专业服务费 | Expense |
| 5500 | Office Supplies · 办公用品 | Expense |
| 5600 | Rent and Utilities · 租金及水电 | Expense |
| 5700 | Bank and Payment Fees · 银行及支付手续费 | Expense |
| 5800 | Travel Expenses · 差旅费 | Expense |
| 5900 | Advertising and Marketing · 广告推广 | Expense |
| 6000 | Insurance · 保险 | Expense |
| 6100 | Repairs and Maintenance · 维修维护 | Expense |
| 6200 | Depreciation Expense · 折旧费用 | Expense |

程序中的科目名称使用表内英文，可自行改成中文。Asset 为资产，Liability 为负债，Equity 为权益，Income 为收入，Expense 为费用。

升级时保留已有同编号科目，包括已停用或不同类型的科目；同名同类型但编号不同的科目也不重复添加。默认费用科目和税务控制科目沿用已配置的固定关联，因此旧公司实际编号可能不同于表中示例。模板完成后记录应用状态，之后不会因改名、改编号或停用而重新补回原编号。

Expense 录入仅使用启用的 Expense 科目。默认费用科目及税务控制科目仍受原有类型与启用状态保护。模板本身不建立期初余额。0.10.0 起新增 GL 自动入账，并额外建立 6300 汇兑损益和 6400 税款调整科目，标准新公司共 32 项；参见 [GL 使用说明](GENERAL_LEDGER.md)。银行名称中的 CAD / USD 是名称标签，费用和发票的实际币种仍由单据记录。
