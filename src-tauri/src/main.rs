// release 构建隐藏控制台窗口；debug 保留控制台，便于用 CLI 模式做实测
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    duoswitch_lib::run()
}
