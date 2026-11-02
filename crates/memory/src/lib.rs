use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessModule {
    pub base_address: usize,
    pub size: usize,
}

impl ProcessModule {
    /// Address of `offset..offset+size` inside the module image, when the
    /// range is non-empty and fits the image.
    pub fn range_address(&self, offset: usize, size: usize) -> Option<usize> {
        if size == 0 {
            return None;
        }
        let end = offset.checked_add(size)?;
        if end > self.size {
            return None;
        }
        self.base_address.checked_add(offset)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MemoryError {
    UnsupportedPlatform,
    AttachFailed(&'static str),
    ReadFailed { size: usize },
    InvalidRead { size: usize },
}

impl fmt::Display for MemoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlatform => write!(f, "process memory acquisition requires Windows"),
            Self::AttachFailed(message) => write!(f, "process attach failed: {message}"),
            Self::ReadFailed { size } => write!(f, "process read failed: {size} bytes"),
            Self::InvalidRead { size } => write!(f, "invalid process read: {size} bytes"),
        }
    }
}

impl std::error::Error for MemoryError {}

pub type MemoryResult<T> = Result<T, MemoryError>;

/// What a reader of game memory needs: the module image the layout applies
/// to, a copy into a caller-owned buffer, and process liveness.
pub trait ProcessMemory {
    fn main_module(&self) -> ProcessModule;
    fn read_into(&self, address: usize, buffer: &mut [u8]) -> MemoryResult<()>;
    fn is_alive(&self) -> MemoryResult<bool>;
}

