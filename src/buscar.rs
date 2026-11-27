// Buscar y reemplazar: un panel que sale de debajo de la barra, con la fila de reemplazar debajo cuando hace falta. E
// «Ir a la línea», con un diálogo de los de dialogo.rs.
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::{EM_SETSEL, TOOLTIPS_CLASSW, TTF_SUBCLASS, TTM_ADDTOOLW, TTM_NEWTOOLRECTW, TTS_ALWAYSTIP, TTS_NOPREFIX, TTTOOLINFOW, WM_MOUSELEAVE};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::textos::{con_atajo, limpio, t};
use crate::ventana::{self, app};
use crate::w::{con, enviar, pulsada, seguro, ws};
use crate::dialogo::{color_control, poner_texto, tema_dialogo, texto_ventana, Plantilla, BOTON, EDICION, ESTATICO};
use crate::{barras, editor, tema};

const CR: u16 = 13;
const ETIQUETA: i32 = 20;
const NUMERO: i32 = 21;

thread_local! {
    static LINEA: Cell<usize> = const { Cell::new(0) };
}

// --- Buscar y reemplazar ---

/// Lo que hacen los botones del panel. Su número es también el WM_COMMAND (con lParam 0) que los pulsa.
#[derive(Clone, Copy, PartialEq)]
enum Accion {
    Desplegar = 1,
    Anterior,
    Siguiente,
    Mayus,
    Cerrar,
    Uno,
    Todos,
}

const ACCIONES: [Accion; 7] = [Accion::Desplegar, Accion::Anterior, Accion::Siguiente, Accion::Mayus, Accion::Cerrar, Accion::Uno, Accion::Todos];
const QUE: i32 = 10;
const POR: i32 = 12;
/// El paso de la animación y, tras cambiar el texto, volver a contar (ver `cambiado`).
const RELOJ_PASO: usize = 1;
const RELOJ_CONTAR: usize = 2;
/// El despliegue: 8 pasos de 15 ms (unos 120 ms), que frenan al final. Por pasos y no por tiempo: si Windows tarda en
/// mandar alguno, dura un poco más en vez de dar un salto.
const PASOS: u32 = 8;

thread_local! {
    static CAJA_QUE: Cell<HWND> = const { Cell::new(std::ptr::null_mut()) };
    static CAJA_POR: Cell<HWND> = const { Cell::new(std::ptr::null_mut()) };
    static AYUDAS: Cell<HWND> = const { Cell::new(std::ptr::null_mut()) };
    static TEXTOS_AYUDA: RefCell<Vec<Vec<u16>>> = const { RefCell::new(Vec::new()) };
    static CON_POR: Cell<bool> = const { Cell::new(false) };
    static MAYUS: Cell<bool> = const { Cell::new(false) };
    /// El alto que tiene ahora (el que se ve) y la animación: de dónde, a dónde y por qué paso va.
    static ALTO: Cell<i32> = const { Cell::new(0) };
    static DESDE: Cell<i32> = const { Cell::new(0) };
    static META: Cell<i32> = const { Cell::new(0) };
    static PASO: Cell<u32> = const { Cell::new(0) };
    static ANCHO: Cell<isize> = const { Cell::new(-1) };
    static ENCIMA: Cell<i32> = const { Cell::new(-1) };
    static PULSADO: Cell<i32> = const { Cell::new(-1) };
    /// «3 de 12», a la derecha de la casilla de buscar.
    static CUENTA: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
    static PINCEL_CAJA: Cell<(HBRUSH, u32)> = const { Cell::new((std::ptr::null_mut(), 0)) };
}

fn px(n: i32) -> i32 {
    n * barras::dpi() / 96
}

/// Una fila del panel: como la de la barra de botones, sin pasarse de su alto.
pub fn fila() -> i32 {
    px(36)
}

fn panel() -> HWND {
    app().buscador.get()
}

/// El alto que ocupa ahora: lo reparte `ventana::colocar` entre la barra y el texto.
pub fn alto() -> i32 {
    if panel().is_null() { 0 } else { ALTO.with(Cell::get) }
}

/// Abierto o abriéndose: la lupa de la barra se ve pulsada.
pub fn abierto() -> bool {
    META.with(Cell::get) > 0
}

fn noche() -> bool {
    app().cfg.borrow().noche
}

/// Dónde va cada cosa del panel con este ancho: los botones (por `Accion`, menos 1), las dos casillas y la cuenta.
struct Sitios {
    botones: [RECT; 7],
    que: RECT,
    por: RECT,
    cuenta: RECT,
}

