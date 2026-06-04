#[cfg(target_os = "macos")]
pub(crate) mod macos {
    use std::ffi::CString;

    unsafe extern "C" {
        unsafe fn notify_post(name: *const std::os::raw::c_char) -> u32;
    }
    pub(crate) fn notify_folder_changed() {
        unsafe {
            let key = CString::new("com.archiveClientRs.mappedFolderChanged").unwrap();
            notify_post(key.as_ptr());
        }
    }
}

#[cfg(windows)]
mod windows {
    pub(crate) fn notify_folder_changed() {
        // EMPTY
    }
}