#[cfg(windows)]
mod windows_impl {
    use super::*;
    use std::ffi::OsString;
    use std::mem::{size_of, zeroed};
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_PARTIAL_COPY, HANDLE, INVALID_HANDLE_VALUE, STILL_ACTIVE,
    };
    use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
    use windows_sys::Win32::System::ProcessStatus::{
        EnumProcessModulesEx, GetModuleFileNameExW, GetModuleInformation, LIST_MODULES_ALL,
        MODULEINFO,
    };
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_INFORMATION,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
    };

    /// Reads another process through `ReadProcessMemory`.
    #[derive(Debug)]
    pub struct WindowsProcessAttachment {
        handle: Handle,
        main_module: ProcessModule,
    }

    impl WindowsProcessAttachment {
        pub fn attach_by_pid(process_id: ProcessId) -> MemoryResult<Self> {
            let handle = open_read_only(process_id)?;
            let main_module = main_module(handle.raw())?;
            Ok(Self { handle, main_module })
        }

        pub fn read_exact(&self, address: usize, size: usize) -> MemoryResult<Vec<u8>> {
            if size == 0 {
                return Err(MemoryError::InvalidRead { size });
            }
            let mut buffer = vec![0_u8; size];
            let mut bytes_read = 0;
            let ok = unsafe {
                ReadProcessMemory(
                    self.handle.raw(),
                    address as *const _,
                    buffer.as_mut_ptr().cast(),
                    size,
                    &mut bytes_read,
                )
            };
            if ok == 0 || bytes_read != size {
                return Err(MemoryError::ReadFailed { size });
            }
            Ok(buffer)
        }

        pub fn read_module_exact(&self, offset: usize, size: usize) -> MemoryResult<Vec<u8>> {
            let address = self
                .main_module
                .range_address(offset, size)
                .ok_or(MemoryError::InvalidRead { size })?;
            self.read_exact(address, size)
        }
    }

    impl ProcessMemory for WindowsProcessAttachment {
        fn main_module(&self) -> ProcessModule {
            self.main_module
        }

        fn read_into(&self, address: usize, buffer: &mut [u8]) -> MemoryResult<()> {
            let size = buffer.len();
            if size == 0 || address.checked_add(size).is_none() {
                return Err(MemoryError::InvalidRead { size });
            }
            let mut bytes_read = 0;
            let ok = unsafe {
                ReadProcessMemory(
                    self.handle.raw(),
                    address as *const _,
                    buffer.as_mut_ptr().cast(),
                    size,
                    &mut bytes_read,
                )
            };
            if ok == 0 || bytes_read != size {
                return Err(MemoryError::ReadFailed { size });
            }
            Ok(())
        }

        fn is_alive(&self) -> MemoryResult<bool> {
            let mut exit_code = 0;
            let ok = unsafe { GetExitCodeProcess(self.handle.raw(), &mut exit_code) };
            if ok == 0 {
                return Err(MemoryError::AttachFailed("failed to query process state"));
            }
            Ok(exit_code == STILL_ACTIVE as u32)
        }
    }

    #[derive(Debug)]
    struct Handle(HANDLE);

    impl Handle {
        fn raw(&self) -> HANDLE {
            self.0
        }
    }

    impl Drop for Handle {
        fn drop(&mut self) {
            if self.0 != 0 && self.0 != INVALID_HANDLE_VALUE {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
    }

    fn open_read_only(process_id: ProcessId) -> MemoryResult<Handle> {
        let access =
            PROCESS_VM_READ | PROCESS_QUERY_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION;
        let handle = unsafe { OpenProcess(access, 0, process_id.0) };
        if handle == 0 {
            return Err(MemoryError::AttachFailed(
                "failed to open process for read/query access",
            ));
        }
        Ok(Handle(handle))
    }

    fn main_module(handle: HANDLE) -> MemoryResult<ProcessModule> {
        let mut modules = [0isize; 1024];
        let mut needed = 0;
        let ok = unsafe {
            EnumProcessModulesEx(
                handle,
                modules.as_mut_ptr(),
                (modules.len() * size_of::<isize>()) as u32,
                &mut needed,
                LIST_MODULES_ALL,
            )
        };
        if ok == 0 || modules[0] == 0 {
            let last_error = unsafe { GetLastError() };
            let message = if last_error == ERROR_PARTIAL_COPY {
                "failed to enumerate process modules for this architecture"
            } else {
                "failed to enumerate process modules"
            };
            return Err(MemoryError::AttachFailed(message));
        }
        let module_count = needed as usize / size_of::<isize>();
        if module_count == 0 || module_count > modules.len() {
            return Err(MemoryError::AttachFailed(
                "process module list changed or exceeded the bounded buffer",
            ));
        }
        let image_path = process_image_path(handle)?;
        let Some(main_module) = modules[..module_count]
            .iter()
            .copied()
            .find(|module| module_path(handle, *module).is_some_and(|path| path == image_path))
        else {
            return Err(MemoryError::AttachFailed(
                "failed to identify the process main module",
            ));
        };

        let mut info: MODULEINFO = unsafe { zeroed() };
        let ok = unsafe {
            GetModuleInformation(
                handle,
                main_module,
                &mut info,
                size_of::<MODULEINFO>() as u32,
            )
        };
        if ok == 0 || info.lpBaseOfDll.is_null() || info.SizeOfImage == 0 {
            return Err(MemoryError::AttachFailed(
                "failed to inspect process main module",
            ));
        }
        Ok(ProcessModule {
            base_address: info.lpBaseOfDll as usize,
            size: info.SizeOfImage as usize,
        })
    }

    fn process_image_path(handle: HANDLE) -> MemoryResult<String> {
        let mut buffer = vec![0_u16; 32768];
        let mut length = buffer.len() as u32;
        let ok = unsafe {
            QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut length)
        };
        if ok == 0 || length == 0 {
            return Err(MemoryError::AttachFailed(
                "failed to query the process image path",
            ));
        }
        Ok(normalize_windows_path(&buffer[..length as usize]))
    }

    fn module_path(handle: HANDLE, module: isize) -> Option<String> {
        let mut buffer = vec![0_u16; 32768];
        let length = unsafe {
            GetModuleFileNameExW(handle, module, buffer.as_mut_ptr(), buffer.len() as u32)
        };
        if length == 0 || length as usize >= buffer.len() {
            return None;
        }
        Some(normalize_windows_path(&buffer[..length as usize]))
    }

    fn normalize_windows_path(path: &[u16]) -> String {
        OsString::from_wide(path)
            .to_string_lossy()
            .replace('/', "\\")
            .to_lowercase()
    }

    #[cfg(test)]
    mod windows_tests {
        use super::*;

        #[test]
        fn attach_by_pid_can_read_current_process_main_module_header() {
            let attachment = WindowsProcessAttachment::attach_by_pid(ProcessId(std::process::id()))
                .expect("current process should be readable");
            assert!(attachment.main_module().base_address > 0);
            assert!(attachment.main_module().size > 0);
            assert!(attachment
                .is_alive()
                .expect("process liveness should be queryable"));

            let dos_header = attachment
                .read_module_exact(0, 2)
                .expect("main module header should be readable");
            assert_eq!(dos_header, b"MZ");
        }
    }
}

#[cfg(windows)]
pub use windows_impl::WindowsProcessAttachment;

#[cfg(windows)]
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

/// Reads the memory of the process it runs in. Inside a [`GameThreadReads`]
/// scope (the game thread at the frame boundary) it copies directly, with no
/// system call, tear rechecking, or region validation: nothing is freed under
/// it there. Any other thread reads while the game runs and may meet memory
/// the game just freed, so it reads through the system, which fails instead
/// of faulting.
#[derive(Debug)]
pub struct InProcessMemory {
    module: ProcessModule,
}

impl InProcessMemory {
    /// Attaches to the running image of the current process.
    #[cfg(windows)]
    pub fn attach() -> MemoryResult<Self> {
        let base = unsafe { GetModuleHandleW(std::ptr::null()) } as usize;
        if base == 0 {
            return Err(MemoryError::AttachFailed(
                "failed to locate the current process image",
            ));
        }
        // SizeOfImage is 56 bytes into the optional header in PE32 and PE32+.
        let e_lfanew = unsafe { std::ptr::read_unaligned((base + 0x3C) as *const u32) } as usize;
        let optional_header = base + e_lfanew + 24;
        let size = unsafe { std::ptr::read_unaligned((optional_header + 56) as *const u32) } as usize;
        if size == 0 {
            return Err(MemoryError::AttachFailed(
                "failed to parse the current process image headers",
            ));
        }
        Ok(Self {
            module: ProcessModule { base_address: base, size },
        })
    }