fn ancho_texto(dc: HDC, f: HFONT, t: &[u16]) -> i32 {
    let mut s = SIZE { cx: 0, cy: 0 };
    unsafe {
        let antes = SelectObject(dc, f as _);
        if !t.is_empty() {
            GetTextExtentPoint32W(dc, t.as_ptr(), t.len() as i32, &mut s);
        }
        SelectObject(dc, antes);
    }
    s.cx
}

fn sitios(ancho: i32) -> Sitios {
    let (b, m) = (px(28), px(4));
    let (y0, y1) = ((fila() - b) / 2, fila() + (fila() - b) / 2);
    let r = |x: i32, y: i32, w: i32| RECT { left: x, top: y, right: x + w, bottom: y + b };
    let tt = t();
    let (uno, todos, cuenta) = unsafe {
        let dc = GetDC(std::ptr::null_mut());
        let f = barras::fuente_ui();
        let medidas = (
            ancho_texto(dc, f, &ws(tt.reemplazar_uno)) + px(20),
            ancho_texto(dc, f, &ws(tt.reemplazar_todo)) + px(20),
            CUENTA.with(|c| ancho_texto(dc, f, &c.borrow())),
        );
        ReleaseDC(std::ptr::null_mut(), dc);
        medidas
    };
    let x0 = m + b + m;
    // Las casillas encogen lo que haga falta para que los botones quepan en una ventana estrecha (M3).
    let caja = (ancho - x0 - (3 * b + m) - (b + 2 * m)).clamp(px(60), px(420));
    let caja_por = caja.min(ancho - x0 - uno - todos - 3 * m).max(px(60));
    let que = r(x0, y0, caja);
    let x = que.right + m;
    let botones = [r(m, y0, b), r(x, y0, b), r(x + b, y0, b), r(x + 2 * b, y0, b), r((ancho - m - b).max(x + 3 * b + m), y0, b), r(x0 + caja_por + m, y1, uno), r(x0 + caja_por + 2 * m + uno, y1, todos)];
    let cuenta = RECT { left: que.right - px(8) - cuenta, top: y0, right: que.right - px(8), bottom: y0 + b };
    Sitios { botones, que, por: r(x0, y1, caja_por), cuenta }
}

/// Las casillas dentro de su recuadro, centradas en alto; la de buscar deja sitio a la cuenta.
fn colocar_casillas() {
    let h = panel();
    let mut rc = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { GetClientRect(h, &mut rc) };
    let s = sitios(rc.right);
    let alto_letra = unsafe {
        let dc = GetDC(std::ptr::null_mut());
        let antes = SelectObject(dc, barras::fuente_ui() as _);
        let mut tm: TEXTMETRICW = std::mem::zeroed();
        GetTextMetricsW(dc, &mut tm);
        SelectObject(dc, antes);
        ReleaseDC(std::ptr::null_mut(), dc);
        tm.tmHeight
    };
    for (caja, r, derecha) in [(CAJA_QUE.with(Cell::get), s.que, s.cuenta.left - px(4)), (CAJA_POR.with(Cell::get), s.por, s.por.right - px(6))] {
        let y = (r.top + r.bottom - alto_letra) / 2;
        unsafe { MoveWindow(caja, r.left + px(6), y, (derecha - r.left - px(6)).max(px(20)), alto_letra, 1) };
    }
    unsafe { ShowWindow(CAJA_POR.with(Cell::get), if CON_POR.with(Cell::get) { SW_SHOWNA } else { SW_HIDE }) };
    AYUDAS.with(|a| {
        for (i, r) in s.botones.iter().enumerate() {
            let mut ti = herramienta(h, i, r);
            enviar(a.get(), TTM_NEWTOOLRECTW, 0, &mut ti as *mut _ as isize);
        }
    });
}

fn herramienta(h: HWND, i: usize, r: &RECT) -> TTTOOLINFOW {
    let mut ti: TTTOOLINFOW = unsafe { std::mem::zeroed() };
    ti.cbSize = std::mem::size_of::<TTTOOLINFOW>() as u32;
    ti.uFlags = TTF_SUBCLASS;
    ti.hwnd = h;
    ti.uId = i;
    ti.rect = *r;
    ti.lpszText = TEXTOS_AYUDA.with(|v| v.borrow()[i].as_ptr() as *mut u16);
    ti
}

