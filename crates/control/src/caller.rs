//! Who is on the other end of a connection, as the system reports it: the process that connected, the agent that
//! started it, and the app that agent runs in, with that app's signature (macOS and Windows). None of it comes from
//! what the other side says about itself, so the take card can name the caller truthfully.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Caller {
    /// The process that connected.
    pub pid: u32,
    /// Its program's file name, e.g. `shotmaxxing`.
    pub program: String,
    /// Where that program is.
    pub path: String,
    /// The program that started it, e.g. `claude` or `node`: the agent, when the caller is the shotmaxxing command.
    pub agent: Option<String>,
    /// The app all of this runs in, e.g. "Terminal" or "Visual Studio Code".
    pub app: Option<String>,
    /// Who signed that app, when its signature checks out and Apple vouches for the signer: "Apple", "the Mac App
    /// Store", or the name on the developer's Developer ID.
    pub signer: Option<String>,
    /// Whether the app's signature was checked at all. It isn't on Linux, nor on Windows for an app from a package
    /// (the Store, MSIX) whose program isn't signed itself: then a missing signer says nothing about the app.
    pub checked: bool,
}

impl Caller {
    /// Whether the caller is the `shotmaxxing` command (any copy of it), rather than some other program.
    pub fn is_command(&self) -> bool {
        let name = self.program.to_ascii_lowercase();
        name.strip_suffix(".exe").unwrap_or(&name) == "shotmaxxing"
    }
}

/// What an app's signature says.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, PartialEq, Eq)]
enum Signature {
    /// It checks out, and the system vouches for who signed it.
    By(String),
    /// Checked: it's unsigned, doesn't check out, or nobody the system vouches for signed it.
    Nobody,
    /// Not checked.
    Unknown,
}

/// A running process: its program and the process that started it.
struct Process {
    path: String,
    parent: Option<u32>,
}

/// Names the process `pid` and walks up its parents to the app it runs in. `None` when it has already gone.
pub fn identify(pid: u32) -> Option<Caller> {
    let first = imp::process(pid)?;
    let mut caller =
        Caller { pid, program: file_name(&first.path), path: first.path.clone(), ..Caller::default() };
    let mut ancestors: Vec<(u32, String)> = Vec::new();
    let (mut child, mut next) = (pid, first.parent);
    while let Some(parent) = next.filter(|&p| p > 1 && ancestors.len() < 32) {
        // A number the real parent left behind may belong to another process by now.
        if !imp::is_parent(parent, child) {
            break;
        }
        let Some(process) = imp::process(parent) else { break };
        ancestors.push((parent, process.path));
        (child, next) = (parent, process.parent);
    }
    caller.agent = ancestors.first().map(|(_, path)| file_name(path));
    if let Some((app, app_pid)) = imp::host_app(&ancestors) {
        match imp::signature(app_pid) {
            Signature::By(signer) => (caller.checked, caller.signer) = (true, Some(signer)),
            Signature::Nobody => caller.checked = true,
            Signature::Unknown => {}
        }
        caller.app = Some(app);
    }
    Some(caller)
}

/// The process on the other end of a local socket. Only macOS says, of the systems the app runs on that use sockets.
#[cfg(unix)]
pub(crate) fn peer_pid(stream: &std::os::unix::net::UnixStream) -> Option<u32> {
    #[cfg(target_os = "macos")]
    return imp::peer_pid(stream);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = stream;
        None
    }
}

