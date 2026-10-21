#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let window = tauri::webview::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::default(),
            )
            .title("NorthStar Minecraft Launcher")
            .inner_size(1200.0, 760.0)
            .min_inner_size(960.0, 640.0)
            .decorations(false);

            // 部分开发机的安全策略会拦截未签名程序在系统盘创建 WebView2 数据目录，
            // 调试构建改为存放在可执行文件旁，发布构建仍使用系统默认位置。
            #[cfg(debug_assertions)]
            let window = match std::env::current_exe() {
                Ok(exe) => {
                    let data_dir = exe
                        .parent()
                        .map(|dir| dir.join("WebView2-dev"))
                        .unwrap_or_else(|| std::path::PathBuf::from("WebView2-dev"));
                    window.data_directory(data_dir)
                }
                Err(_) => window,
            };

            window.build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("无法启动 NSCL 桌面应用");
}
