// Diálogos de Windows hechos en memoria, sin recursos: «Ir a la línea» y los avisos con botones propios (al cerrar, la
// nota para la próxima vez). Tab, Intro y Esc van como en cualquier diálogo, y en modo noche se ven oscuros.
use std::cell::Cell;

use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::tema;
use crate::ventana::app;
use crate::w::{enviar, seguro, ws};

pub const BOTON: u16 = 0x80;
pub const EDICION: u16 = 0x81;
pub const ESTATICO: u16 = 0x82;
const SS_NOPREFIX: u32 = 0x80;

/// Un diálogo en memoria (DLGTEMPLATE y sus controles), en unidades de diálogo.
pub struct Plantilla {
    v: Vec<u16>,
    n: u16,
}

impl Plantilla {
    pub fn nueva(titulo: &str, ancho: i16, alto: i16, extendido: u32) -> Plantilla {
        let estilo = WS_POPUP | WS_CAPTION | WS_SYSMENU | DS_SETFONT as u32 | DS_MODALFRAME as u32;
        let mut v = Vec::new();
        for d in [estilo, extendido] {
            v.extend([d as u16, (d >> 16) as u16]);
        }
        v.extend([0, 0, 0, ancho as u16, alto as u16, 0, 0]); // controles (se pone luego), x, y, ancho, alto, menú, clase
        v.extend(titulo.encode_utf16().chain(Some(0)));
        v.push(9);
        v.extend("Segoe UI".encode_utf16().chain(Some(0)));
        Plantilla { v, n: 0 }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn control(&mut self, clase: u16, texto: &str, id: i32, estilo: u32, x: i16, y: i16, ancho: i16, alto: i16) {
        if !self.v.len().is_multiple_of(2) {
            self.v.push(0); // cada control empieza alineado a 4 bytes
        }
        let estilo = estilo | WS_CHILD | WS_VISIBLE;
        self.v.extend([estilo as u16, (estilo >> 16) as u16, 0, 0, x as u16, y as u16, ancho as u16, alto as u16, id as u16, 0xFFFF, clase]);
        self.v.extend(texto.encode_utf16().chain(Some(0)));
        self.v.push(0);
        self.n += 1;
    }

    /// Copiada en memoria alineada a 4 bytes, como pide Windows.
    pub fn hecha(mut self) -> Vec<u32> {
        self.v[4] = self.n;
        if !self.v.len().is_multiple_of(2) {
            self.v.push(0);
        }
        self.v.chunks(2).map(|p| p[0] as u32 | (p[1] as u32) << 16).collect()
    }
}

pub fn texto_ventana(h: HWND) -> Vec<u16> {
    let largo = unsafe { GetWindowTextLengthW(h) }.max(0) as usize;
    let mut b = vec![0u16; largo + 1];
    let n = unsafe { GetWindowTextW(h, b.as_mut_ptr(), b.len() as i32) }.max(0) as usize;
    b.truncate(n);
    b
}

pub fn poner_texto(h: HWND, s: &[u16]) {
    let mut z = s.to_vec();
    z.push(0);
    unsafe { SetWindowTextW(h, z.as_ptr()) };
}

thread_local! {
    static PINCEL: Cell<HBRUSH> = const { Cell::new(std::ptr::null_mut()) };
    static PINCEL_EDICION: Cell<HBRUSH> = const { Cell::new(std::ptr::null_mut()) };
}

/// Colores del diálogo en modo noche (en claro, los de Windows).
pub fn color_control(m: u32, dc: HDC) -> isize {
    let p = tema::paleta(true);
    let (pincel, fondo) = if m == WM_CTLCOLOREDIT { (&PINCEL_EDICION, p.fondo) } else { (&PINCEL, p.barra) };
    pincel.with(|b| {
        if b.get().is_null() {
            b.set(unsafe { CreateSolidBrush(fondo) });
        }
        unsafe {
            SetTextColor(dc, p.texto);
            SetBkColor(dc, fondo);
        }
        b.get() as isize
    })
}

/// El título y los controles, oscuros en modo noche.
pub fn tema_dialogo(h: HWND) {
    let noche = app().cfg.borrow().noche;
    tema::titulo(h, noche);
    let mut c = unsafe { GetWindow(h, GW_CHILD) };
    while !c.is_null() {
        let mut clase = [0u16; 16];
        let n = unsafe { GetClassNameW(c, clase.as_mut_ptr(), 16) }.max(0) as usize;
        match String::from_utf16_lossy(&clase[..n]).as_str() {
            "Button" => tema::nativo(c, noche, "DarkMode_Explorer"),
            "Edit" => tema::nativo(c, noche, "DarkMode_CFD"),
            _ => {}
        }
        c = unsafe { GetWindow(c, GW_HWNDNEXT) };
    }
    unsafe { InvalidateRect(h, std::ptr::null(), 1) };
}

// --- Avisos con botones propios ---

const TEXTO: i32 = 20;
const NOTA: i32 = 21;

struct Pregunta<'a> {
    texto: &'a str,
    botones: &'a [(i32, &'a str)],
    nota: Option<&'a str>,
}