fn crear() -> HWND {
    let a = app();
    let tt = t();
    unsafe {
        let ins = GetModuleHandleW(std::ptr::null());
        let clase = ws("BlocBuscar");
        let c = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(proc_panel),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: ins,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: clase.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&c);
        let h = CreateWindowExW(0, clase.as_ptr(), std::ptr::null(), WS_CHILD | WS_CLIPCHILDREN, 0, 0, 0, 0, a.hwnd.get(), std::ptr::null_mut(), ins, std::ptr::null());
        let edicion = ws("EDIT");
        for (id, celda, aviso) in [(QUE, &CAJA_QUE, tt.buscar_etq), (POR, &CAJA_POR, tt.reemplazar_etq)] {
            let caja = CreateWindowExW(0, edicion.as_ptr(), std::ptr::null(), WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL as u32, 0, 0, 0, 0, h, id as _, ins, std::ptr::null());
            enviar(caja, WM_SETFONT, barras::fuente_ui() as usize, 0);
            let aviso = ws(aviso.trim_end_matches([':', '：', ' ']));
            enviar(caja, 0x1501, 1, aviso.as_ptr() as isize); // EM_SETCUEBANNER, también con el foco dentro
            tema::nativo(caja, noche(), "DarkMode_CFD");
            celda.with(|c| c.set(caja));
        }
        TEXTOS_AYUDA.with(|v| {
            *v.borrow_mut() = [
                con_atajo(tt.reemplazar),
                con_atajo(tt.buscar_anterior),
                con_atajo(tt.buscar_siguiente),
                tt.mayusculas.to_string(),
                format!("{} (Esc)", tt.cerrar),
                tt.reemplazar_uno.to_string(),
                tt.reemplazar_todo.to_string(),
            ]
            .iter()
            .map(|s| ws(s))
            .collect()
        });
        let ayudas = CreateWindowExW(0, TOOLTIPS_CLASSW, std::ptr::null(), WS_POPUP | TTS_ALWAYSTIP | TTS_NOPREFIX, 0, 0, 0, 0, h, std::ptr::null_mut(), ins, std::ptr::null());
        AYUDAS.with(|x| x.set(ayudas));
        for i in 0..ACCIONES.len() {
            let mut ti = herramienta(h, i, &RECT { left: 0, top: 0, right: 0, bottom: 0 });
            enviar(ayudas, TTM_ADDTOOLW, 0, &mut ti as *mut _ as isize);
        }
        h
    }
}

/// Ctrl+F (`reemplazar` falso) y Ctrl+H: lo seleccionado, si es de una línea, pasa a la casilla de buscar.
pub fn mostrar(reemplazar: bool) {
    let a = app();
    if a.buscador.get().is_null() {
        a.buscador.set(crear());
    }
    let h = panel();
    let que = CAJA_QUE.with(Cell::get);
    let (s, e) = editor::seleccion(a.ed.get());
    if e > s && e - s <= 200 {
        let sel = ventana::trozo(s, e);
        if !sel.contains(&CR) {
            poner_texto(que, &sel);
        }
    }
    CON_POR.with(|c| c.set(c.get() || reemplazar));
    unsafe { ShowWindow(h, SW_SHOWNA) };
    colocar_casillas();
    animar(filas() * fila());
    contar();
    let casilla = if reemplazar && enviar(que, WM_GETTEXTLENGTH, 0, 0) > 0 { CAJA_POR.with(Cell::get) } else { que };
    unsafe { SetFocus(casilla) };
    enviar(casilla, EM_SETSEL, 0, -1);
    ventana::pulsados();
}

fn filas() -> i32 {
    if CON_POR.with(Cell::get) { 2 } else { 1 }
}

/// Esc o la ✕: se pliega y el cursor vuelve al texto.
fn cerrar() {
    HECHO.with(|c| c.replace(None));
    animar(0);
    unsafe { SetFocus(app().ed.get()) };
    ventana::pulsados();
}

/// Del alto de ahora a `meta`, frenando al final; de golpe si Windows tiene quitadas las animaciones.
fn animar(meta: i32) {
    let h = panel();
    let mut con_animaciones = 1i32;
    unsafe { SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, &mut con_animaciones as *mut _ as *mut _, 0) };
    META.with(|m| m.set(meta));
    if con_animaciones == 0 || ALTO.with(Cell::get) == meta {
        // Si se cambia de idea a mitad del despliegue, el reloj de antes seguiría con lo viejo.
        unsafe { KillTimer(h, RELOJ_PASO) };
        ALTO.with(|x| x.set(meta));
        acabar(h);
        return;
    }
    DESDE.with(|d| d.set(ALTO.with(Cell::get)));
    PASO.with(|p| p.set(0));
    unsafe { SetTimer(h, RELOJ_PASO, 15, None) };
}

