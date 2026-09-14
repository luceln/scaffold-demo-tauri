//! 构建脚本：Tauri 代码生成 + Windows 清单注入。
//!
//! ⚠️ 这个文件里最重要的不是 tauri-build，而是下面那段清单注入 ——
//! 没有它，Windows 上的 `cargo test` 会以 `0xc0000139 STATUS_ENTRYPOINT_NOT_FOUND`
//! 静默崩掉（详细分析见 app.manifest 的注释）。

fn main() {
    // 关掉 tauri-build 自带的清单注入。
    //
    // 它原本会做这件事，但底层 embed-resource 发出的是 `cargo:rustc-link-arg-bins`
    // —— 只对 **bin** 目标生效。`cargo test` 的测试宿主、`cargo run --example`
    // 都拿不到清单，于是在 Windows 上必崩。
    //
    // 注意：WindowsAttributes 在非 Windows 上也是一个普通结构体（源码里标了
    // #[allow(dead_code)]，没有 #[cfg(windows)] 门控），所以这一行在 Linux CI 上
    // 同样能编译，只是不起作用。
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest()),
    )
    .expect("tauri-build 失败：通常是 tauri.conf.json 不合法或缺少 frontendDist 目录");

    // 只在 Windows 上注入：`/MANIFEST:EMBED` 是 link.exe 的参数，
    // 传给 Linux 上的 ld 会变成无法识别的选项。
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("app.manifest");
        // ★ 故意用不带 `-bins` / `-tests` 后缀的形式：这个参数会作用于
        // **所有可链接目标**（bin / lib 测试宿主 / 集成测试 / example）。
        //
        // 顺带记一个 cargo 的坑：`cargo:rustc-link-arg-tests=` 不是"测试目标"的
        // 意思，它指的是**集成测试**（tests/ 目录），lib 内的 #[cfg(test)] 宿主
        // 拿不到。要覆盖全部目标，只能用这个无后缀形式。
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        println!("cargo:rerun-if-changed=app.manifest");
    }
}
