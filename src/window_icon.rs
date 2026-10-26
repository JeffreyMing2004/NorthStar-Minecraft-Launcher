pub fn data() -> egui::IconData {
    let image = image::load_from_memory(include_bytes!("../assets/nscl-icon.png"))
        .expect("embedded application icon is valid")
        .into_rgba8();
    egui::IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    }
}

pub fn initialize() {
    #[cfg(windows)]
    {
        let id: Vec<u16> = crate::store::APP_IDENTIFIER
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // SAFETY: id is a valid, NUL-terminated string and remains alive for the call.
        let result = unsafe {
            windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(id.as_ptr())
        };
        if result < 0 {
            eprintln!("Could not set the taskbar application identity: {result:#x}");
        }
    }
}
