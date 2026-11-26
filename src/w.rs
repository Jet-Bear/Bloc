// Ayudas pequeñas para hablar con Windows.
use std::ffi::OsStr;
use std::io::Write;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, SYSTEMTIME, WPARAM};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyState;
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, SendMessageW, MB_ICONWARNING, MB_OK};

/// Cadena de Windows (UTF-16 terminada en cero).
pub fn ws(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Una ruta de Windows tal cual: con from_utf16_lossy, un nombre con medio carácter suelto (que NTFS admite) sería otro.
pub fn ruta(w: &[u16]) -> std::path::PathBuf {
    std::ffi::OsString::from_wide(w).into()
}

pub fn ws_os(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

pub fn enviar(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe { SendMessageW(h, m, w, l) }
}

/// Una función de una DLL de System32, cargada solo cuando hace falta (para no cargarla al abrir Bloc).
pub fn funcion(dll: &str, nombre: &str) -> Option<usize> {
    let d = ws(dll);
    let n = format!("{nombre}\0");
    unsafe {
        let m = LoadLibraryExW(d.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
        if m.is_null() {
            return None;
        }
        GetProcAddress(m, n.as_ptr()).map(|f| f as usize)
    }
}

pub fn pulsada(vk: u16) -> bool {
    unsafe { GetKeyState(vk as i32) < 0 }
}

pub const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | (g as u32) << 8 | (b as u32) << 16
}

/// Sustituye el primer «{}» de una frase traducida.
pub fn con(frase: &str, valor: impl std::fmt::Display) -> String {
    frase.replacen("{}", &valor.to_string(), 1)
}

pub fn mensaje(padre: HWND, texto: &str, botones: u32) -> i32 {
    let t = ws(texto);
    let c = ws("Bloc");
    unsafe { MessageBoxW(padre, t.as_ptr(), c.as_ptr(), botones) }
}

/// Los fallos inesperados se apuntan en %APPDATA%\Bloc\errores.log para poder arreglarlos. La vista previa del Explorador
/// va con integridad baja y no puede escribir ahí: lo suyo va a `registro_bajo`, y Bloc lo junta con el suyo al abrir.
pub fn apuntar(que: &str) {
    let mut t: SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut t) };
    let linea = format!("{:02}/{:02}/{} {:02}:{:02}:{:02}  {}\r\n\r\n", t.wDay, t.wMonth, t.wYear, t.wHour, t.wMinute, t.wSecond, que);
    let normal = std::env::var_os("APPDATA").map(|d| std::path::Path::new(&d).join(r"Bloc\errores.log"));
    for ruta in [normal, registro_bajo()].into_iter().flatten() {
        if let Some(dir) = ruta.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if std::fs::OpenOptions::new().create(true).append(true).open(&ruta).is_ok_and(|mut f| f.write_all(linea.as_bytes()).is_ok()) {
            return;
        }
    }
}

/// El registro de lo que no puede escribir en %APPDATA%: `LocalLow\Bloc\errores.log`, junto a `Local`.
pub fn registro_bajo() -> Option<std::path::PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(std::path::Path::new(&local).parent()?.join(r"LocalLow\Bloc\errores.log"))
}

static AVISANDO: AtomicBool = AtomicBool::new(false);
/// La vista previa: un aviso encima dejaría al Explorador esperando.
pub static SIN_AVISOS: AtomicBool = AtomicBool::new(false);

/// Todo lo que llama Windows pasa por aquí: un fallo se apunta y se avisa, pero la ventana y el texto siguen.
/// Sin esto, un pánico dentro de una función que llama Windows cerraría el programa de golpe.
pub fn seguro(f: impl FnOnce() -> LRESULT, si_falla: impl FnOnce() -> LRESULT) -> LRESULT {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(_) => {
            // El aviso abre su propio bucle de mensajes: si algo vuelve a fallar mientras se ve, basta con apuntarlo.
            if !SIN_AVISOS.load(Ordering::Relaxed) && !AVISANDO.swap(true, Ordering::SeqCst) {
                mensaje(std::ptr::null_mut(), crate::textos::t().fallo, MB_OK | MB_ICONWARNING);
                AVISANDO.store(false, Ordering::SeqCst);
            }
            si_falla()
        }
    }
}

#[cfg(test)]
mod pruebas {
    #[test]
    fn ruta_con_medio_caracter() {
        use std::os::windows::ffi::OsStrExt;
        let w = [b'a' as u16, 0xD800, b'.' as u16, b't' as u16];
        assert_eq!(super::ruta(&w).as_os_str().encode_wide().collect::<Vec<_>>(), w, "la ruta sigue siendo la misma");
    }
}