fn paso(h: HWND) {
    let k = PASO.with(|p| {
        p.set(p.get() + 1);
        p.get()
    });
    let (desde, meta) = (DESDE.with(Cell::get), META.with(Cell::get));
    let frena = 1.0 - (1.0 - k as f32 / PASOS as f32).powi(2);
    ALTO.with(|x| x.set(desde + ((meta - desde) as f32 * frena).round() as i32));
    if k >= PASOS {
        unsafe { KillTimer(h, RELOJ_PASO) };
        acabar(h);
    } else {
        ventana::colocar();
    }
}

fn acabar(h: HWND) {
    ventana::colocar();
    if META.with(Cell::get) == 0 {
        unsafe { ShowWindow(h, SW_HIDE) };
    }
}

fn mayusculas() -> bool {
    MAYUS.with(Cell::get)
}

fn minusculas(s: &[u16]) -> Vec<u16> {
    let mut v = s.to_vec();
    unsafe { CharLowerBuffW(v.as_mut_ptr(), v.len() as u32) };
    v
}

/// Lo último buscado: en qué texto (y en minúsculas, si hizo falta), qué y dónde salía. Escribir en la casilla, contar
/// y pasar de una a otra no vuelven a recorrer un archivo grande mientras no cambie.
struct Hecho {
    texto: Vec<u16>,
    minusculas: Option<Vec<u16>>,
    buscado: Option<(Vec<u16>, bool)>,
    donde: Rc<Vec<usize>>,
}

thread_local! {
    static HECHO: RefCell<Option<Hecho>> = const { RefCell::new(None) };
}

/// Dónde empieza cada vez que sale `q` en el texto, sin solaparse (como al reemplazar todo).
fn coincidencias(q: &[u16]) -> Rc<Vec<usize>> {
    let t = ventana::texto();
    let mayus = mayusculas();
    HECHO.with(|c| {
        let mut c = c.borrow_mut();
        let h = match &mut *c {
            Some(h) if h.texto == t => h,
            c => c.insert(Hecho { texto: t, minusculas: None, buscado: None, donde: Rc::default() }),
        };
        if h.buscado.as_ref().is_none_or(|(b, m)| b != q || *m != mayus) {
            if !mayus && h.minusculas.is_none() {
                h.minusculas = Some(minusculas(&h.texto));
            }
            let (donde, q2) = match &h.minusculas {
                Some(m) if !mayus => (m, minusculas(q)),
                _ => (&h.texto, q.to_vec()),
            };
            let mut r = Vec::new();
            let mut i = 0;
            while !q2.is_empty() && i + q2.len() <= donde.len() {
                if donde[i..i + q2.len()] == q2[..] {
                    r.push(i);
                    i += q2.len();
                } else {
                    i += 1;
                }
            }
            h.donde = Rc::new(r);
            h.buscado = Some((q.to_vec(), mayus));
        }
        h.donde.clone()
    })
}

/// «3 de 12» (o «– de 12» si lo seleccionado no es una de ellas), «Sin resultados» o nada si no se busca nada.
fn contar() {
    let q = texto_ventana(CAJA_QUE.with(Cell::get));
    let cuenta = if q.is_empty() {
        Vec::new()
    } else {
        let donde = coincidencias(&q);
        let (s, e) = editor::seleccion(app().ed.get());
        let tt = t();
        if donde.is_empty() {
            ws(tt.sin_resultados)
        } else {
            let esta = donde.binary_search(&s).ok().filter(|_| e - s == q.len());
            ws(&con(&con(tt.de_total, esta.map_or("–".to_string(), |k| (k + 1).to_string())), donde.len()))
        }
    };
    if CUENTA.with(|c| c.replace(cuenta.clone())) != cuenta {
        // También como nombre del panel: lo leen los lectores de pantalla (y las pruebas).
        poner_texto(panel(), &cuenta);
        colocar_casillas();
        unsafe { InvalidateRect(panel(), std::ptr::null(), 0) };
    }
}

/// Lo escrito se busca al momento desde donde empieza la selección: escribir más letras alarga la misma.
fn al_escribir() {
    let q = texto_ventana(CAJA_QUE.with(Cell::get));
    if !q.is_empty() {
        let ed = app().ed.get();
        let (s, _) = editor::seleccion(ed);
        let donde = coincidencias(&q);
        if let Some(&i) = donde.iter().find(|&&i| i >= s).or(donde.first()) {
            editor::seleccionar(ed, i, i + q.len());
            ventana::ver_cursor();
            ventana::estado();
        }
    }
    contar();
}