    #[cfg(not(windows))]
    pub fn attach() -> MemoryResult<Self> {
        Err(MemoryError::UnsupportedPlatform)
    }

    /// Reader over a module image the caller already knows.
    pub fn with_module(module: ProcessModule) -> Self {
        Self { module }
    }
}

thread_local! {
    static GAME_THREAD_READS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Marks the current thread as the game thread between two loop steps for
/// as long as it lives, so [`InProcessMemory`] copies directly there.
pub struct GameThreadReads {
    previous: bool,
}

impl GameThreadReads {
    pub fn enter() -> Self {
        Self { previous: GAME_THREAD_READS.with(|flag| flag.replace(true)) }
    }
}

impl Drop for GameThreadReads {
    fn drop(&mut self) {
        GAME_THREAD_READS.with(|flag| flag.set(self.previous));
    }
}

impl ProcessMemory for InProcessMemory {
    fn main_module(&self) -> ProcessModule {
        self.module
    }

    fn read_into(&self, address: usize, buffer: &mut [u8]) -> MemoryResult<()> {
        let size = buffer.len();
        if address == 0 || address.checked_add(size).is_none() {
            return Err(MemoryError::InvalidRead { size });
        }
        if GAME_THREAD_READS.with(|flag| flag.get()) {
            unsafe { std::ptr::copy_nonoverlapping(address as *const u8, buffer.as_mut_ptr(), size) };
            return Ok(());
        }
        checked_self_read(address, buffer)
    }

    fn is_alive(&self) -> MemoryResult<bool> {
        Ok(true)
    }
}

#[cfg(windows)]
fn checked_self_read(address: usize, buffer: &mut [u8]) -> MemoryResult<()> {
    use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let mut read = 0;
    let ok = unsafe {
        ReadProcessMemory(GetCurrentProcess(), address as *const _, buffer.as_mut_ptr().cast(), buffer.len(), &mut read)
    };
    if ok == 0 || read != buffer.len() {
        return Err(MemoryError::InvalidRead { size: buffer.len() });
    }
    Ok(())
}

#[cfg(not(windows))]
fn checked_self_read(address: usize, buffer: &mut [u8]) -> MemoryResult<()> {
    // Only tests build this off Windows; they read their own live buffers.
    unsafe { std::ptr::copy_nonoverlapping(address as *const u8, buffer.as_mut_ptr(), buffer.len()) };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_does_not_emit_addresses() {
        assert_eq!(
            MemoryError::ReadFailed { size: 4 }.to_string(),
            "process read failed: 4 bytes"
        );
        assert_eq!(
            MemoryError::InvalidRead { size: 8 }.to_string(),
            "invalid process read: 8 bytes"
        );
    }

    #[test]
    fn module_ranges_reject_zero_oversized_and_overflowing_reads() {
        let module = ProcessModule {
            base_address: 100,
            size: 10,
        };

        assert_eq!(module.range_address(0, 0), None);
        assert_eq!(module.range_address(9, 2), None);
        assert_eq!(module.range_address(usize::MAX, 2), None);
        assert_eq!(module.range_address(0, 10), Some(100));
        assert_eq!(module.range_address(9, 1), Some(109));
    }

    #[test]
    fn in_process_reads_copy_from_the_given_address() {
        let reader = InProcessMemory::with_module(ProcessModule {
            base_address: 0x1000,
            size: 0x100,
        });
        let image = [1_u8, 2, 3, 4, 5, 6, 7, 8];

        let mut buffer = [0_u8; 4];
        reader
            .read_into(image.as_ptr() as usize + 2, &mut buffer)
            .expect("own buffer address is readable");
        assert_eq!(buffer, [3, 4, 5, 6]);

        assert_eq!(
            reader.read_into(0, &mut buffer),
            Err(MemoryError::InvalidRead { size: 4 })
        );
        assert_eq!(
            reader.read_into(usize::MAX, &mut [0_u8; 2]),
            Err(MemoryError::InvalidRead { size: 2 })
        );
        assert_eq!(reader.is_alive(), Ok(true));
    }

    #[cfg(windows)]
    #[test]
    fn in_process_attach_reads_the_current_image_header() {
        let reader = InProcessMemory::attach().expect("current image is attached");
        assert!(reader.main_module().base_address > 0);
        assert!(reader.main_module().size > 0);

        let mut dos_header = [0_u8; 2];
        reader
            .read_into(reader.main_module().base_address, &mut dos_header)
            .expect("main module header should be readable");
        assert_eq!(&dos_header, b"MZ");
    }
}
