// La página de después de instalar, que también se abre desde Archivo: qué archivos abre Bloc, en cuáles enseña la vista
// previa del Explorador y si se abre al iniciar Windows. Es un diálogo de Bloc con el aspecto del de tareas de Windows
// (cuerpo claro y pie gris con los botones), porque ese no admite una tabla de casillas.
use std::cell::Cell;

use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::{CloseThemeData, GetThemePartSize, OpenThemeData, BP_CHECKBOX, CBS_UNCHECKEDNORMAL, TS_TRUE};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetFocus, SetFocus};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::dialogo::{color_control, Plantilla, BOTON, ESTATICO};
use crate::instalar::Eleccion;
use crate::tema;
use crate::textos::t;
use crate::w::{enviar, seguro, ws};

/// Los botones, con el mismo número que los del instalador: las pruebas los pulsan por él.
pub const PREDETERMINADO: i32 = 101;
pub const ABRIR: i32 = 102;
// Cada fila suma su número a estos; las cabeceras, cuatro por columna de la tabla.
const CASILLA_ABRIR: i32 = 1000;
const CASILLA_VISTA: i32 = 2000;
const NOMBRE: i32 = 3000;
const AHORA: i32 = 4000;
const CABECERA: i32 = 5000;
const TITULO: i32 = 330;
const TEXTO: i32 = 331;
const NOTA: i32 = 332;
const INICIO: i32 = 333;
const INICIO_ETIQUETA: i32 = 334;
// Cada casilla suelta va seguida de su etiqueta.
const EDITAR: i32 = 335;
const EDITAR_ETIQUETA: i32 = 336;
const SS_NOPREFIX: u32 = 0x80;
const SS_NOTIFY: u32 = 0x100;
const SS_ENDELLIPSIS: u32 = 0x4000;
/// DWLP_MSGRESULT y DLGPROC van delante, 8 bytes cada uno.
const DWLP_USER: i32 = 16;
/// Más filas que estas y la tabla se parte en columnas, para que la página quepa en la pantalla.
const FILAS: usize = 16;

fn columnas(n: usize) -> usize {
    n.div_ceil(FILAS).max(1)
}

struct Datos<'a> {
    e: &'a mut Eleccion,
    ahora: &'a [String],
    instalador: bool,
    noche: bool,
    padre: HWND,
    /// Dónde empieza el pie gris.
    pie: Cell<i32>,
    titulo: Cell<HFONT>,
    cuerpo: Cell<HBRUSH>,
    gris: Cell<HBRUSH>,
}

/// Las casillas son solo el cuadro: con su texto, Windows lo pinta negro en modo noche. El texto va en una etiqueta al
/// lado, y la casilla lo lleva igual para los lectores de pantalla.
fn casilla(p: &mut Plantilla, nombre: &str, id: i32) {
    p.control(BOTON, nombre, id, WS_TABSTOP | BS_AUTOCHECKBOX as u32 | BS_NOTIFY as u32, 0, 0, 0, 0);
}