/// F3, Mayús+F3, Intro y las flechas del panel: la siguiente o la anterior, dando la vuelta.
pub fn buscar(adelante: bool) {
    let a = app();
    let q = if a.buscador.get().is_null() { Vec::new() } else { texto_ventana(CAJA_QUE.with(Cell::get)) };
    if q.is_empty() {
        mostrar(false);
        return;
    }
    let ed = a.ed.get();
    let (s, e) = editor::seleccion(ed);
    let donde = coincidencias(&q);
    let (r, vuelta) = if adelante {
        match donde.iter().find(|&&i| i >= e) {
            Some(&i) => (Some(i), ""),
            None => (donde.first().copied(), t().vuelta_principio),
        }
    } else {
        match donde.iter().rev().find(|&&i| i < s) {
            Some(&i) => (Some(i), ""),
            None => (donde.last().copied(), t().vuelta_final),
        }
    };
    let Some(i) = r else {
        ventana::poner_mensaje(&con(t().no_encuentra, String::from_utf16_lossy(&q)));
        unsafe { MessageBeep(MB_OK) };
        contar();
        return;
    };
    editor::seleccionar(ed, i, i + q.len());
    ventana::ver_cursor();
    ventana::poner_mensaje(vuelta);
    ventana::estado();
    contar();
}

pub fn reemplazar() {
    let q = texto_ventana(CAJA_QUE.with(Cell::get));
    if q.is_empty() {
        return;
    }
    let ed = app().ed.get();
    let (s, e) = editor::seleccion(ed);
    let sel = ventana::trozo(s, e);
    let igual = if mayusculas() { sel == q } else { minusculas(&sel) == minusculas(&q) };
    if igual {
        editor::sustituir(ed, &texto_ventana(CAJA_POR.with(Cell::get)));
    }
    buscar(true);
}

pub fn reemplazar_todo() {
    let (q, por) = (texto_ventana(CAJA_QUE.with(Cell::get)), texto_ventana(CAJA_POR.with(Cell::get)));
    if q.is_empty() {
        return;
    }
    let t0 = ventana::texto();
    let donde = coincidencias(&q);
    if donde.is_empty() {
        ventana::poner_mensaje(&con(t().no_encuentra, String::from_utf16_lossy(&q)));
        unsafe { MessageBeep(MB_OK) };
        return;
    }
    let mut r = Vec::with_capacity(t0.len());
    let mut i = 0;
    for &d in donde.iter() {
        r.extend_from_slice(&t0[i..d]);
        r.extend_from_slice(&por);
        i = d + q.len();
    }
    r.extend_from_slice(&t0[i..]);
    let ed = app().ed.get();
    let (pos, _) = editor::seleccion(ed);
    // Solo de la primera sustitución a la última: lo metido coge la letra de donde cae, y en un Markdown el resto
    // se quedaría con la del primer carácter.
    if let Some((p, s)) = crate::historial::diferencia(&t0, &r) {
        editor::seleccionar(ed, p, t0.len() - s);
        editor::sustituir(ed, &r[p..r.len() - s]);
    }
    let p = pos.min(r.len());
    editor::seleccionar(ed, p, p);
    editor::ver_cursor(ed);
    let n = donde.len();
    ventana::poner_mensaje(&if n == 1 { t().una_sustitucion.to_string() } else { con(t().sustituciones, n) });
    contar();
}

/// El texto ha cambiado con el panel abierto: se vuelve a contar un poco después, no en cada tecla (en un archivo
/// grande, contar recorre todo el texto).
pub fn cambiado() {
    let h = panel();
    if !h.is_null() && abierto() {
        unsafe { SetTimer(h, RELOJ_CONTAR, 300, None) };
    }
}

/// Al cambiar de modo.
pub fn pintar() {
    let h = panel();
    if !h.is_null() {
        for c in [CAJA_QUE.with(Cell::get), CAJA_POR.with(Cell::get)] {
            tema::nativo(c, noche(), "DarkMode_CFD");
        }
        unsafe { InvalidateRect(h, std::ptr::null(), 1) };
    }
}