/// Un aviso con sus botones, el primero el de por defecto, y debajo una nota en gris si hace falta. Devuelve el número
/// del botón pulsado; Esc y la ✕ dan IDCANCEL, así que uno de los botones tiene que llevarlo.
pub fn preguntar(texto: &str, botones: &[(i32, &str)], nota: Option<&str>) -> i32 {
    let mut p = Plantilla::nueva("Bloc", 200, 60, 0);
    p.control(ESTATICO, texto, TEXTO, SS_NOPREFIX, 0, 0, 0, 0);
    if let Some(n) = nota {
        p.control(ESTATICO, n, NOTA, SS_NOPREFIX, 0, 0, 0, 0);
    }
    for (i, (id, b)) in botones.iter().enumerate() {
        p.control(BOTON, b, *id, WS_TABSTOP | if i == 0 { BS_DEFPUSHBUTTON } else { BS_PUSHBUTTON } as u32, 0, 0, 0, 0);
    }
    let plantilla = p.hecha();
    let datos = Pregunta { texto, botones, nota };
    unsafe {
        DialogBoxIndirectParamW(
            GetModuleHandleW(std::ptr::null()),
            plantilla.as_ptr() as *const DLGTEMPLATE,
            app().hwnd.get(),
            Some(proc_pregunta),
            &datos as *const Pregunta as isize,
        ) as i32
    }
}