/// Desde el instalador (`padre` nulo): «Abrir Bloc» y «Cerrar», y los dos guardan lo elegido. Desde Bloc: Aceptar y
/// Cancelar. Devuelve el botón pulsado; lo elegido queda en `e` (también con Cancelar, sin cambios).
pub fn preguntar(padre: HWND, noche: bool, e: &mut Eleccion, ahora: &[String]) -> i32 {
    let t = t();
    let instalador = padre.is_null();
    let mut p = Plantilla::nueva("Bloc", 300, 200, 0);
    p.control(ESTATICO, if instalador { t.instalado_titulo } else { t.eleccion_titulo }, TITULO, SS_NOPREFIX, 0, 0, 0, 0);
    p.control(ESTATICO, t.eleccion_texto, TEXTO, SS_NOPREFIX, 0, 0, 0, 0);
    for c in 0..columnas(e.todas.len()) {
        for (i, cab) in [t.col_archivo, t.col_ahora, t.col_abrir, t.col_vista].iter().enumerate() {
            p.control(ESTATICO, cab, CABECERA + (4 * c + i) as i32, SS_NOPREFIX, 0, 0, 0, 0);
        }
    }
    for (i, ext) in e.todas.iter().enumerate() {
        let i = i as i32;
        p.control(ESTATICO, ext, NOMBRE + i, SS_NOPREFIX, 0, 0, 0, 0);
        p.control(ESTATICO, &ahora[i as usize], AHORA + i, SS_NOPREFIX | SS_ENDELLIPSIS, 0, 0, 0, 0);
        casilla(&mut p, &format!("{}: {ext}", t.col_abrir), CASILLA_ABRIR + i);
        casilla(&mut p, &format!("{}: {ext}", t.col_vista), CASILLA_VISTA + i);
    }
    for (id, texto) in [(INICIO, t.inicio_windows), (EDITAR, t.editar_menu)] {
        p.control(ESTATICO, texto, id + 1, SS_NOPREFIX | SS_NOTIFY, 0, 0, 0, 0);
        casilla(&mut p, texto, id);
    }
    p.control(ESTATICO, t.eleccion_nota, NOTA, SS_NOPREFIX, 0, 0, 0, 0);
    p.control(BOTON, t.predeterminado, PREDETERMINADO, WS_TABSTOP | BS_PUSHBUTTON as u32, 0, 0, 0, 0);
    let botones = if instalador { [(ABRIR, t.abrir_bloc), (IDCLOSE, t.cerrar)] } else { [(IDOK, t.aceptar), (IDCANCEL, t.cancelar)] };
    for (i, (id, b)) in botones.iter().enumerate() {
        p.control(BOTON, b, *id, WS_TABSTOP | if i == 0 { BS_DEFPUSHBUTTON } else { BS_PUSHBUTTON } as u32, 0, 0, 0, 0);
    }
    let plantilla = p.hecha();
    let mut datos = Datos {
        e,
        ahora,
        instalador,
        noche,
        padre,
        pie: Cell::new(0),
        titulo: Cell::new(std::ptr::null_mut()),
        cuerpo: Cell::new(std::ptr::null_mut()),
        gris: Cell::new(std::ptr::null_mut()),
    };
    let r = unsafe { DialogBoxIndirectParamW(GetModuleHandleW(std::ptr::null()), plantilla.as_ptr() as *const DLGTEMPLATE, padre, Some(proc_eleccion), &mut datos as *mut Datos as isize) as i32 };
    unsafe {
        for o in [datos.titulo.get() as HGDIOBJ, datos.cuerpo.get() as _, datos.gris.get() as _] {
            if !o.is_null() {
                DeleteObject(o);
            }
        }
    }
    r
}

