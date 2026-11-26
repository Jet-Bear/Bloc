// Modo noche. Windows oscurece la barra de título, los menús desplegables y las barras de desplazamiento si se le
// pide (los menús, con dos funciones sin nombre de uxtheme, las mismas que usan Notepad++ y el Explorador); lo
// demás lo pinta Bloc. Si alguna de esas funciones no existe, esa parte se queda clara, sin fallar.
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::{GetSysColor, COLOR_3DSHADOW, COLOR_BTNFACE, COLOR_BTNTEXT, COLOR_GRAYTEXT, COLOR_WINDOW, COLOR_WINDOWTEXT};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows_sys::Win32::UI::Controls::SetWindowTheme;

use crate::md::Colores;
use crate::w::{rgb, ws};

/// Colores de la ventana (barras, menús y diálogos).
#[derive(Clone, Copy)]
pub struct Paleta {
    pub fondo: u32,
    pub texto: u32,
    pub barra: u32,
    pub resalte: u32,
    pub borde: u32,
    pub apagado: u32,
}

pub fn paleta(noche: bool) -> Paleta {
    if noche {
        Paleta {
            fondo: rgb(31, 31, 31),
            texto: rgb(214, 214, 214),
            barra: rgb(43, 43, 43),
            resalte: rgb(64, 64, 64),
            borde: rgb(72, 72, 72),
            apagado: rgb(140, 140, 140),
        }
    } else {
        unsafe {
            Paleta {
                fondo: GetSysColor(COLOR_WINDOW),
                texto: GetSysColor(COLOR_WINDOWTEXT),
                barra: GetSysColor(COLOR_BTNFACE),
                resalte: rgb(218, 230, 245),
                borde: GetSysColor(COLOR_3DSHADOW),
                apagado: GetSysColor(COLOR_GRAYTEXT),
            }
        }
    }
}

/// Texto de los botones y la barra de estado en claro.
pub fn texto_barra(noche: bool) -> u32 {
    if noche { paleta(true).texto } else { unsafe { GetSysColor(COLOR_BTNTEXT) } }
}

/// Los colores de un Markdown, como en GitHub.
/// Los de la vista previa del Explorador (la de PowerToys), en claro y en oscuro.
pub fn colores_md(noche: bool) -> Colores {
    if noche {
        Colores {
            texto: rgb(212, 212, 212),
            apagado: rgb(125, 133, 144),
            acento: rgb(3, 102, 214),
            fondo_codigo: rgb(22, 22, 22),
            cita: rgb(212, 212, 212),
            fondo: rgb(30, 30, 30),
            rejilla: rgb(71, 71, 71),
            raya: rgb(71, 71, 71),
            fondo_cita: rgb(40, 40, 40),
            barra_cita: rgb(212, 212, 212),
        }
    } else {
        Colores {
            texto: rgb(33, 37, 41),
            apagado: rgb(140, 149, 159),
            acento: rgb(3, 102, 214),
            fondo_codigo: rgb(246, 248, 250),
            cita: rgb(106, 115, 125),
            fondo: rgb(255, 255, 255),
            rejilla: rgb(223, 226, 229),
            raya: rgb(234, 236, 239),
            fondo_cita: rgb(255, 255, 255),
            barra_cita: rgb(223, 226, 229),
        }
    }
}

pub fn como_windows() -> bool {
    let clave = ws(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let valor = ws("AppsUseLightTheme");
    let mut claro: u32 = 1;
    let mut tam = 4u32;
    let r = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, clave.as_ptr(), valor.as_ptr(), RRF_RT_REG_DWORD, std::ptr::null_mut(), &mut claro as *mut u32 as *mut _, &mut tam)
    };
    r == 0 && claro == 0
}

/// Los menús desplegables (de todo el proceso).
pub fn menus(noche: bool) {
    unsafe {
        let ux = ws("uxtheme.dll");
        let m = LoadLibraryExW(ux.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
        if m.is_null() {
            return;
        }
        if let (Some(modo), Some(vaciar)) = (GetProcAddress(m, 135 as _), GetProcAddress(m, 136 as _)) {
            let modo: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(modo);
            let vaciar: unsafe extern "system" fn() = std::mem::transmute(vaciar);
            modo(if noche { 2 } else { 0 }); // 2: siempre oscuro
            vaciar();
        }
    }
}

pub fn titulo(h: HWND, noche: bool) {
    let si: i32 = noche as i32;
    unsafe { DwmSetWindowAttribute(h, 20, &si as *const i32 as *const _, 4) }; // DWMWA_USE_IMMERSIVE_DARK_MODE
}

/// Las barras de desplazamiento y los bordes de un control de Windows.
pub fn nativo(h: HWND, noche: bool, clase: &str) {
    let t = ws(clase);
    unsafe { SetWindowTheme(h, if noche { t.as_ptr() } else { std::ptr::null() }, std::ptr::null()) };
}
