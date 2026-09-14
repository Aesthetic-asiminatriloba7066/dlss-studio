#[cfg(windows)]
use windows::core::PCWSTR;
#[cfg(windows)]
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, LPARAM, WPARAM};
#[cfg(windows)]
use windows::Win32::System::Threading::CreateMutexW;
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, SetForegroundWindow, ShowWindow, SW_RESTORE};

pub struct SingleInstanceGuard {
    #[cfg(windows)]
    handle: HANDLE,
}

#[cfg(windows)]
impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        if !self.handle.is_invalid() {
            unsafe {
                let _ = CloseHandle(self.handle);
            }
        }
    }
}

/// Acquires a named Windows mutex. If the mutex is already held by another process, returns `None`.
pub fn acquire_named_mutex(mutex_name: PCWSTR) -> Option<SingleInstanceGuard> {
    #[cfg(windows)]
    unsafe {
        let handle = match CreateMutexW(None, true, mutex_name) {
            Ok(h) => h,
            Err(e) => {
                crate::core::logger::warn("single_instance", &format!("CreateMutexW failed: {:?}", e));
                return None;
            }
        };

        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(handle);
            return None;
        }

        Some(SingleInstanceGuard { handle })
    }

    #[cfg(not(windows))]
    None
}

/// Tries to acquire single-instance ownership for DLSS Studio.
/// If an instance is already running, signals the running instance to restore/show its window and returns `None`.
/// If this is the primary instance, returns `Some(SingleInstanceGuard)`.
pub fn acquire_single_instance() -> Option<SingleInstanceGuard> {
    #[cfg(windows)]
    {
        let mutex_name = windows::core::w!("Local\\DLSS5_Swapper_Rust_SingleInstance_Mutex");
        if let Some(guard) = acquire_named_mutex(mutex_name) {
            Some(guard)
        } else {
            signal_existing_instance();
            None
        }
    }

    #[cfg(not(windows))]
    None
}

/// Signals the existing instance via its Win32 tray/message window to restore and focus its UI.
pub fn signal_existing_instance() {
    #[cfg(windows)]
    unsafe {
        let class_name = windows::core::w!("DLSS5SwapperTrayClass");
        let window_name = windows::core::w!("DLSS5SwapperTrayWindow");
        if let Ok(hwnd) = FindWindowW(class_name, window_name) {
            if !hwnd.0.is_null() {
                let _ = PostMessageW(hwnd, crate::core::tray::WM_SHOW_WINDOW, WPARAM(0), LPARAM(0));
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(hwnd);
                crate::core::logger::info("single_instance", "Signaled existing instance via Win32 tray window");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_instance_guard_lifecycle() {
        let test_mutex = windows::core::w!("Local\\DLSS5_Swapper_Test_Mutex_9999");
        let guard = acquire_named_mutex(test_mutex);
        assert!(guard.is_some(), "First acquisition in test must succeed");
        let second = acquire_named_mutex(test_mutex);
        assert!(second.is_none(), "Second acquisition while guard is held must be None");
        drop(guard);
        let third = acquire_named_mutex(test_mutex);
        assert!(third.is_some(), "Acquisition after dropping guard must succeed");
    }

    #[test]
    fn test_signal_existing_instance_call() {
        // Safe to call even when no window is active (FindWindow returns 0 / handles null gracefully)
        signal_existing_instance();
    }
}