unsafe extern "system" fn proc_eleccion(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> isize {
    seguro(|| mensaje(h, m, wp, lp), || 0)
}

/// Los de `preguntar`, que viven mientras el diálogo.
fn datos(h: HWND) -> Option<&'static mut Datos<'static>> {
    let p = unsafe { GetWindowLongPtrW(h, DWLP_USER) } as *mut Datos<'static>;
    unsafe { p.as_mut() }
}

fn es_casilla(id: i32) -> bool {
    (CASILLA_ABRIR..NOMBRE).contains(&id) || matches!(id, INICIO | EDITAR)
}

fn marcada(h: HWND, id: i32) -> bool {
    enviar(unsafe { GetDlgItem(h, id) }, BM_GETCHECK, 0, 0) == 1
}

/// Lo que dicen las casillas, a la elección.
fn leer(h: HWND, d: &mut Datos) {
    let cuales = |base: i32| d.e.todas.iter().enumerate().filter(|(i, _)| marcada(h, base + *i as i32)).map(|(_, e)| e.clone()).collect();
    let (abrir, vista) = (cuales(CASILLA_ABRIR), cuales(CASILLA_VISTA));
    d.e.abrir = abrir;
    d.e.vista = vista;
    d.e.inicio = marcada(h, INICIO);
    d.e.editar = marcada(h, EDITAR);
}

fn colores(d: &Datos) -> (u32, u32, u32) {
    let p = tema::paleta(d.noche);
    if d.noche { (p.fondo, p.barra, p.texto) } else { (p.fondo, p.barra, unsafe { GetSysColor(COLOR_WINDOWTEXT) }) }
}

fn mensaje(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> isize {
    match m {
        WM_INITDIALOG => {
            unsafe { SetWindowLongPtrW(h, DWLP_USER, lp) };
            let Some(d) = datos(h) else { return 0 };
            let (cuerpo, gris, _) = colores(d);
            d.cuerpo.set(unsafe { CreateSolidBrush(cuerpo) });
            d.gris.set(unsafe { CreateSolidBrush(gris) });
            for (i, ext) in d.e.todas.iter().enumerate() {
                let i = i as i32;
                enviar(unsafe { GetDlgItem(h, CASILLA_ABRIR + i) }, BM_SETCHECK, d.e.abrir.contains(ext) as usize, 0);
                enviar(unsafe { GetDlgItem(h, CASILLA_VISTA + i) }, BM_SETCHECK, d.e.vista.contains(ext) as usize, 0);
            }
            enviar(unsafe { GetDlgItem(h, INICIO) }, BM_SETCHECK, d.e.inicio as usize, 0);
            enviar(unsafe { GetDlgItem(h, EDITAR) }, BM_SETCHECK, d.e.editar as usize, 0);
            colocar(h, d);
            tema::titulo(h, d.noche);
            let mut c = unsafe { GetWindow(h, GW_CHILD) };
            while !c.is_null() {
                let mut clase = [0u16; 8];
                let n = unsafe { GetClassNameW(c, clase.as_mut_ptr(), 8) }.max(0) as usize;
                if String::from_utf16_lossy(&clase[..n]) == "Button" {
                    tema::nativo(c, d.noche, "DarkMode_Explorer");
                }
                c = unsafe { GetWindow(c, GW_HWNDNEXT) };
            }
            let si: i32 = 1;
            unsafe { DwmSetWindowAttribute(h, 3, &si as *const i32 as *const _, 4) }; // DWMWA_TRANSITIONS_FORCEDISABLED
            unsafe { SetFocus(GetDlgItem(h, if d.instalador { ABRIR } else { IDOK })) };
            0
        }
        WM_COMMAND => {
            let (id, aviso) = ((wp & 0xFFFF) as i32, (wp >> 16) as u32);
            let Some(d) = datos(h) else { return 0 };
            match (id, aviso) {
                (INICIO_ETIQUETA | EDITAR_ETIQUETA, STN_CLICKED) => {
                    let c = unsafe { GetDlgItem(h, id - 1) };
                    enviar(c, BM_SETCHECK, !marcada(h, id - 1) as usize, 0);
                    unsafe { SetFocus(c) };
                }
                (PREDETERMINADO, BN_CLICKED) => {
                    leer(h, d);
                    crate::instalar::aplicar(d.e);
                    crate::instalar::pantalla_predeterminados(h);
                }
                (_, BN_SETFOCUS | BN_KILLFOCUS) if es_casilla(id) => repintar_foco(h, id),
                (ABRIR | IDOK, BN_CLICKED) => {
                    leer(h, d);
                    unsafe { EndDialog(h, id as isize) };
                }
                // Esc y la ✕: en el instalador, como Cerrar (lo elegido se guarda); desde Bloc, como Cancelar.
                (IDCLOSE | IDCANCEL, BN_CLICKED) => {
                    if d.instalador {
                        leer(h, d);
                    }
                    unsafe { EndDialog(h, if d.instalador { IDCLOSE } else { IDCANCEL } as isize) };
                }
                _ => {}
            }
            1
        }
        WM_ERASEBKGND => {
            let Some(d) = datos(h) else { return 0 };
            let dc = wp as HDC;
            let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            unsafe {
                GetClientRect(h, &mut r);
                let pie = RECT { top: d.pie.get(), ..r };
                FillRect(dc, &RECT { bottom: d.pie.get(), ..r }, d.cuerpo.get());
                FillRect(dc, &pie, d.gris.get());
                let raya = CreateSolidBrush(tema::paleta(d.noche).borde);
                FillRect(dc, &RECT { bottom: pie.top + 1, ..pie }, raya);
                DeleteObject(raya as HGDIOBJ);
                // Las casillas son solo el cuadro: el foco se dibuja alrededor, en el diálogo.
                let f = GetFocus();
                let id = GetDlgCtrlID(f);
                if GetParent(f) == h && es_casilla(id) && enviar(h, WM_QUERYUISTATE, 0, 0) & 1 == 0 {
                    SetTextColor(dc, colores(d).2);
                    SetBkColor(dc, colores(d).0);
                    DrawFocusRect(dc, &alrededor(h, f));
                }
            }
            1
        }
        WM_CTLCOLORDLG | WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
            let Some(d) = datos(h) else { return 0 };
            let c = lp as HWND;
            let id = unsafe { GetDlgCtrlID(c) };
            let dc = wp as HDC;
            let (cuerpo, gris, texto) = colores(d);
            let en_pie = m == WM_CTLCOLORBTN && matches!(id, PREDETERMINADO | ABRIR | IDOK | IDCANCEL | IDCLOSE);
            if m == WM_CTLCOLORDLG {
                return d.cuerpo.get() as isize;
            }
            if d.noche && en_pie {
                return color_control(m, dc);
            }
            let apagado = matches!(id, NOTA) || (AHORA..CABECERA + 1000).contains(&id);
            let titulo = id == TITULO;
            unsafe {
                SetBkColor(dc, if en_pie { gris } else { cuerpo });
                SetTextColor(
                    dc,
                    match () {
                        _ if titulo && !d.noche => crate::w::rgb(0, 51, 153),
                        _ if apagado => tema::paleta(d.noche).apagado,
                        _ => texto,
                    },
                );
            }
            (if en_pie { d.gris.get() } else { d.cuerpo.get() }) as isize
        }
        _ => 0,
    }
}

/// El rectángulo del foco de una casilla, en el diálogo.
fn alrededor(h: HWND, c: HWND) -> RECT {
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe {
        GetWindowRect(c, &mut r);
        MapWindowPoints(std::ptr::null_mut(), h, &mut r as *mut RECT as *mut _, 2);
        InflateRect(&mut r, 2, 2);
    }
    r
}

fn repintar_foco(h: HWND, id: i32) {
    let r = alrededor(h, unsafe { GetDlgItem(h, id) });
    unsafe {
        let mut fuera = r;
        InflateRect(&mut fuera, 1, 1);
        InvalidateRect(h, &fuera, 1);
    }
}

/// En píxeles, según lo que mida cada texto: el título y la explicación, la tabla (archivo, con qué se abre ahora y las
/// dos casillas, centradas bajo su cabecera), la casilla de iniciar con Windows, la nota y el pie con los botones.
fn colocar(h: HWND, d: &Datos) {
    let du = |x: i32, y: i32| {
        let mut r = RECT { left: 0, top: 0, right: x, bottom: y };
        unsafe { MapDialogRect(h, &mut r) };
        (r.right, r.bottom)
    };
    let (m, my) = du(7, 7);
    let (hueco, alto_boton) = du(4, 14);
    let (_, fila) = du(0, 12);
    let (columna, _) = du(10, 0);
    let (minimo, _) = du(50, 0);
    let (relleno, _) = du(8, 0);
    unsafe {
        let dc = GetDC(h);
        let letra = enviar(h, WM_GETFONT, 0, 0) as HFONT;
        let mut lf: LOGFONTW = std::mem::zeroed();
        GetObjectW(letra as HGDIOBJ, std::mem::size_of::<LOGFONTW>() as i32, &mut lf as *mut LOGFONTW as *mut _);
        lf.lfHeight = lf.lfHeight * 4 / 3; // 12 puntos, como el título del diálogo de tareas
        d.titulo.set(CreateFontIndirectW(&lf));
        enviar(GetDlgItem(h, TITULO), WM_SETFONT, d.titulo.get() as usize, 0);
        let antes = SelectObject(dc, letra as HGDIOBJ);
        let medir = |t: &str, ancho: i32, formato: u32| {
            let s = ws(t);
            let mut r = RECT { left: 0, top: 0, right: ancho, bottom: 0 };
            DrawTextW(dc, s.as_ptr(), -1, &mut r, DT_CALCRECT | DT_NOPREFIX | formato);
            (r.right, r.bottom)
        };
        let tema_boton = OpenThemeData(h, ws("Button").as_ptr());
        let mut caja = SIZE { cx: du(10, 0).0, cy: du(0, 10).1 };
        if tema_boton != 0 {
            GetThemePartSize(tema_boton, dc, BP_CHECKBOX, CBS_UNCHECKEDNORMAL, std::ptr::null(), TS_TRUE, &mut caja);
            CloseThemeData(tema_boton);
        }
        let t = t();
        let cab = [t.col_archivo, t.col_ahora, t.col_abrir, t.col_vista];
        let mut anchos: Vec<i32> = cab.iter().map(|c| medir(c, 0, DT_SINGLELINE).0).collect();
        let n = d.e.todas.len();
        let cols = columnas(n);
        anchos[0] = d.e.todas.iter().map(|e| medir(e, 0, DT_SINGLELINE).0).max().unwrap_or(0).max(anchos[0]);
        anchos[1] = d.ahora.iter().map(|a| medir(a, 0, DT_SINGLELINE).0).max().unwrap_or(0).max(anchos[1]).min(du(if cols > 1 { 90 } else { 130 }, 0).0);
        anchos[2] = anchos[2].max(caja.cx);
        anchos[3] = anchos[3].max(caja.cx);
        let bloque = anchos.iter().sum::<i32>() + columna * 3;
        let (separacion, _) = du(16, 0);
        let tabla = bloque * cols as i32 + separacion * (cols as i32 - 1);
        let botones = [PREDETERMINADO, if d.instalador { ABRIR } else { IDOK }, if d.instalador { IDCLOSE } else { IDCANCEL }];
        let ancho_boton: Vec<i32> = botones
            .iter()
            .map(|&b| {
                let mut s = [0u16; 128];
                let n = GetWindowTextW(GetDlgItem(h, b), s.as_mut_ptr(), 128).max(0) as usize;
                (medir(&String::from_utf16_lossy(&s[..n]), 0, DT_SINGLELINE).0 + 2 * relleno).max(minimo)
            })
            .collect();
        let fila_botones = ancho_boton.iter().sum::<i32>() + hueco * 3;
        let ancho = tabla.max(fila_botones).max(du(260, 0).0);

        let mover = |id: i32, x: i32, y: i32, w: i32, a: i32| MoveWindow(GetDlgItem(h, id), x, y, w, a, 0);
        let mut y = my;
        SelectObject(dc, d.titulo.get() as HGDIOBJ);
        let (_, alto_titulo) = medir(if d.instalador { t.instalado_titulo } else { t.eleccion_titulo }, ancho, DT_WORDBREAK);
        SelectObject(dc, letra as HGDIOBJ);
        mover(TITULO, m, y, ancho, alto_titulo);
        y += alto_titulo + hueco;
        let (_, alto) = medir(t.eleccion_texto, ancho, DT_WORDBREAK);
        mover(TEXTO, m, y, ancho, alto);
        y += alto + my;

        let (_, alto_letra) = medir("Ág", 0, DT_SINGLELINE);
        let mut x = 0;
        let xs: Vec<i32> = anchos
            .iter()
            .map(|a| {
                let r = x;
                x += a + columna;
                r
            })
            .collect();
        let izquierda = |c: usize| m + c as i32 * (bloque + separacion);
        for c in 0..cols {
            for (i, (cx, a)) in xs.iter().zip(&anchos).enumerate() {
                // Las cabeceras de las casillas, centradas sobre ellas.
                let (w, _) = medir(cab[i], 0, DT_SINGLELINE);
                let x = izquierda(c) + if i >= 2 { cx + (a - w) / 2 } else { *cx };
                mover(CABECERA + (4 * c + i) as i32, x, y, w, alto_letra);
            }
        }
        y += alto_letra + hueco;
        let filas = n.div_ceil(cols);
        for i in 0..n {
            let (x0, fy) = (izquierda(i / filas), y + (i % filas) as i32 * fila);
            let i = i as i32;
            let centro = fy + (fila - alto_letra) / 2;
            mover(NOMBRE + i, x0 + xs[0], centro, anchos[0], alto_letra);
            mover(AHORA + i, x0 + xs[1], centro, anchos[1], alto_letra);
            let cy = fy + (fila - caja.cy) / 2;
            mover(CASILLA_ABRIR + i, x0 + xs[2] + (anchos[2] - caja.cx) / 2, cy, caja.cx, caja.cy);
            mover(CASILLA_VISTA + i, x0 + xs[3] + (anchos[3] - caja.cx) / 2, cy, caja.cx, caja.cy);
        }
        y += filas as i32 * fila;
        y += my;
        for (id, texto) in [(INICIO, t.inicio_windows), (EDITAR, t.editar_menu)] {
            mover(id, m, y + (fila - caja.cy) / 2, caja.cx, caja.cy);
            let (w, _) = medir(texto, 0, DT_SINGLELINE);
            mover(id + 1, m + caja.cx + hueco, y + (fila - alto_letra) / 2, w, alto_letra);
            y += fila;
        }
        y += hueco;
        let (_, alto) = medir(t.eleccion_nota, ancho, DT_WORDBREAK);
        mover(NOTA, m, y, ancho, alto);
        y += alto + my;

        d.pie.set(y);
        y += my;
        mover(PREDETERMINADO, m, y, ancho_boton[0], alto_boton);
        let mut x = m + ancho - ancho_boton[1] - ancho_boton[2] - hueco;
        for (b, a) in botones[1..].iter().zip(&ancho_boton[1..]) {
            mover(*b, x, y, *a, alto_boton);
            x += a + hueco;
        }
        y += alto_boton + my;
        SelectObject(dc, antes);
        ReleaseDC(h, dc);

        let estilo = GetWindowLongW(h, GWL_STYLE) as u32;
        let extendido = GetWindowLongW(h, GWL_EXSTYLE) as u32;
        let mut r = RECT { left: 0, top: 0, right: ancho + 2 * m, bottom: y };
        AdjustWindowRectEx(&mut r, estilo, 0, extendido);
        let (w, a) = (r.right - r.left, r.bottom - r.top);
        // Centrado sobre Bloc o, desde el instalador, en la pantalla.
        let mut v = RECT { left: 0, top: 0, right: GetSystemMetrics(SM_CXSCREEN), bottom: GetSystemMetrics(SM_CYSCREEN) };
        if !d.padre.is_null() {
            GetWindowRect(d.padre, &mut v);
        } else {
            SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut v as *mut RECT as *mut _, 0);
        }
        let (x, y) = (((v.left + v.right - w) / 2).max(v.left), ((v.top + v.bottom - a) / 2).max(v.top));
        SetWindowPos(h, std::ptr::null_mut(), x, y, w, a, SWP_NOZORDER | SWP_NOACTIVATE);
    }
}
