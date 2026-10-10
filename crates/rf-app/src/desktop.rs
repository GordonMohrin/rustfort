//! Move the game window to a given Windows virtual desktop (for users who work with several
//! desktops: automated screenshot/test runs should not pop up on the active desktop).
//!
//! Uses the documented `IVirtualDesktopManager::MoveWindowToDesktop` with the desktop GUIDs
//! Windows keeps in `HKCU\...\Explorer\VirtualDesktops\VirtualDesktopIDs` (16 bytes per
//! desktop, in desktop order). Best effort: any failure only logs a line.

/// Which desktop to use: `--desktop N` wins, then `RUSTFORT_DESKTOP`, then desktop 1 for
/// screenshot/test runs; `None` = leave the window where Windows opened it (normal play).
pub fn wanted(cli: Option<usize>, screenshot: bool) -> Option<usize> {
    if let Some(n) = cli {
        return (n > 0).then_some(n);
    }
    if let Ok(v) = std::env::var("RUSTFORT_DESKTOP") {
        return v.trim().parse::<usize>().ok().filter(|&n| n > 0);
    }
    screenshot.then_some(1)
}

#[cfg(windows)]
pub fn move_to(n: usize) {
    match imp::move_own_window(n) {
        Ok(true) => eprintln!("rustfort: window moved to virtual desktop {n}"),
        Ok(false) => {}
        Err(e) => eprintln!("rustfort: could not move the window to desktop {n}: {e}"),
    }
}

#[cfg(not(windows))]
pub fn move_to(_n: usize) {}

#[cfg(windows)]
mod imp {
    use windows::core::{GUID, PCWSTR};
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED};
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_BINARY};
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::Shell::{IVirtualDesktopManager, VirtualDesktopManager};
    use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, IsWindowVisible};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// GUIDs of all virtual desktops in order (empty with a single desktop).
    fn desktop_ids() -> Result<Vec<GUID>, String> {
        let key = wide(r"Software\Microsoft\Windows\CurrentVersion\Explorer\VirtualDesktops");
        let val = wide("VirtualDesktopIDs");
        let mut buf = vec![0u8; 16 * 64];
        let mut len = buf.len() as u32;
        let r = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                PCWSTR(val.as_ptr()),
                RRF_RT_REG_BINARY,
                None,
                Some(buf.as_mut_ptr() as *mut _),
                Some(&mut len),
            )
        };
        if r.is_err() {
            return Ok(Vec::new());
        }
        Ok(buf[..len as usize]
            .chunks_exact(16)
            .map(|b| {
                GUID::from_values(
                    u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                    u16::from_le_bytes([b[4], b[5]]),
                    u16::from_le_bytes([b[6], b[7]]),
                    [b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]],
                )
            })
            .collect())
    }

    unsafe extern "system" fn find(hwnd: HWND, lp: LPARAM) -> BOOL {
        let out = &mut *(lp.0 as *mut (u32, Option<HWND>));
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == out.0 && IsWindowVisible(hwnd).as_bool() {
            out.1 = Some(hwnd);
            return BOOL(0);
        }
        BOOL(1)
    }

    pub fn move_own_window(n: usize) -> Result<bool, String> {
        let ids = desktop_ids()?;
        if ids.len() < 2 || n == 0 || n > ids.len() {
            return Ok(false);
        }
        let mut st: (u32, Option<HWND>) = (unsafe { GetCurrentProcessId() }, None);
        unsafe {
            let _ = EnumWindows(Some(find), LPARAM(&mut st as *mut _ as isize));
        }
        let Some(hwnd) = st.1 else { return Err("game window not found".into()) };
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let mgr: IVirtualDesktopManager = CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_ALL).map_err(|e| e.to_string())?;
            mgr.MoveWindowToDesktop(hwnd, &ids[n - 1]).map_err(|e| e.to_string())?;
        }
        Ok(true)
    }
}
