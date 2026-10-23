//! A drop target for drag-out probes (Windows): a plain window registered
//! with OLE (`RegisterDragDrop`) that accepts any copy and prints what a
//! drop carried (`DROPPED text=… url=… files=…`, one line each), so a probe
//! drags out of zgui onto it rather than onto anything of the user's. It
//! prints `TARGET x y width height` (its screen rectangle) once it shows,
//! and `ENTERED` / `LEFT` as a drag crosses it. Like Explorer and browsers,
//! it shows the drag's image over itself (`IDropTargetHelper`).
#[cfg(windows)]
fn main() -> windows::core::Result<()> {
    target::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("drop_target_probe: Windows only");
}

#[cfg(windows)]
mod target {
    use std::io::Write;
    use windows::{
        Win32::{
            Foundation::{HWND, LPARAM, LRESULT, POINT, POINTL, RECT, WPARAM},
            Graphics::Gdi::{
                BeginPaint, CreateSolidBrush, EndPaint, FillRect, HBRUSH, PAINTSTRUCT,
            },
            System::{
                Com::{
                    CLSCTX_INPROC_SERVER, CoCreateInstance, DVASPECT_CONTENT, FORMATETC,
                    IDataObject, TYMED_HGLOBAL,
                },
                DataExchange::RegisterClipboardFormatW,
                LibraryLoader::GetModuleHandleW,
                Memory::{GlobalLock, GlobalSize, GlobalUnlock},
                Ole::{
                    CF_HDROP, CF_UNICODETEXT, DROPEFFECT, DROPEFFECT_COPY, IDropTarget,
                    IDropTarget_Impl, OleInitialize, RegisterDragDrop, ReleaseStgMedium,
                },
                SystemServices::MODIFIERKEYS_FLAGS,
            },
            UI::{
                HiDpi::{
                    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
                },
                Shell::{CLSID_DragDropHelper, DragQueryFileW, HDROP, IDropTargetHelper},
                WindowsAndMessaging::{
                    CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW,
                    GetWindowRect, MSG, PostQuitMessage, RegisterClassExW, TranslateMessage,
                    WINDOW_EX_STYLE, WM_DESTROY, WM_PAINT, WNDCLASSEXW, WS_OVERLAPPEDWINDOW,
                    WS_VISIBLE,
                },
            },
        },
        core::{Ref, implement, w},
    };

    #[implement(IDropTarget)]
    struct Target {
        hwnd: HWND,
        /// Shell's helper, showing the drag's image over the window.
        helper: Option<IDropTargetHelper>,
    }
    fn point(at: &POINTL) -> POINT {
        POINT { x: at.x, y: at.y }
    }
    impl IDropTarget_Impl for Target_Impl {
        fn DragEnter(
            &self,
            data: Ref<IDataObject>,
            _: MODIFIERKEYS_FLAGS,
            at: &POINTL,
            effect: *mut DROPEFFECT,
        ) -> windows::core::Result<()> {
            say("ENTERED");
            // SAFETY: OLE's out parameter; the helper with its own data.
            unsafe {
                *effect = DROPEFFECT_COPY;
                if let (Some(helper), Ok(data)) = (&self.helper, data.ok()) {
                    let _ = helper.DragEnter(self.hwnd, data, &point(at), DROPEFFECT_COPY);
                }
            }
            Ok(())
        }
        fn DragOver(
            &self,
            _: MODIFIERKEYS_FLAGS,
            at: &POINTL,
            effect: *mut DROPEFFECT,
        ) -> windows::core::Result<()> {
            // SAFETY: OLE's out parameter; the helper.
            unsafe {
                *effect = DROPEFFECT_COPY;
                if let Some(helper) = &self.helper {
                    let _ = helper.DragOver(&point(at), DROPEFFECT_COPY);
                }
            }
            Ok(())
        }
        fn DragLeave(&self) -> windows::core::Result<()> {
            say("LEFT");
            if let Some(helper) = &self.helper {
                // SAFETY: the helper.
                let _ = unsafe { helper.DragLeave() };
            }
            Ok(())
        }
        fn Drop(
            &self,
            data: Ref<IDataObject>,
            _: MODIFIERKEYS_FLAGS,
            at: &POINTL,
            effect: *mut DROPEFFECT,
        ) -> windows::core::Result<()> {
            let data = data.ok()?;
            if let Some(helper) = &self.helper {
                // SAFETY: the helper, with the data dropped.
                let _ = unsafe { helper.Drop(data, &point(at), DROPEFFECT_COPY) };
            }
            let text = read(data, CF_UNICODETEXT.0).map(|bytes| wide(&bytes));
            // SAFETY: registers (or finds) a format by a static name.
            let url_format = unsafe { RegisterClipboardFormatW(w!("UniformResourceLocatorW")) };
            let url = read(data, url_format as u16).map(|bytes| wide(&bytes));
            let files = files(data);
            say(&format!(
                "DROPPED text={text:?} url={url:?} files={files:?}"
            ));
            // SAFETY: OLE's out parameter.
            unsafe { *effect = DROPEFFECT_COPY };
            Ok(())
        }
    }