fn hacer(accion: Accion) {
    match accion {
        Accion::Desplegar => {
            CON_POR.with(|c| c.set(!c.get()));
            colocar_casillas();
            animar(filas() * fila());
            unsafe { SetFocus(if CON_POR.with(Cell::get) { CAJA_POR.with(Cell::get) } else { CAJA_QUE.with(Cell::get) }) };
        }
        Accion::Anterior => buscar(false),
        Accion::Siguiente => buscar(true),
        Accion::Mayus => {
            MAYUS.with(|m| m.set(!m.get()));
            contar();
        }
        Accion::Cerrar => cerrar(),
        Accion::Uno => reemplazar(),
        Accion::Todos => reemplazar_todo(),
    }
    unsafe { InvalidateRect(panel(), std::ptr::null(), 0) };
}

/// Las teclas dentro del panel: Intro y Mayús+Intro (en la de reemplazar, reemplaza), Esc, Tab entre las casillas y
/// F3, Ctrl+F y Ctrl+H como en el texto.
pub fn tecla(m: &MSG) -> bool {
    if m.message != WM_KEYDOWN {
        return false;
    }
    let mayus = pulsada(VK_SHIFT);
    let en_por = m.hwnd == CAJA_POR.with(Cell::get);
    match m.wParam as u16 {
        VK_RETURN if en_por && !mayus => reemplazar(),
        VK_RETURN => buscar(!mayus),
        VK_F3 => buscar(!mayus),
        VK_ESCAPE => cerrar(),
        VK_TAB if CON_POR.with(Cell::get) => unsafe {
            SetFocus(if en_por { CAJA_QUE.with(Cell::get) } else { CAJA_POR.with(Cell::get) });
        },
        VK_TAB => {}
        k if pulsada(VK_CONTROL) && (k == b'F' as u16 || k == b'H' as u16) => mostrar(k == b'H' as u16),
        k if pulsada(VK_CONTROL) && k == b'A' as u16 => {
            enviar(m.hwnd, EM_SETSEL, 0, -1);
        }
        _ => return false,
    }
    true
}

/// Ctrl+C, Ctrl+X, Ctrl+V, Ctrl+Z y Ctrl+Y en una casilla son de la casilla, no del texto.
pub fn de_casilla(m: &MSG) -> bool {
    m.message == WM_KEYDOWN && pulsada(VK_CONTROL) && b"CXVZY".iter().any(|&k| m.wParam == k as usize)
}

unsafe extern "system" fn proc_panel(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    seguro(|| mensaje_panel(h, m, wp, lp), || 0)
}

fn boton_en(h: HWND, lp: LPARAM) -> i32 {
    let (x, y) = ((lp & 0xFFFF) as i16 as i32, ((lp >> 16) & 0xFFFF) as i16 as i32);
    let mut rc = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { GetClientRect(h, &mut rc) };
    let s = sitios(rc.right);
    let visibles = if CON_POR.with(Cell::get) { 7 } else { 5 };
    s.botones[..visibles].iter().position(|r| x >= r.left && x < r.right && y >= r.top && y < r.bottom).map_or(-1, |i| i as i32)
}

