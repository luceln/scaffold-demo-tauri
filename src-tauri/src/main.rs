// 发布版不额外弹出控制台窗口（debug 版保留，方便看日志输出）。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // ★ 注意这里用的是**固定库名** `tauri_app_lib`，不是包名。
    //
    // 包名来自用户填的 scaffold-demo-tauri，是 kebab-case（如 my-tool），
    // 而 Rust 标识符不允许连字符 —— 契约的渲染层又没有"连字符→下划线"
    // 的变换能力（见 Cargo.toml 里 [lib] 的注释）。
    // 所以库名被钉成常量，整个骨架不依赖任何名称变换。
    tauri_app_lib::run();
}
