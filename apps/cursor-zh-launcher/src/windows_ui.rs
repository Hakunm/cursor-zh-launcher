// SPDX-License-Identifier: GPL-3.0-only

#[cfg(any(not(windows), debug_assertions))]
pub fn report_error(message: &str) {
    eprintln!("Cursor 中文启动器错误：{message}");
}

#[cfg(any(not(windows), debug_assertions))]
pub fn report_warning(message: &str) {
    eprintln!("Cursor 中文启动器警告：{message}");
}

#[cfg(any(not(windows), debug_assertions))]
pub fn prompt_update(tag: &str, release_url: &str) {
    eprintln!("Cursor 中文启动器有新版本 {tag}：{release_url}");
}

#[cfg(all(windows, not(debug_assertions)))]
#[allow(unsafe_code)]
pub fn report_error(message: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};

    let title = wide("Cursor 中文启动器");
    let body = wide(&format!("启动失败：\n\n{message}"));
    // SAFETY: Both UTF-16 buffers are NUL-terminated and remain alive for the
    // duration of the synchronous MessageBoxW call. A null owner is allowed.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            body.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

#[cfg(all(windows, not(debug_assertions)))]
#[allow(unsafe_code)]
pub fn report_warning(message: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONWARNING, MB_OK, MessageBoxW};

    let title = wide("Cursor 中文启动器");
    let body = wide(message);
    // SAFETY: Both UTF-16 buffers are NUL-terminated and remain alive for the
    // duration of the synchronous MessageBoxW call. A null owner is allowed.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            body.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONWARNING,
        );
    }
}

#[cfg(all(windows, not(debug_assertions)))]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(all(windows, not(debug_assertions)))]
#[allow(unsafe_code)]
pub fn prompt_update(tag: &str, release_url: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IDYES, MB_ICONINFORMATION, MB_YESNO, MessageBoxW,
    };

    let title = wide("Cursor 中文启动器更新");
    let body = wide(&format!(
        "发现新版本 {tag}。\n\n是否打开 GitHub Releases 页面？"
    ));
    // SAFETY: Both strings are valid NUL-terminated UTF-16 buffers and the
    // synchronous call does not retain their pointers.
    let response = unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            body.as_ptr(),
            title.as_ptr(),
            MB_YESNO | MB_ICONINFORMATION,
        )
    };
    if response == IDYES {
        let _ = open::that(release_url);
    }
}