fn mensaje_panel(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = unsafe { std::mem::zeroed() };
            let dc = unsafe { BeginPaint(h, &mut ps) };
            pintar_panel(h, dc);
            unsafe { EndPaint(h, &ps) };
            0
        }
        WM_ERASEBKGND => 1,
        // Mientras se despliega solo cambia el alto, y lo de dentro sigue en su sitio. El de cuando se crea, sin nada
        // dentro todavía, no cuenta: lo coloca `mostrar`.
        WM_SIZE => {
            if !panel().is_null() && ANCHO.with(|a| a.replace(lp & 0xFFFF)) != lp & 0xFFFF {
                colocar_casillas();
            }
            0
        }
        WM_TIMER if wp == RELOJ_PASO => {
            paso(h);
            0
        }
        WM_TIMER if wp == RELOJ_CONTAR => {
            unsafe { KillTimer(h, RELOJ_CONTAR) };
            contar();
            0
        }
        WM_COMMAND if lp == 0 => {
            if let Some(&a) = ACCIONES.get((wp & 0xFFFF).wrapping_sub(1)) {
                hacer(a);
            }
            0
        }
        WM_COMMAND => {
            match ((wp & 0xFFFF) as i32, (wp >> 16) as u32) {
                (QUE, EN_CHANGE) => al_escribir(),
                (_, EN_SETFOCUS | EN_KILLFOCUS) => unsafe {
                    InvalidateRect(h, std::ptr::null(), 0);
                },
                _ => {}
            }
            0
        }
        WM_CTLCOLOREDIT => {
            let p = tema::paleta(noche());
            let caja = PINCEL_CAJA.with(|c| {
                let (b, color) = c.get();
                if b.is_null() || color != p.fondo {
                    if !b.is_null() {
                        unsafe { DeleteObject(b as _) };
                    }
                    c.set((unsafe { CreateSolidBrush(p.fondo) }, p.fondo));
                }
                c.get().0
            });
            unsafe {
                SetTextColor(wp as HDC, p.texto);
                SetBkColor(wp as HDC, p.fondo);
            }
            caja as LRESULT
        }
        WM_MOUSEMOVE => {
            let i = boton_en(h, lp);
            if i != ENCIMA.with(Cell::get) {
                ENCIMA.with(|e| e.set(i));
                unsafe { InvalidateRect(h, std::ptr::null(), 0) };
                let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: h, dwHoverTime: 0 };
                unsafe { TrackMouseEvent(&mut t) };
            }
            0
        }
        WM_MOUSELEAVE => {
            ENCIMA.with(|e| e.set(-1));
            unsafe { InvalidateRect(h, std::ptr::null(), 0) };
            0
        }
        WM_LBUTTONDOWN => {
            PULSADO.with(|p| p.set(boton_en(h, lp)));
            unsafe {
                SetCapture(h);
                InvalidateRect(h, std::ptr::null(), 0);
            }
            0
        }
        WM_LBUTTONUP => {
            let antes = PULSADO.with(|p| p.replace(-1));
            unsafe {
                ReleaseCapture();
                InvalidateRect(h, std::ptr::null(), 0);
            }
            if antes >= 0 && boton_en(h, lp) == antes {
                hacer(ACCIONES[antes as usize]);
            }
            0
        }
        _ => unsafe { DefWindowProcW(h, m, wp, lp) },
    }
}

fn rellenar(dc: HDC, r: &RECT, color: u32) {
    unsafe {
        let b = CreateSolidBrush(color);
        FillRect(dc, r, b);
        DeleteObject(b as _);
    }
}