    fn say(line: &str) {
        println!("{line}");
        let _ = std::io::stdout().flush();
    }
    fn format(format: u16) -> FORMATETC {
        FORMATETC {
            cfFormat: format,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
        }
    }
    /// The bytes `data` holds as `format`, if any.
    fn read(data: &IDataObject, format_id: u16) -> Option<Vec<u8>> {
        // SAFETY: an HGLOBAL medium, read while locked and then released.
        unsafe {
            let mut medium = data.GetData(&format(format_id)).ok()?;
            let handle = medium.u.hGlobal;
            let at = GlobalLock(handle);
            let bytes = (!at.is_null())
                .then(|| std::slice::from_raw_parts(at.cast::<u8>(), GlobalSize(handle)).to_vec());
            let _ = GlobalUnlock(handle);
            ReleaseStgMedium(&mut medium);
            bytes
        }
    }
    fn wide(bytes: &[u8]) -> String {
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .take_while(|unit| *unit != 0)
            .collect();
        String::from_utf16_lossy(&units)
    }
    fn files(data: &IDataObject) -> Vec<String> {
        // SAFETY: a CF_HDROP medium, its files read and then released.
        unsafe {
            let Ok(mut medium) = data.GetData(&format(CF_HDROP.0)) else {
                return Vec::new();
            };
            let drop = HDROP(medium.u.hGlobal.0);
            let count = DragQueryFileW(drop, u32::MAX, None);
            let files = (0..count)
                .map(|index| {
                    let mut buffer = [0u16; 1024];
                    let n = DragQueryFileW(drop, index, Some(&mut buffer));
                    String::from_utf16_lossy(&buffer[..n as usize])
                })
                .collect();
            ReleaseStgMedium(&mut medium);
            files
        }
    }

    unsafe extern "system" fn proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_PAINT => {
                // SAFETY: paints our own window grey.
                unsafe {
                    let mut paint = PAINTSTRUCT::default();
                    let dc = BeginPaint(hwnd, &mut paint);
                    let brush: HBRUSH =
                        CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x00505050));
                    FillRect(dc, &paint.rcPaint, brush);
                    let _ = EndPaint(hwnd, &paint);
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                // SAFETY: ends this thread's message loop.
                unsafe { PostQuitMessage(0) };
                LRESULT(0)
            }
            // SAFETY: the default procedure, for our own window.
            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        }
    }

    pub fn run() -> windows::core::Result<()> {
        // SAFETY: this process's DPI awareness, OLE and its window, on its
        // one thread.
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            OleInitialize(None)?;
            let instance = GetModuleHandleW(None)?;
            let class = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(proc),
                hInstance: instance.into(),
                lpszClassName: w!("ZguiDropTargetProbe"),
                ..Default::default()
            };
            RegisterClassExW(&class);
            let (x, y) = std::env::args()
                .find_map(|arg| {
                    let (x, y) = arg.strip_prefix("--at=")?.split_once(',')?;
                    Some((x.parse().ok()?, y.parse().ok()?))
                })
                .unwrap_or((CW_USEDEFAULT, CW_USEDEFAULT));
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("ZguiDropTargetProbe"),
                w!("Drop target probe"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                x,
                y,
                360,
                300,
                None,
                None,
                Some(instance.into()),
                None,
            )?;
            let helper = CoCreateInstance(&CLSID_DragDropHelper, None, CLSCTX_INPROC_SERVER).ok();
            let target: IDropTarget = Target { hwnd, helper }.into();
            RegisterDragDrop(hwnd, &target)?;
            let mut rect = RECT::default();
            GetWindowRect(hwnd, &mut rect)?;
            say(&format!(
                "TARGET {} {} {} {}",
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top
            ));
            let mut message = MSG::default();
            while GetMessageW(&mut message, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        Ok(())
    }
}
