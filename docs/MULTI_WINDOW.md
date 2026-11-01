# Multi-screen and business windows (0.17.0)

## Stable dragging and recovery

Saved position and size are restored once when a business window opens. Those creation hints stay fixed while it is open; live Windows move/resize observations update preferences without sending the geometry back to the window. This prevents the application from fighting native dragging or monitor transitions. Layout writes wait until movement has settled for 400 ms and flush on close.

Saved off-screen positions fall back to the main window's display when reopening. If a monitor is disconnected while a window is already open, use the existing Window-menu action to bring business windows back to the main display. Native X close and Escape retain the confirmation flow for unsaved inputs. Return to editing preserves the draft; Discard and close releases the window without recording a payment.

目标：主屏保留导航和业务列表，副屏查看凭证或报表；录入窗口不能相互覆盖草稿。

## 参考与取舍

- [QuickBooks Desktop 多屏说明](https://quickbooks.intuit.com/learn-support/en-ca/help-article/product-setup/use-quickbooks-multiple-screens-multi-monitor-mode/L3jcGJTV0_CA_en_CA)：参考多屏业务查看方式。Axibiz 使用独立系统窗口，交给 Windows 拖动、最大化和分屏，不实现内部窗口的层叠/平铺管理。
- [Sage 50 窗口关闭与独占操作说明](https://ca-kb.sage.com/portal/app/portlets/results/botviewsolution.jsp?hypermediatext=null&solutionid=221924950016048)：参考集中管理窗口和重要操作前关闭其他窗口的交互。Axibiz 在恢复备份前要求关闭业务录入窗口。
- 单业务单录入窗口是本项目明确要求，并非声称上述产品所有版本都采用同样限制。

## 已实现的窗口规则

| 窗口组 | 唯一实例覆盖范围 |
| --- | --- |
| Task | 新建任务、编辑任务共享一个窗口 |
| Invoice | Direct invoice creation, Task merge previews and invoice details share one window |
| Receive Payment | One independent receipt-entry window; opening another receipt activates the existing draft |
| Customer | New/Edit Customer share one protected native window |
| Vendor | New/Edit Vendor share one protected native window |
| Expense / Account | 费用和科目编辑共享一个窗口，比每类分别一个更严格 |
| JE | 新建、查看及编辑分录共享一个窗口 |
| General Ledger | 总账/试算平衡一个独立查看窗口 |
| Reconciliation | 截止日对账一个独立查看窗口 |
| Accounting Checks | 账务检查一个独立查看窗口 |

不同窗口组可以同时打开。重复打开任何录入入口时，激活已有窗口，保持原来的记录及输入，不自动切换。需要换单据时，先保存并关闭或明确放弃原窗口。直接开票或合并开票成功后，可以在同一 Invoice 窗口内进入生成的发票详情。

Customer Center and the other navigation lists stay in the main workspace. New/Edit Customer uses its own native window. Arbitrary list detachment and multiple copies of the same report with independent filters are not implemented; each independent report retains one instance.

## 使用与多屏行为

- 业务窗口使用系统标题栏，可拖至其他显示器。窗口内容直接占用客户区，查找候选和辅助录入仍留在所属窗口内。
- 主菜单 Window 列出已打开的业务窗口，点击可恢复最小化并激活。
- General Ledger 页面提供“在独立窗口查看总账 / 试算平衡”。独立查看期间主工作台不再重复渲染同一份报表。
- 按窗口组保存普通窗口的位置和尺寸，不自动恢复未保存草稿。布局偏好保存在数据库同目录、同名的 `.windows.json` 中，与账务数据分开。
- Windows 下检查保存位置是否与现有显示器相交；不可见位置回退到主窗口所在屏幕。允许副屏位于主屏左侧或上方。
- 提供“将业务窗口移回主窗口所在屏幕”，恢复窗口后按偏移排列。首次尺寸参考当前屏幕大小；系统负责原生 DPI 缩放。

## 数据保护

- 所有窗口属于同一应用进程，继续共用数据库连接及既有事务，不启动第二个业务进程或复制数据库。
- 窗口关闭按钮先确认是否放弃输入；返回编辑不会重建或清空表单。Task、费用、发票详情和合并开票的取消/Escape 也先保护输入；JE 和直接开票保留原有 dirty/discard 检查。
- 保存并关闭只在保存成功后释放窗口；失败时保留表单。窗口管理层不代替数据库校验。
- Task 保存前重新检查是否已开票或取消；已锁定时拒绝写入，保留编辑现场。
- 数据库发生变更后请求主窗口及其他已打开窗口重绘，保留查询条件。报表重新读库；开票预览继续在确认时重新校验，而不静默重建正在编辑的草稿。
- 主窗口退出检查业务窗口和 PDF/打印后台任务；用户可以返回工作台，或明确放弃未保存输入并退出。恢复备份在录入窗口仍打开时拒绝执行。
- 重复过账、重复开票、已锁账日期等继续由既有业务事务和状态校验处理。单窗口约束是额外交互保护，不是全部数据安全措施。

## 验收

自动回归覆盖业务窗口去重、JE 未保存行保留、Task 新建/编辑互斥、Invoice 各入口互斥、关闭确认保留输入、窗口组隔离及非法布局过滤，并继续执行完整业务测试、格式检查和 Clippy。

发布后需在实际双屏设备人工验证：100%/150% 混合缩放、拖动跨屏、Windows 分屏、负坐标显示器、断开副屏后的窗口恢复、最小化激活、关闭和退出确认、保存后关联报表刷新。无头渲染测试不能证明物理多屏下的所有原生行为。