fn escribir(dc: HDC, f: HFONT, t: &[u16], mut r: RECT, formato: u32) {
    if t.is_empty() {
        return;
    }
    unsafe {
        let antes = SelectObject(dc, f as _);
        DrawTextW(dc, t.as_ptr(), t.len() as i32, &mut r, formato | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
        SelectObject(dc, antes);
    }
}

fn pintar_panel(h: HWND, dc: HDC) {
    let noche = noche();
    let p = tema::paleta(noche);
    let mut rc = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { GetClientRect(h, &mut rc) };
    let s = sitios(rc.right);
    let con_por = CON_POR.with(Cell::get);
    let foco = unsafe { GetFocus() };
    // Se pinta aparte y se copia de una vez: sin parpadeo al pasar el ratón ni durante el despliegue.
    unsafe {
        let mem = CreateCompatibleDC(dc);
        let bmp = CreateCompatibleBitmap(dc, rc.right.max(1), rc.bottom.max(1));
        let viejo = SelectObject(mem, bmp as _);
        rellenar(mem, &rc, p.barra);
        rellenar(mem, &RECT { top: rc.bottom - 1, ..rc }, p.borde);
        let mut cajas = vec![(s.que, CAJA_QUE.with(Cell::get))];
        if con_por {
            cajas.push((s.por, CAJA_POR.with(Cell::get)));
        }
        for (r, caja) in cajas {
            rellenar(mem, &r, if foco == caja { p.apagado } else { p.borde });
            rellenar(mem, &RECT { left: r.left + 1, top: r.top + 1, right: r.right - 1, bottom: r.bottom - 1 }, p.fondo);
        }
        SetBkMode(mem, TRANSPARENT as i32);
        SetTextColor(mem, p.apagado);
        CUENTA.with(|c| escribir(mem, barras::fuente_ui(), &c.borrow(), s.cuenta, DT_RIGHT));
        let tt = t();
        let color = tema::texto_barra(noche);
        let (encima, pulsado) = (ENCIMA.with(Cell::get), PULSADO.with(Cell::get));
        for (i, (r, accion)) in s.botones.iter().zip(ACCIONES).enumerate() {
            if !con_por && matches!(accion, Accion::Uno | Accion::Todos) {
                continue;
            }
            let i = i as i32;
            if i == pulsado || (accion == Accion::Mayus && mayusculas()) {
                rellenar(mem, r, p.borde);
            } else if i == encima {
                rellenar(mem, r, p.resalte);
            }
            SetTextColor(mem, color);
            let (letra, texto) = match accion {
                Accion::Desplegar => (barras::fuente_iconos(), vec![if con_por { 0xE70D } else { 0xE76C }]),
                Accion::Anterior => (barras::fuente_iconos(), vec![0xE74A]),
                Accion::Siguiente => (barras::fuente_iconos(), vec![0xE74B]),
                Accion::Mayus => (barras::fuente_ui(), ws("Aa")),
                Accion::Cerrar => (barras::fuente_iconos(), vec![0xE711]),
                Accion::Uno => (barras::fuente_ui(), ws(tt.reemplazar_uno)),
                Accion::Todos => (barras::fuente_ui(), ws(tt.reemplazar_todo)),
            };
            if matches!(accion, Accion::Uno | Accion::Todos) && i != pulsado && i != encima {
                // Los de texto, con su recuadro: se leen como botones.
                rellenar(mem, r, p.borde);
                rellenar(mem, &RECT { left: r.left + 1, top: r.top + 1, right: r.right - 1, bottom: r.bottom - 1 }, p.barra);
            }
            escribir(mem, letra, &texto, *r, DT_CENTER);
        }
        BitBlt(dc, 0, 0, rc.right, rc.bottom, mem, 0, 0, SRCCOPY);
        SelectObject(mem, viejo);
        DeleteObject(bmp as _);
        DeleteDC(mem);
    }
}

// --- Ir a la línea ---

unsafe extern "system" fn proc_ir(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> isize {
    seguro(|| mensaje_ir(h, m, wp, lp), || 0)
}

fn mensaje_ir(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> isize {
    match m {
        WM_INITDIALOG => {
            let total = lp as usize;
            poner_texto(unsafe { GetDlgItem(h, ETIQUETA) }, &ws(&con(t().ir_etiqueta, total)));
            poner_texto(unsafe { GetDlgItem(h, NUMERO) }, &ws(&LINEA.with(Cell::get).to_string()));
            tema_dialogo(h);
            let n = unsafe { GetDlgItem(h, NUMERO) };
            enviar(n, EM_SETSEL, 0, -1);
            unsafe { SetFocus(n) };
            0
        }
        WM_COMMAND => {
            match (wp & 0xFFFF) as i32 {
                1 => {
                    let n: usize = String::from_utf16_lossy(&texto_ventana(unsafe { GetDlgItem(h, NUMERO) })).trim().parse().unwrap_or(0);
                    LINEA.with(|l| l.set(n));
                    unsafe { EndDialog(h, 1) };
                }
                2 => unsafe {
                    EndDialog(h, 0);
                },
                _ => {}
            }
            1
        }
        WM_CTLCOLORDLG | WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORBTN if app().cfg.borrow().noche => color_control(m, wp as HDC),
        _ => 0,
    }
}

pub fn ir_a_linea() {
    let a = app();
    let ed = a.ed.get();
    let texto = ventana::texto();
    let total = texto.iter().filter(|&&c| c == CR).count() + 1;
    let (s, _) = editor::seleccion(ed);
    let actual = texto[..s.min(texto.len())].iter().filter(|&&c| c == CR).count() + 1;
    LINEA.with(|l| l.set(actual));
    let t = t();
    let mut p = Plantilla::nueva(&limpio(t.ir_a_linea), 160, 62, 0);
    p.control(ESTATICO, "", ETIQUETA, 0, 7, 7, 146, 9);
    p.control(EDICION, "", NUMERO, WS_BORDER | WS_TABSTOP | ES_NUMBER as u32 | ES_AUTOHSCROLL as u32, 7, 19, 146, 13);
    p.control(BOTON, t.aceptar, 1, WS_TABSTOP | BS_DEFPUSHBUTTON as u32, 47, 40, 50, 15);
    p.control(BOTON, t.cancelar, 2, WS_TABSTOP | BS_PUSHBUTTON as u32, 103, 40, 50, 15);
    let plantilla = p.hecha();
    let r = unsafe { DialogBoxIndirectParamW(GetModuleHandleW(std::ptr::null()), plantilla.as_ptr() as *const DLGTEMPLATE, a.hwnd.get(), Some(proc_ir), total as isize) };
    if r != 1 {
        return;
    }
    let n = LINEA.with(Cell::get).clamp(1, total);
    let pos = texto.split(|&c| c == CR).take(n - 1).map(|l| l.len() + 1).sum::<usize>();
    editor::seleccionar(ed, pos, pos);
    ventana::ver_cursor();
    ventana::estado();
}
