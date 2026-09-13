// 작업표시줄 버튼에 상태 배지를 올린다.
//
// 본 아이콘은 exe 리소스에 박힌 정적 아이콘으로 고정한다.
// 작업표시줄에 고정하면 Windows 가 그 아이콘만 쓰고 WM_SETICON 을 무시하는데,
// 고정 여부에 따라 다르게 보이는 것보다 항상 같게 두는 편이 낫다.
// 상태는 ITaskbarList3::SetOverlayIcon 으로 얹는 배지로만 표시한다.
//
// 대상은 항상 keenpin 자신의 창이다 (Tier A).

use crate::icon;
use crate::icon::Rgba;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS,
    DeleteObject, HBITMAP,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::UI::Shell::{ITaskbarList3, TaskbarList};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, DestroyIcon, HICON, ICONINFO};
use windows::core::PCWSTR;
const BADGE: u32 = 16;

pub struct Overlay {
    bar: Option<ITaskbarList3>,
    shown: Option<bool>,
    current: Option<HICON>,
}

impl Overlay {
    pub fn new() -> Self {
        let bar = unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            CoCreateInstance::<_, ITaskbarList3>(&TaskbarList, None, CLSCTX_INPROC_SERVER)
                .ok()
                .and_then(|b| b.HrInit().ok().map(|_| b))
        };
        Self {
            bar,
            shown: None,
            current: None,
        }
    }

    /// 값이 바뀔 때만 갱신한다.
    pub fn reflect(&mut self, hwnd: HWND, locked: bool) {
        if self.shown == Some(locked) {
            return;
        }
        let Some(bar) = self.bar.clone() else { return };

        let Some(hicon) = make_icon(locked) else {
            return;
        };
        let tip: Vec<u16> = if locked { "LOCKED\0" } else { "RELEASED\0" }
            .encode_utf16()
            .collect();

        unsafe {
            let _ = bar.SetOverlayIcon(hwnd, hicon, PCWSTR(tip.as_ptr()));
            if let Some(old) = self.current.replace(hicon) {
                let _ = DestroyIcon(old);
            }
        }
        self.shown = Some(locked);
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        if let Some(h) = self.current.take() {
            unsafe {
                let _ = DestroyIcon(h);
            }
        }
    }
}

/// RGBA 픽셀에서 HICON 을 만든다. 32bpp 알파를 그대로 쓰므로 마스크는 비워 둔다.
fn make_icon(locked: bool) -> Option<HICON> {
    icon_from(&icon::dot(locked, BADGE))
}

fn icon_from(img: &Rgba) -> Option<HICON> {
    let head = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: img.w as i32,
            // 음수면 위에서 아래로. 우리 픽셀 순서와 맞는다.
            biHeight: -(img.h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    unsafe {
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let color = CreateDIBSection(None, &head, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        if bits.is_null() {
            let _ = DeleteObject(color.into());
            return None;
        }

        let dst = std::slice::from_raw_parts_mut(bits.cast::<u8>(), (img.w * img.h * 4) as usize);
        for (d, s) in dst.chunks_exact_mut(4).zip(img.px.chunks_exact(4)) {
            d.copy_from_slice(&[s[2], s[1], s[0], s[3]]);
        }

        let mask: HBITMAP = CreateBitmap(img.w as i32, img.w as i32, 1, 1, None);
        let info = ICONINFO {
            fIcon: true.into(),
            hbmColor: color,
            hbmMask: mask,
            ..Default::default()
        };
        let hicon = CreateIconIndirect(&info).ok();

        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        hicon
    }
}