unsafe extern "system" fn proc_pregunta(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> isize {
    seguro(|| mensaje_pregunta(h, m, wp, lp), || 0)
}

fn mensaje_pregunta(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> isize {
    match m {
        WM_INITDIALOG => {
            let p = unsafe { &*(lp as *const Pregunta) };
            colocar(h, p);
            tema_dialogo(h);
            // Sin la animación de Windows al abrirse: el aviso sale al momento.
            let si: i32 = 1;
            unsafe { DwmSetWindowAttribute(h, 3, &si as *const i32 as *const _, 4) }; // DWMWA_TRANSITIONS_FORCEDISABLED
            unsafe { SetFocus(GetDlgItem(h, p.botones[0].0)) };
            0
        }
        WM_COMMAND => {
            let id = (wp & 0xFFFF) as i32;
            // Solo sus botones (los que llevan número de botón y no de los textos) y Esc.
            if id == IDCANCEL || (id != TEXTO && id != NOTA && !unsafe { GetDlgItem(h, id) }.is_null()) {
                unsafe { EndDialog(h, id as isize) };
            }
            1
        }
        WM_CTLCOLORSTATIC if unsafe { GetDlgCtrlID(lp as HWND) } == NOTA => {
            let r = if app().cfg.borrow().noche { color_control(m, wp as HDC) } else { unsafe { GetSysColorBrush(COLOR_BTNFACE) as isize } };
            unsafe {
                SetTextColor(wp as HDC, if app().cfg.borrow().noche { tema::paleta(true).apagado } else { GetSysColor(COLOR_GRAYTEXT) });
                if !app().cfg.borrow().noche {
                    SetBkColor(wp as HDC, GetSysColor(COLOR_BTNFACE));
                }
            }
            r
        }
        WM_CTLCOLORDLG | WM_CTLCOLORSTATIC | WM_CTLCOLORBTN if app().cfg.borrow().noche => color_control(m, wp as HDC),
        _ => 0,
    }
}

/// En píxeles, según lo que mida cada texto con la letra del diálogo: el texto arriba, los botones a la derecha debajo y
/// la nota al pie.
fn colocar(h: HWND, p: &Pregunta) {
    let du = |x: i32, y: i32| {
        let mut r = RECT { left: 0, top: 0, right: x, bottom: y };
        unsafe { MapDialogRect(h, &mut r) };
        (r.right, r.bottom)
    };
    let (m, _) = du(7, 0);
    let (hueco, alto_boton) = du(4, 14);
    let (minimo, _) = du(50, 0);
    let (relleno, _) = du(8, 0);
    let (ancho_minimo, _) = du(220, 0);
    unsafe {
        let dc = GetDC(h);
        let antes = SelectObject(dc, enviar(h, WM_GETFONT, 0, 0) as _);
        let medir = |t: &str, ancho: i32, formato: u32| {
            let s = ws(t);
            let mut r = RECT { left: 0, top: 0, right: ancho, bottom: 0 };
            DrawTextW(dc, s.as_ptr(), -1, &mut r, DT_CALCRECT | formato);
            (r.right, r.bottom)
        };
        let anchos: Vec<i32> = p.botones.iter().map(|(_, b)| (medir(b, 0, DT_SINGLELINE).0 + 2 * relleno).max(minimo)).collect();
        let fila = anchos.iter().sum::<i32>() + hueco * (anchos.len() as i32 - 1);
        let ancho = fila.max(ancho_minimo);
        let (_, alto_texto) = medir(p.texto, ancho, DT_WORDBREAK | DT_NOPREFIX);
        let mut y = m;
        MoveWindow(GetDlgItem(h, TEXTO), m, y, ancho, alto_texto, 0);
        y += alto_texto + m + hueco;
        let mut x = m + ancho - fila;
        for ((id, _), a) in p.botones.iter().zip(&anchos) {
            MoveWindow(GetDlgItem(h, *id), x, y, *a, alto_boton, 0);
            x += a + hueco;
        }
        y += alto_boton;
        if let Some(n) = p.nota {
            let (_, alto_nota) = medir(n, ancho, DT_WORDBREAK | DT_NOPREFIX);
            y += hueco + hueco / 2;
            MoveWindow(GetDlgItem(h, NOTA), m, y, ancho, alto_nota, 0);
            y += alto_nota;
        }
        y += m;
        SelectObject(dc, antes);
        ReleaseDC(h, dc);
        let estilo = GetWindowLongW(h, GWL_STYLE) as u32;
        let extendido = GetWindowLongW(h, GWL_EXSTYLE) as u32;
        let mut r = RECT { left: 0, top: 0, right: ancho + 2 * m, bottom: y };
        AdjustWindowRectEx(&mut r, estilo, 0, extendido);
        // Centrado sobre la ventana de Bloc.
        let mut v = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        GetWindowRect(app().hwnd.get(), &mut v);
        let (w, a) = (r.right - r.left, r.bottom - r.top);
        SetWindowPos(h, std::ptr::null_mut(), (v.left + v.right - w) / 2, (v.top + v.bottom - a) / 2, w, a, SWP_NOZORDER | SWP_NOACTIVATE);
    }
}