/// A path's last part: `/usr/local/bin/claude` → `claude`.
fn file_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod imp {
    use std::ffi::c_void;
    use std::os::fd::AsRawFd;
    use std::os::unix::net::UnixStream;

    use core_foundation::array::CFArray;
    use core_foundation::base::{CFType, CFTypeRef, TCFType};
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::number::CFNumber;
    use core_foundation::string::{CFString, CFStringRef};

    use super::{Process, Signature};

    pub(super) fn peer_pid(stream: &UnixStream) -> Option<u32> {
        let mut pid: libc::pid_t = 0;
        let mut len = std::mem::size_of::<libc::pid_t>() as libc::socklen_t;
        // SAFETY: asks about a socket this process holds open; the answer is one pid, written into `pid`.
        let ok = unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                libc::SOL_LOCAL,
                libc::LOCAL_PEERPID,
                (&raw mut pid).cast(),
                &mut len,
            )
        };
        (ok == 0 && pid > 0).then_some(pid as u32)
    }

    /// `PROC_PIDUNIQIDENTIFIERINFO`: ids the system never reuses, for a process and for the one that started it.
    const UNIQUE_IDS: libc::c_int = 17;

    /// `struct proc_uniqidentifierinfo` from `<sys/proc_info.h>`.
    #[repr(C)]
    struct UniqueIds {
        uuid: [u8; 16],
        id: u64,
        parent: u64,
        version: i32,
        reserved: u32,
        more: [u64; 2],
    }

    /// One `proc_pidinfo` answer. The two flavors used here (short BSD info and unique ids) describe any user's
    /// process; the full BSD info would refuse a root process such as `login`, which sits between a terminal and its
    /// shell.
    fn info<T>(pid: u32, flavor: libc::c_int) -> Option<T> {
        // SAFETY: only called with plain C structs, for which all zeros is a valid value to fill.
        let mut out: T = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<T>() as i32;
        // SAFETY: the buffer is one T, and `size` says so.
        let read = unsafe { libc::proc_pidinfo(pid as i32, flavor, 0, (&raw mut out).cast(), size) };
        (read == size).then_some(out)
    }

    pub(super) fn process(pid: u32) -> Option<Process> {
        let short: libc::proc_bsdshortinfo = info(pid, libc::PROC_PIDT_SHORTBSDINFO)?;
        let mut path = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        // SAFETY: the buffer holds PROC_PIDPATHINFO_MAXSIZE bytes, as proc_pidpath expects.
        let len = unsafe { libc::proc_pidpath(pid as i32, path.as_mut_ptr().cast(), path.len() as u32) };
        if len <= 0 {
            return None;
        }
        path.truncate(len as usize);
        Some(Process { path: String::from_utf8_lossy(&path).into_owned(), parent: Some(short.pbsi_ppid) })
    }

    pub(super) fn is_parent(parent: u32, child: u32) -> bool {
        match (info::<UniqueIds>(parent, UNIQUE_IDS), info::<UniqueIds>(child, UNIQUE_IDS)) {
            (Some(p), Some(c)) => c.parent == p.id,
            _ => false,
        }
    }

    /// The outermost app bundle around the nearest ancestor that runs in one: `Visual Studio Code`, not the
    /// `Code Helper (Plugin)` inside it. With that ancestor, whose signature is checked.
    pub(super) fn host_app(ancestors: &[(u32, String)]) -> Option<(String, u32)> {
        ancestors.iter().find_map(|(pid, path)| {
            let end = path.find(".app/")?;
            let name = path[..end].rsplit('/').next().filter(|n| !n.is_empty())?;
            Some((name.to_string(), *pid))
        })
    }

    #[link(name = "Security", kind = "framework")]
    unsafe extern "C" {
        static kSecGuestAttributePid: CFStringRef;
        static kSecCodeInfoCertificates: CFStringRef;
        static kSecCodeInfoTeamIdentifier: CFStringRef;
        fn SecCodeCopyGuestWithAttributes(
            host: CFTypeRef,
            attributes: CFDictionaryRef,
            flags: u32,
            guest: *mut CFTypeRef,
        ) -> i32;
        fn SecCodeCheckValidity(code: CFTypeRef, flags: u32, requirement: CFTypeRef) -> i32;
        fn SecRequirementCreateWithString(text: CFStringRef, flags: u32, requirement: *mut CFTypeRef) -> i32;
        fn SecCodeCopySigningInformation(
            code: CFTypeRef,
            flags: u32,
            information: *mut CFDictionaryRef,
        ) -> i32;
        fn SecCertificateCopySubjectSummary(certificate: *const c_void) -> CFStringRef;
    }

    /// `kSecCSSigningInformation`: include the certificates and the team.
    const SIGNING_INFORMATION: u32 = 1 << 1;

    /// Signed by Apple for the Mac App Store.
    pub(super) const MAC_APP_STORE: &str =
        "anchor apple generic and certificate leaf[field.1.2.840.113635.100.6.1.9] exists";
    /// Signed with a Developer ID, which Apple issues to developers it has identified.
    pub(super) const DEVELOPER_ID: &str = "anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists \
                                and certificate leaf[field.1.2.840.113635.100.6.1.13] exists";

    pub(super) fn signature(pid: u32) -> Signature {
        signer(pid).map_or(Signature::Nobody, Signature::By)
    }

    /// Who signed the running process `pid`, if its signature checks out and Apple vouches for the signer: "Apple"
    /// for Apple's own code, "the Mac App Store" for apps from it, otherwise the name on the developer's Developer
    /// ID. `None` when it's unsigned, signed ad hoc, signed by anyone else, or doesn't check out.
    pub(super) fn signer(pid: u32) -> Option<String> {
        // SAFETY: constants the Security framework defines, valid for the life of the process.
        let (pid_key, certificates_key, team_key) = unsafe {
            (
                CFString::wrap_under_get_rule(kSecGuestAttributePid),
                CFString::wrap_under_get_rule(kSecCodeInfoCertificates),
                CFString::wrap_under_get_rule(kSecCodeInfoTeamIdentifier),
            )
        };
        let attributes = CFDictionary::from_CFType_pairs(&[(pid_key, CFNumber::from(pid as i32))]);
        let mut code: CFTypeRef = std::ptr::null();
        // SAFETY: asks the system for the running code of `pid`; the result is ours to release.
        let status = unsafe {
            SecCodeCopyGuestWithAttributes(std::ptr::null(), attributes.as_concrete_TypeRef(), 0, &mut code)
        };
        if status != 0 || code.is_null() {
            return None;
        }
        // SAFETY: a new reference from a Copy function, released when this drops.
        let code = unsafe { CFType::wrap_under_create_rule(code) };
        // SAFETY: checks the signature of code we hold, against no extra requirement.
        if unsafe { SecCodeCheckValidity(code.as_CFTypeRef(), 0, std::ptr::null()) } != 0 {
            return None;
        }
        if satisfies(&code, "anchor apple") {
            return Some("Apple".into());
        }
        if satisfies(&code, MAC_APP_STORE) {
            return Some("the Mac App Store".into());
        }
        // The check above lets the certificates end at any certificate that signs itself, which anyone can make in
        // any name, and Apple gives development certificates to any Apple ID, in the name it was given. Only a
        // Developer ID names a developer Apple has identified.
        if !satisfies(&code, DEVELOPER_ID) {
            return None;
        }
        let mut info: CFDictionaryRef = std::ptr::null();
        // SAFETY: reads the signing information of code we hold; the dictionary is ours to release.
        let status =
            unsafe { SecCodeCopySigningInformation(code.as_CFTypeRef(), SIGNING_INFORMATION, &mut info) };
        if status != 0 || info.is_null() {
            return None;
        }
        // SAFETY: a new dictionary from a Copy function, released when this drops.
        let info: CFDictionary<CFString, CFType> = unsafe { CFDictionary::wrap_under_create_rule(info) };
        let named = info.find(&certificates_key).and_then(|value| {
            let certificates = value.downcast::<CFArray>()?;
            let leaf: *const c_void = *certificates.get(0)?;
            // SAFETY: the first certificate of a code signature is a SecCertificate; the summary is ours to release.
            let summary = unsafe { SecCertificateCopySubjectSummary(leaf) };
            // SAFETY: a new string from a Copy function, released when this drops.
            (!summary.is_null()).then(|| unsafe { CFString::wrap_under_create_rule(summary) }.to_string())
        });
        named.map(|summary| super::developer(&summary)).or_else(|| {
            let team = info.find(&team_key)?.downcast::<CFString>()?;
            Some(format!("team {team}"))
        })
    }

    /// A requirement compiled from its text, when the text is one.
    pub(super) fn requirement(text: &str) -> Option<CFType> {
        let text = CFString::new(text);
        let mut compiled: CFTypeRef = std::ptr::null();
        // SAFETY: compiles a requirement from text; the result is ours to release.
        let status = unsafe { SecRequirementCreateWithString(text.as_concrete_TypeRef(), 0, &mut compiled) };
        // SAFETY: a new reference from a Create function, released when this drops.
        (status == 0 && !compiled.is_null()).then(|| unsafe { CFType::wrap_under_create_rule(compiled) })
    }

    fn satisfies(code: &CFType, text: &str) -> bool {
        let Some(compiled) = requirement(text) else { return false };
        // SAFETY: checks code we hold against a requirement we hold.
        unsafe { SecCodeCheckValidity(code.as_CFTypeRef(), 0, compiled.as_CFTypeRef()) == 0 }
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod imp {
    use std::collections::HashMap;
    use std::iter::once;
    use std::mem::size_of;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};

    use windows_sys::Win32::Foundation::{
        APPMODEL_ERROR_NO_PACKAGE, FILETIME, GENERIC_READ, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Security::Cryptography::Catalog::{
        CATALOG_INFO, CryptCATAdminAcquireContext2, CryptCATAdminCalcHashFromFileHandle2,
        CryptCATAdminEnumCatalogFromHash, CryptCATAdminReleaseCatalogContext, CryptCATAdminReleaseContext,
        CryptCATCatalogInfoFromContext,
    };
    use windows_sys::Win32::Security::Cryptography::{
        BCRYPT_SHA256_ALGORITHM, CERT_NAME_SIMPLE_DISPLAY_TYPE, CertGetNameStringW,
    };
    use windows_sys::Win32::Security::WinTrust::{
        DRIVER_ACTION_VERIFY, WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_CATALOG_INFO, WINTRUST_DATA,
        WINTRUST_FILE_INFO, WTD_CACHE_ONLY_URL_RETRIEVAL, WTD_CHOICE_CATALOG, WTD_CHOICE_FILE,
        WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
        WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData, WinVerifyTrust,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, GetFileVersionInfoSizeW, GetFileVersionInfoW,
        OPEN_EXISTING, VerQueryValueW,
    };
    use windows_sys::Win32::Storage::Packaging::Appx::GetPackageFullName;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };

    use super::{Process, Signature};

    /// Every running process's parent and program name, from one snapshot.
    fn snapshot() -> HashMap<u32, (u32, String)> {
        let mut out = HashMap::new();
        // SAFETY: takes a snapshot of the process list; the handle is closed when it drops.
        let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if raw.is_null() || raw as isize == -1 {
            return out;
        }
        // SAFETY: a handle just made, owned here.
        let handle = unsafe { OwnedHandle::from_raw_handle(raw as RawHandle) };
        // SAFETY: PROCESSENTRY32W is plain data, so all zeros is a valid value to fill.
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let raw = handle.as_raw_handle() as HANDLE;
        // SAFETY: walks the snapshot with an entry whose size is set, as the API asks.
        let mut more = unsafe { Process32FirstW(raw, &mut entry) } != 0;
        while more {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            out.insert(entry.th32ProcessID, (entry.th32ParentProcessID, name));
            // SAFETY: as above.
            more = unsafe { Process32NextW(raw, &mut entry) } != 0;
        }
        out
    }

    fn open(pid: u32) -> Option<OwnedHandle> {
        // SAFETY: opens the process with the least access that can read its path and times.
        let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        // SAFETY: a handle just opened, owned here.
        (!raw.is_null()).then(|| unsafe { OwnedHandle::from_raw_handle(raw as RawHandle) })
    }

    fn full_path(pid: u32) -> Option<String> {
        let process = open(pid)?;
        let mut buffer = vec![0u16; 32_768];
        let mut len = buffer.len() as u32;
        // SAFETY: the buffer holds `len` characters.
        let ok = unsafe {
            QueryFullProcessImageNameW(
                process.as_raw_handle() as HANDLE,
                PROCESS_NAME_WIN32,
                buffer.as_mut_ptr(),
                &mut len,
            )
        };
        (ok != 0).then(|| String::from_utf16_lossy(&buffer[..len as usize]))
    }

    pub(super) fn process(pid: u32) -> Option<Process> {
        let processes = snapshot();
        let (parent, name) = processes.get(&pid)?.clone();
        Some(Process { path: full_path(pid).unwrap_or(name), parent: Some(parent) })
    }

    fn started(pid: u32) -> Option<u64> {
        let process = open(pid)?;
        let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
        // SAFETY: reads the times of a process handle held here.
        let ok = unsafe {
            GetProcessTimes(
                process.as_raw_handle() as HANDLE,
                &mut created,
                &mut exited,
                &mut kernel,
                &mut user,
            )
        };
        (ok != 0).then(|| (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
    }

    pub(super) fn is_parent(parent: u32, child: u32) -> bool {
        // A parent that started after its child is a reused number.
        matches!((started(parent), started(child)), (Some(p), Some(c)) if p <= c)
    }

    /// Programs that only carry an agent: shells, consoles and runtimes. The app is the first ancestor that isn't one.
    const CARRIERS: [&str; 22] = [
        "cmd.exe",
        "powershell.exe",
        "pwsh.exe",
        "bash.exe",
        "sh.exe",
        "zsh.exe",
        "fish.exe",
        "wsl.exe",
        "wslhost.exe",
        "conhost.exe",
        "openconsole.exe",
        "node.exe",
        "bun.exe",
        "deno.exe",
        "python.exe",
        "pythonw.exe",
        "py.exe",
        "uv.exe",
        "uvx.exe",
        "npx.exe",
        "npm.exe",
        "git-bash.exe",
    ];

    pub(super) fn host_app(ancestors: &[(u32, String)]) -> Option<(String, u32)> {
        // The agent itself (the first ancestor) isn't its own app.
        for (pid, path) in ancestors.iter().skip(1) {
            let name = super::file_name(path).to_ascii_lowercase();
            if name == "explorer.exe" || name == "svchost.exe" || name == "services.exe" {
                return None;
            }
            if CARRIERS.contains(&name.as_str()) {
                continue;
            }
            let shown = description(path).unwrap_or_else(|| {
                super::file_name(path).trim_end_matches(".exe").trim_end_matches(".EXE").to_string()
            });
            return Some((shown, *pid));
        }
        None
    }

    /// The program's own description, e.g. "Visual Studio Code", from its version information.
    fn description(path: &str) -> Option<String> {
        let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: a NUL-terminated path; asks for the size only.
        let size = unsafe { GetFileVersionInfoSizeW(wide.as_ptr(), std::ptr::null_mut()) };
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        // SAFETY: the buffer holds `size` bytes.
        if unsafe { GetFileVersionInfoW(wide.as_ptr(), 0, size, data.as_mut_ptr().cast()) } == 0 {
            return None;
        }
        let query = |key: &str| -> Option<(*const u16, u32)> {
            let key: Vec<u16> = key.encode_utf16().chain(std::iter::once(0)).collect();
            let mut value: *mut core::ffi::c_void = std::ptr::null_mut();
            let mut len = 0u32;
            // SAFETY: reads from the version block just loaded; the pointer points into `data`.
            let ok = unsafe { VerQueryValueW(data.as_ptr().cast(), key.as_ptr(), &mut value, &mut len) };
            (ok != 0 && !value.is_null() && len > 0).then_some((value as *const u16, len))
        };
        let (translation, len) = query(r"\VarFileInfo\Translation")?;
        if len < 4 {
            return None;
        }
        // SAFETY: the translation table starts with a language and a code page, two 16-bit values.
        let (language, codepage) = unsafe { (*translation, *translation.add(1)) };
        let (text, len) = query(&format!(r"\StringFileInfo\{language:04x}{codepage:04x}\FileDescription"))?;
        // SAFETY: `len` characters of the string sit at `text`, inside `data`.
        let chars = unsafe { std::slice::from_raw_parts(text, len as usize) };
        let end = chars.iter().position(|&c| c == 0).unwrap_or(chars.len());
        let description = String::from_utf16_lossy(&chars[..end]).trim().to_string();
        (!description.is_empty()).then_some(description)
    }

    /// What the running process `pid`'s program's signature says: its own (Authenticode), or else one of the
    /// system's catalogs, which sign most of Windows's own programs. Only a signature that checks out to a root
    /// Windows trusts names a signer.
    pub(super) fn signature(pid: u32) -> Signature {
        // A program that can't be read can't be vouched for (it may have quit as soon as it started the agent).
        let Some(path) = full_path(pid) else { return Signature::Nobody };
        match signer_of(&path) {
            Some(signer) => Signature::By(signer),
            // A package's own signature vouches for the programs in it, and isn't checked here: such an app gets
            // neither the shield nor the warning.
            None if packaged(pid) => Signature::Unknown,
            None => Signature::Nobody,
        }
    }

    pub(super) fn signer_of(path: &str) -> Option<String> {
        let path = wide(path);
        file_signer(&path).or_else(|| catalog_signer(&path))
    }

    /// A NUL-terminated wide string.
    pub(super) fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(once(0)).collect()
    }

    /// A check WinVerifyTrust is to make: no windows, and no waiting on the network for revocation lists (macOS
    /// doesn't either, by default).
    fn trust_data(choice: u32) -> WINTRUST_DATA {
        WINTRUST_DATA {
            cbStruct: size_of::<WINTRUST_DATA>() as u32,
            dwUIChoice: WTD_UI_NONE,
            fdwRevocationChecks: WTD_REVOKE_NONE,
            dwUnionChoice: choice,
            dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL,
            ..Default::default()
        }
    }

    /// Makes the check, and names who signed when it checks out.
    fn verify(data: &mut WINTRUST_DATA) -> Option<String> {
        let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
        data.dwStateAction = WTD_STATEACTION_VERIFY;
        // SAFETY: `data` describes one file or catalog member, with the structures it points to alive for the call.
        let status = unsafe { WinVerifyTrust(std::ptr::null_mut(), &mut action, (&raw mut *data).cast()) };
        let signer = if status == 0 { signer_name(data.hWVTStateData) } else { None };
        data.dwStateAction = WTD_STATEACTION_CLOSE;
        // SAFETY: frees the state the check above kept, whatever it found.
        unsafe { WinVerifyTrust(std::ptr::null_mut(), &mut action, (&raw mut *data).cast()) };
        signer
    }

    /// The name on the certificate that made the signature a check kept: e.g. "Microsoft Corporation".
    fn signer_name(state: HANDLE) -> Option<String> {
        // SAFETY: reads the state a successful check kept, before it's freed; the helpers point into it.
        let signer = unsafe {
            let provider = WTHelperProvDataFromStateData(state);
            if provider.is_null() {
                return None;
            }
            WTHelperGetProvSignerFromChain(provider, 0, 0, 0)
        };
        // SAFETY: a signer the state holds, if any; its chain starts with the signing certificate.
        let leaf = unsafe {
            let signer = signer.as_ref()?;
            if signer.csCertChain == 0 {
                return None;
            }
            signer.pasCertChain.as_ref()?.pCert
        };
        if leaf.is_null() {
            return None;
        }
        let mut name = [0u16; 256];
        // SAFETY: the buffer holds as many characters as said.
        let len = unsafe {
            CertGetNameStringW(
                leaf,
                CERT_NAME_SIMPLE_DISPLAY_TYPE,
                0,
                std::ptr::null(),
                name.as_mut_ptr(),
                name.len() as u32,
            )
        } as usize;
        // The length counts the closing NUL.
        let name =
            String::from_utf16_lossy(&name[..len.saturating_sub(1).min(name.len())]).trim().to_string();
        (!name.is_empty()).then_some(name)
    }

    /// Who signed the program at `path` (NUL-terminated) itself.
    fn file_signer(path: &[u16]) -> Option<String> {
        let mut file = WINTRUST_FILE_INFO {
            cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
            pcwszFilePath: path.as_ptr(),
            ..Default::default()
        };
        let mut data = trust_data(WTD_CHOICE_FILE);
        data.Anonymous.pFile = &mut file;
        verify(&mut data)
    }

    /// Who signed the system catalog that lists the program at `path` (NUL-terminated), if one does.
    pub(super) fn catalog_signer(path: &[u16]) -> Option<String> {
        // SAFETY: opens the program to read it, leaving it open to everyone else; closed when the handle drops.
        let raw = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };
        if raw.is_null() || raw == INVALID_HANDLE_VALUE {
            return None;
        }
        // SAFETY: a handle just opened, owned here.
        let file = unsafe { OwnedHandle::from_raw_handle(raw as RawHandle) };
        // Newer catalogs list programs by their SHA-256 hash, older ones by SHA-1 (the default).
        [BCRYPT_SHA256_ALGORITHM, std::ptr::null()]
            .into_iter()
            .find_map(|algorithm| catalog_member(path, &file, algorithm))
    }

    fn catalog_member(path: &[u16], file: &OwnedHandle, algorithm: *const u16) -> Option<String> {
        let mut admin: isize = 0;
        // SAFETY: opens the system's catalogs for one hash algorithm; released below.
        if unsafe {
            CryptCATAdminAcquireContext2(&mut admin, &DRIVER_ACTION_VERIFY, algorithm, std::ptr::null(), 0)
        } == 0
        {
            return None;
        }
        let handle = file.as_raw_handle() as HANDLE;
        let found = (|| {
            let mut size = 0u32;
            // SAFETY: asks for the size of the program's hash first...
            unsafe {
                CryptCATAdminCalcHashFromFileHandle2(admin, handle, &mut size, std::ptr::null_mut(), 0)
            };
            if size == 0 {
                return None;
            }
            let mut hash = vec![0u8; size as usize];
            // SAFETY: ...then the hash, into a buffer of that size.
            if unsafe { CryptCATAdminCalcHashFromFileHandle2(admin, handle, &mut size, hash.as_mut_ptr(), 0) }
                == 0
            {
                return None;
            }
            // SAFETY: looks the hash up among the catalogs; the context found is released below.
            let catalog = unsafe {
                CryptCATAdminEnumCatalogFromHash(admin, hash.as_ptr(), size, 0, std::ptr::null_mut())
            };
            if catalog == 0 {
                return None;
            }
            let mut info = CATALOG_INFO { cbStruct: size_of::<CATALOG_INFO>() as u32, ..Default::default() };
            // SAFETY: reads the found catalog's path into `info`.
            let signer = if unsafe { CryptCATCatalogInfoFromContext(catalog, &mut info, 0) } != 0 {
                // A catalog names its members by their hash, in capitals.
                let tag: String = hash.iter().map(|byte| format!("{byte:02X}")).collect();
                let tag = wide(&tag);
                let mut member = WINTRUST_CATALOG_INFO {
                    cbStruct: size_of::<WINTRUST_CATALOG_INFO>() as u32,
                    pcwszCatalogFilePath: info.wszCatalogFile.as_ptr(),
                    pcwszMemberTag: tag.as_ptr(),
                    pcwszMemberFilePath: path.as_ptr(),
                    hMemberFile: handle,
                    pbCalculatedFileHash: hash.as_mut_ptr(),
                    cbCalculatedFileHash: size,
                    hCatAdmin: admin,
                    ..Default::default()
                };
                let mut data = trust_data(WTD_CHOICE_CATALOG);
                data.Anonymous.pCatalog = &mut member;
                verify(&mut data)
            } else {
                None
            };
            // SAFETY: releases the catalog context found above.
            unsafe { CryptCATAdminReleaseCatalogContext(admin, catalog, 0) };
            signer
        })();
        // SAFETY: releases the catalogs opened above.
        unsafe { CryptCATAdminReleaseContext(admin, 0) };
        found
    }

    /// Whether the process runs from a package: an app from the Store, or one installed with MSIX.
    fn packaged(pid: u32) -> bool {
        let Some(process) = open(pid) else { return false };
        let mut len = 0u32;
        // SAFETY: asks only how long the package's name is; a process outside any package says it has none.
        let status =
            unsafe { GetPackageFullName(process.as_raw_handle() as HANDLE, &mut len, std::ptr::null_mut()) };
        status != APPMODEL_ERROR_NO_PACKAGE
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::Process;

    fn stat(pid: u32) -> Option<(u32, u64)> {
        let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // The program's name may hold spaces or brackets; the fields after its closing bracket are plain.
        let rest = &text[text.rfind(')')? + 2..];
        let fields: Vec<&str> = rest.split_whitespace().collect();
        Some((fields.get(1)?.parse().ok()?, fields.get(19)?.parse().ok()?))
    }

    pub(super) fn process(pid: u32) -> Option<Process> {
        let path = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?;
        Some(Process { path: path.to_string_lossy().into_owned(), parent: stat(pid).map(|(ppid, _)| ppid) })
    }

    pub(super) fn is_parent(parent: u32, child: u32) -> bool {
        matches!((stat(parent), stat(child)), (Some((_, p)), Some((_, c))) if p <= c)
    }

    pub(super) fn host_app(_ancestors: &[(u32, String)]) -> Option<(String, u32)> {
        None
    }

    pub(super) fn signature(_pid: u32) -> super::Signature {
        super::Signature::Unknown
    }
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
mod imp {
    use super::Process;

    pub(super) fn process(_pid: u32) -> Option<Process> {
        None
    }

    pub(super) fn is_parent(_parent: u32, _child: u32) -> bool {
        false
    }

    pub(super) fn host_app(_ancestors: &[(u32, String)]) -> Option<(String, u32)> {
        None
    }

    pub(super) fn signature(_pid: u32) -> super::Signature {
        super::Signature::Unknown
    }
}

/// The developer's name from a Developer ID certificate's summary: `Developer ID Application: Anthropic, PBC
/// (Q6L2SF6YDW)` → `Anthropic, PBC`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn developer(summary: &str) -> String {
    let name = summary.strip_prefix("Developer ID Application: ").unwrap_or(summary).trim();
    let name = match name.rfind(" (") {
        Some(at)
            if name.ends_with(')')
                && name[at + 2..name.len() - 1].chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            &name[..at]
        }
        _ => name,
    };
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_is_named_by_the_system() {
        let me = identify(std::process::id()).expect("this process is running");
        assert_eq!(me.pid, std::process::id());
        let exe = std::env::current_exe().unwrap();
        assert_eq!(me.program, exe.file_name().unwrap().to_string_lossy());
        assert!(!me.is_command(), "a test isn't the shotmaxxing command");
        assert!(me.agent.is_some(), "something started this test");
    }

    #[test]
    fn a_process_that_has_gone_is_nobody() {
        assert_eq!(identify(u32::MAX - 7), None);
    }

    #[test]
    fn developers_are_named_without_certificate_words() {
        assert_eq!(developer("Developer ID Application: Anthropic, PBC (Q6L2SF6YDW)"), "Anthropic, PBC");
        assert_eq!(
            developer("Developer ID Application: Acme (Europe) Ltd (AB12CD34EF)"),
            "Acme (Europe) Ltd"
        );
        assert_eq!(developer("Microsoft Corporation"), "Microsoft Corporation");
        assert_eq!(developer("Acme (Europe) Ltd"), "Acme (Europe) Ltd");
    }

    #[cfg(windows)]
    #[test]
    fn windows_programs_are_named_by_who_signed_them() {
        let system = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let program = |name: &str| format!(r"{system}\System32\{name}");
        // Windows's own programs are signed by Microsoft, most of them in the system's catalogs, not by themselves.
        let signer = imp::signer_of(&program("cmd.exe"));
        assert!(signer.as_deref().is_some_and(|s| s.starts_with("Microsoft")), "cmd.exe: {signer:?}");
        let listed = ["cmd.exe", "notepad.exe", "conhost.exe", "ntdll.dll", "kernel32.dll"]
            .into_iter()
            .filter_map(|name| imp::catalog_signer(&imp::wide(&program(name))))
            .inspect(|signer| assert!(signer.starts_with("Microsoft"), "catalog signer: {signer}"))
            .count();
        assert!(listed > 0, "none of Windows's programs was found in its catalogs");
        // This test isn't signed, by itself or in any catalog, and doesn't run from a package.
        assert_eq!(imp::signature(std::process::id()), Signature::Nobody);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn only_signers_apple_vouches_for_are_named() {
        // Apple's own program, running (spawning returns once it is).
        let mut sleep = std::process::Command::new("/bin/sleep").arg("5").spawn().expect("sleep runs");
        let apple = imp::signer(sleep.id());
        let _ = sleep.kill();
        let _ = sleep.wait();
        assert_eq!(apple.as_deref(), Some("Apple"));
        // This test, signed ad hoc by the linker (or not at all): nobody vouches for it.
        assert_eq!(imp::signer(std::process::id()), None);
        // A requirement that doesn't compile would leave every app from the App Store, or with a Developer ID, unnamed.
        assert!(imp::requirement(imp::MAC_APP_STORE).is_some());
        assert!(imp::requirement(imp::DEVELOPER_ID).is_some());
        assert!(imp::requirement("anchor apple and nonsense[").is_none());
    }

    #[test]
    fn the_command_is_recognised_by_its_program() {
        let named = |program: &str| Caller { program: program.into(), ..Caller::default() };
        assert!(named("shotmaxxing").is_command());
        assert!(named("shotmaxxing.exe").is_command());
        assert!(!named("shotmaxxing-desktop").is_command());
        assert!(!named("evil").is_command());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn apps_are_their_outermost_bundle() {
        let ancestors = [
            (10, "/usr/local/bin/claude".to_string()),
            (
                20,
                "/Applications/Visual Studio Code.app/Contents/Frameworks/Code Helper (Plugin).app/Contents/MacOS/Code Helper (Plugin)".to_string(),
            ),
            (30, "/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal".to_string()),
        ];
        assert_eq!(imp::host_app(&ancestors), Some(("Visual Studio Code".to_string(), 20)));
        assert_eq!(imp::host_app(&ancestors[..1]), None);
    }
}
