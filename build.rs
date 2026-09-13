//! exe 의 .rsrc 에 아이콘을 박는다.
//! ViewportBuilder::with_icon 은 실행 중인 창에만 적용되고,
//! 탐색기·작업표시줄 고정은 exe 리소스를 읽기 때문에 둘 다 필요하다.
//!
//! 아이콘은 src/icon.rs 를 그대로 써서 그린다. 그림이 한 곳에만 있도록.

#[allow(dead_code)]
mod icon {
    include!("src/icon.rs");
}
use icon::{Rgba, badge};

fn main() {
    println!("cargo:rerun-if-changed=src/icon.rs");
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(windows)]
    {
        let dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
        let path = dir.join("keenpin.ico");
        std::fs::write(&path, ico()).expect("ico 쓰기 실패");

        let mut res = winresource::WindowsResource::new();
        res.set_icon(path.to_str().unwrap());
        res.set("FileDescription", "keenpin");
        res.set("ProductName", "keenpin");

        // 관리자 권한으로 실행되지 않으면 UIPI 때문에 더 높은 권한으로 도는 앱의
        // 창에 SetWindowLongPtrW 가 조용히 실패한다. 매니페스트로 고정한다.
        // exe 속성에서 체크하는 방식과 달리 재빌드해도 유지된다.
        // 디버그 빌드에는 넣지 않는다. 매니페스트가 테스트 바이너리에도 붙어서
        // cargo test 가 권한 상승을 요구하게 되기 때문이다.
        if std::env::var("PROFILE").as_deref() == Ok("release") {
            res.set_manifest(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="requireAdministrator" uiAccess="false" />
      </requestedPrivileges>
    </security>
  </trustInfo>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </windowsSettings>
  </application>
</assembly>"#,
        );
        }
        if let Err(e) = res.compile() {
            println!("cargo:warning=아이콘 리소스 임베드 실패: {e}");
        }
    }
}

fn ico() -> Vec<u8> {
    const SIZES: [u32; 6] = [16, 24, 32, 48, 64, 128];

    let images: Vec<(u32, Vec<u8>)> = SIZES.iter().map(|s| (*s, dib(&badge(true, *s)))).collect();

    let mut out = vec![0, 0, 1, 0];
    out.extend_from_slice(&(images.len() as u16).to_le_bytes());

    let mut offset = 6 + 16 * images.len() as u32;
    for (s, data) in &images {
        // 256 은 0 으로 적는다 (ICO 규격).
        let b = if *s >= 256 { 0u8 } else { *s as u8 };
        out.extend_from_slice(&[b, b, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for (_, data) in &images {
        out.extend_from_slice(data);
    }
    out
}

/// 32bpp BITMAPINFOHEADER + 상하 뒤집힌 BGRA + 빈 AND 마스크.
fn dib(img: &Rgba) -> Vec<u8> {
    let (w, h) = (img.w, img.h);
    let mut v = Vec::with_capacity((w * h * 4) as usize + 64);

    v.extend_from_slice(&40u32.to_le_bytes());
    v.extend_from_slice(&(w as i32).to_le_bytes());
    v.extend_from_slice(&((h * 2) as i32).to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&32u16.to_le_bytes());
    v.extend_from_slice(&0u32.to_le_bytes());
    v.extend_from_slice(&0u32.to_le_bytes());
    v.extend_from_slice(&[0u8; 16]);

    for y in (0..h).rev() {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            v.extend_from_slice(&[img.px[i + 2], img.px[i + 1], img.px[i], img.px[i + 3]]);
        }
    }

    let row = w.div_ceil(32) as usize * 4;
    v.resize(v.len() + row * h as usize, 0);
    v
}
