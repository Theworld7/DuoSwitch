; DuoSwitch 安装器钩子

; 目标：卸载时清除用户配置（%APPDATA%\<identifier>\config.json）与
; WebView2 数据目录（%LOCALAPPDATA%\<identifier>\），让「卸载后重新安装」
; 等同于全新安装（首次启动重新填充内置壁纸）。
;
; 实现方式：这里只把 Tauri 内置的「删除应用数据」分支置为启用，
; 而不是自己写 RmDir —— 内置分支带有 $UpdateMode 保护，
; 升级安装（先卸载旧版再装新版）时不会误删用户数据。
;
; 副作用：卸载确认页的那个复选框仍会显示且默认未勾选，但无论勾不勾都会删除。
; Tauri 的模板没有提供「改变复选框默认状态」的钩子，只能这样。

!macro NSIS_HOOK_PREUNINSTALL
  StrCpy $DeleteAppDataCheckboxState 1
!macroend
