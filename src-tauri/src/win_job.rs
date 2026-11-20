use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject, TerminateJobObject,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE};

static JOB: OnceLock<usize> = OnceLock::new();

fn create() -> usize {
    unsafe {
        let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if handle.is_null() {
            return 0;
        }
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let set = SetInformationJobObject(
            handle,
            JobObjectExtendedLimitInformation,
            std::ptr::addr_of!(limits).cast(),
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        if set == 0 {
            CloseHandle(handle);
            return 0;
        }
        handle as usize
    }
}

fn job() -> Option<HANDLE> {
    let raw = *JOB.get_or_init(create);
    (raw != 0).then_some(raw as HANDLE)
}

pub fn hold_pid(pid: u32) -> bool {
    let Some(handle) = job() else {
        return false;
    };
    unsafe {
        let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
        if process.is_null() {
            return false;
        }
        let held = AssignProcessToJobObject(handle, process) != 0;
        CloseHandle(process);
        held
    }
}

pub fn end() -> bool {
    match JOB.get().copied() {
        Some(raw) if raw != 0 => unsafe { TerminateJobObject(raw as HANDLE, 1) != 0 },
        _ => false,
    }
}
