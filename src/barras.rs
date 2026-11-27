// Las dos barras de Bloc: la de formato (arriba, solo en los Markdown) y la de estado (abajo). Las pinta Bloc: así
// se ven igual en claro y en oscuro, y al teclear solo se vuelve a pintar la parte que cambia.
use std::cell::{Cell, RefCell};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::{WM_MOUSELEAVE, TOOLTIPS_CLASSW, TTF_SUBCLASS, TTM_ADDTOOLW, TTM_NEWTOOLRECTW, TTS_ALWAYSTIP, TTS_NOPREFIX, TTTOOLINFOW};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::tema;
use crate::w::{enviar, seguro, ws};

/// Lo que lleva un botón: un símbolo de la letra de iconos de Windows (la del Bloc de notas), o uno dibujado aquí para
/// lo que esa letra no trae.
#[derive(Clone, Copy)]
pub enum Icono {
    Simbolo(u16),
    Titulo(u8),
    Numerada,
    Cita,
}

pub struct Boton {
    pub id: u16,
    pub icono: Icono,
    pub ayuda: Vec<u16>,
    /// Empieza un grupo nuevo: lleva una raya delante.
    pub grupo: bool,
    /// Va a la derecha del todo de la primera fila (la lupa de buscar).
    derecha: bool,
    rect: RECT,
    /// Lo seleccionado ya tiene este formato: se ve pulsado.
    activo: bool,
}

impl Boton {
    pub fn nuevo(id: u16, icono: Icono, ayuda: String, grupo: bool) -> Boton {
        Boton { id, icono, ayuda: ws(&ayuda), grupo, derecha: false, rect: RECT { left: 0, top: 0, right: 0, bottom: 0 }, activo: false }
    }

    pub fn a_la_derecha(self) -> Boton {
        Boton { derecha: true, ..self }
    }
}

/// Los botones de `ids` se ven pulsados; los demás, no. Solo se repintan los que cambian.
pub fn pulsados(h: HWND, ids: &[u16]) {
    for b in barras().botones.borrow_mut().iter_mut() {
        let si = ids.contains(&b.id);
        if b.activo != si {
            b.activo = si;
            unsafe { InvalidateRect(h, &b.rect, 0) };
        }
    }
}

// Las letras de las barras, por su puesto en `Barras::fuentes`.
const UI: usize = 0;
const ICONOS: usize = 1;
const TITULO: usize = 2;
const CIFRAS: usize = 3;
const COMILLAS: usize = 4;

pub const PARTES: usize = 5; // mensaje, posición, zoom, saltos, codificación

pub struct Barras {
    pub botones: RefCell<Vec<Boton>>,
    /// El botón «»» con los que no caben en la fila (vacío si caben todos).
    mas: Cell<RECT>,
    encima: Cell<i32>,
    pulsado: Cell<i32>,
    partes: RefCell<[Vec<u16>; PARTES]>,
    rects: RefCell<[RECT; PARTES]>,
    fuentes: RefCell<Vec<HFONT>>,
    pub ayudas: Cell<HWND>,
    pub noche: Cell<bool>,
}

impl Barras {
    pub fn nuevas() -> Barras {
        Barras {
            botones: RefCell::new(Vec::new()),
            mas: Cell::new(VACIO),
            encima: Cell::new(-1),
            pulsado: Cell::new(-1),
            partes: RefCell::new(Default::default()),
            rects: RefCell::new([RECT { left: 0, top: 0, right: 0, bottom: 0 }; PARTES]),
            fuentes: RefCell::new(Vec::new()),
            ayudas: Cell::new(std::ptr::null_mut()),
            noche: Cell::new(false),
        }
    }
}

fn barras() -> &'static Barras {
    &crate::ventana::app().barras
}

/// Los puntos por pulgada de la pantalla: con el manifiesto (dpiAware) no cambian mientras dura el proceso.
pub fn dpi() -> i32 {
    static DPI: std::sync::OnceLock<i32> = std::sync::OnceLock::new();
    *DPI.get_or_init(|| unsafe {
        let dc = GetDC(std::ptr::null_mut());
        let d = GetDeviceCaps(dc, LOGPIXELSY as i32);
        ReleaseDC(std::ptr::null_mut(), dc);
        d
    })
}

fn px(n: i32) -> i32 {
    n * dpi() / 96
}

/// La letra de la interfaz de Windows, la de iconos y las de los iconos dibujados aquí.
pub fn crear_fuentes() {
    let mut ncm: NONCLIENTMETRICSW = unsafe { std::mem::zeroed() };
    ncm.cbSize = std::mem::size_of::<NONCLIENTMETRICSW>() as u32;
    unsafe { SystemParametersInfoW(SPI_GETNONCLIENTMETRICS, ncm.cbSize, &mut ncm as *mut _ as *mut _, 0) };
    let base = ncm.lfMessageFont;
    let letra = |nombre: &str, alto: i32, peso: i32| {
        let mut l = base;
        l.lfHeight = -px(alto);
        l.lfWeight = peso;
        if !nombre.is_empty() {
            l.lfFaceName = [0; 32];
            for (i, c) in nombre.encode_utf16().take(31).enumerate() {
                l.lfFaceName[i] = c;
            }
        }
        unsafe { CreateFontIndirectW(&l) }
    };
    // Windows 11 trae Segoe Fluent Icons; Windows 10, Segoe MDL2 Assets, con los mismos símbolos.
    let iconos = letra("Segoe Fluent Icons", 16, 400);
    let iconos = if nombre_de(iconos) == "Segoe Fluent Icons" {
        iconos
    } else {
        unsafe { DeleteObject(iconos as _) };
        letra("Segoe MDL2 Assets", 16, 400)
    };
    *barras().fuentes.borrow_mut() =
        vec![unsafe { CreateFontIndirectW(&base) }, iconos, letra("", 14, 600), letra("", 8, 700), letra("Georgia", 28, 700)];
}

/// La letra que da Windows de verdad (si no tiene la pedida, pone otra).
fn nombre_de(f: HFONT) -> String {
    let mut b = [0u16; 32];
    unsafe {
        let dc = GetDC(std::ptr::null_mut());
        let antes = SelectObject(dc, f as _);
        let n = GetTextFaceW(dc, b.len() as i32, b.as_mut_ptr());
        SelectObject(dc, antes);
        ReleaseDC(std::ptr::null_mut(), dc);
        String::from_utf16_lossy(&b[..(n.max(1) - 1) as usize])
    }
}

pub fn fuente_ui() -> HFONT {
    barras().fuentes.borrow()[UI]
}

pub fn fuente_iconos() -> HFONT {
    barras().fuentes.borrow()[ICONOS]
}

fn medir(dc: HDC, t: &[u16]) -> SIZE {
    let mut s = SIZE { cx: 0, cy: 0 };
    if !t.is_empty() {
        unsafe { GetTextExtentPoint32W(dc, t.as_ptr(), t.len() as i32, &mut s) };
    }
    s
}

type Proc = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;

pub fn registrar() {
    for (nombre, proc_) in [("BlocBotones", botones_proc as Proc), ("BlocEstado", estado_proc as Proc)] {
        let n = ws(nombre);
        let c = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW,
            lpfnWndProc: Some(proc_),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: unsafe { GetModuleHandleW(std::ptr::null()) },
            hIcon: std::ptr::null_mut(),
            hCursor: unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) },
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: n.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        unsafe { RegisterClassExW(&c) };
    }
}

pub fn crear(padre: HWND, clase: &str, id: usize) -> HWND {
    let c = ws(clase);
    unsafe { CreateWindowExW(0, c.as_ptr(), std::ptr::null(), WS_CHILD, 0, 0, 0, 0, padre, id as _, GetModuleHandleW(std::ptr::null()), std::ptr::null()) }
}

fn pincel(color: u32) -> HBRUSH {
    unsafe { CreateSolidBrush(color) }
}

fn rellenar(dc: HDC, r: &RECT, color: u32) {
    let b = pincel(color);
    unsafe {
        FillRect(dc, r, b);
        DeleteObject(b as _);
    }
}

// --- Barra de formato ---

const VACIO: RECT = RECT { left: 0, top: 0, right: 0, bottom: 0 };
/// El número que tiene «»» donde los botones tienen el suyo (encima, pulsado).
const MAS: i32 = -2;

/// Coloca los botones en una fila para este ancho y devuelve su alto. Los que no caben van al menú de «»», en su orden:
/// la barra no crece hacia abajo (M3).
pub fn alto_botones(ancho: i32) -> i32 {
    let b = barras();
    let mut v = b.botones.borrow_mut();
    let (w, alto, raya, borde) = (px(34), px(30), px(9), px(3));
    // Los de la derecha, de derecha a izquierda; los demás, con lo que queda.
    let mut d = ancho - borde;
    for boton in v.iter_mut().rev().filter(|b| b.derecha) {
        d -= w;
        boton.rect = RECT { left: d, top: borde, right: d + w, bottom: borde + alto };
    }
    let fila = |v: &mut Vec<Boton>, limite: i32| {
        let (mut x, mut todos) = (borde, true);
        for boton in v.iter_mut().filter(|b| !b.derecha) {
            let izq = if boton.grupo && x > borde { x + raya } else { x };
            todos &= izq + w <= limite;
            boton.rect = if todos { RECT { left: izq, top: borde, right: izq + w, bottom: borde + alto } } else { VACIO };
            if todos {
                x = izq + w;
            }
        }
        (todos, x)
    };
    b.mas.set(VACIO);
    if !fila(&mut v, d).0 {
        let (_, x) = fila(&mut v, d - w);
        b.mas.set(RECT { left: x, top: borde, right: x + w, bottom: borde + alto });
    }
    let dueno = crate::ventana::app().botones.get();
    for (i, boton) in v.iter().enumerate() {
        let mut ti = herramienta(dueno, i, boton);
        enviar(b.ayudas.get(), TTM_NEWTOOLRECTW, 0, &mut ti as *mut _ as isize);
    }
    alto + 2 * borde + 1
}

/// El menú de «»»: los botones que no caben, con su ayuda y marcados si lo seleccionado ya tiene su formato.
fn menu_mas(h: HWND) {
    let r = barras().mas.get();
    let quedan: Vec<(u16, Vec<u16>, bool)> =
        barras().botones.borrow().iter().filter(|b| !b.derecha && b.rect.right == 0).map(|b| (b.id, b.ayuda.clone(), b.activo)).collect();
    unsafe {
        let m = CreatePopupMenu();
        for (id, ayuda, activo) in &quedan {
            AppendMenuW(m, MF_STRING | if *activo { MF_CHECKED } else { MF_UNCHECKED }, *id as usize, ayuda.as_ptr());
        }
        let mut p = POINT { x: r.left, y: r.bottom };
        ClientToScreen(h, &mut p);
        let id = TrackPopupMenu(m, TPM_RETURNCMD | TPM_LEFTALIGN | TPM_TOPALIGN, p.x, p.y, 0, h, std::ptr::null());
        DestroyMenu(m);
        if id != 0 {
            enviar(GetParent(h), WM_COMMAND, id as usize, 0);
        }
    }
}

fn herramienta(dueno: HWND, i: usize, boton: &Boton) -> TTTOOLINFOW {
    let mut ti: TTTOOLINFOW = unsafe { std::mem::zeroed() };
    ti.cbSize = std::mem::size_of::<TTTOOLINFOW>() as u32;
    ti.uFlags = TTF_SUBCLASS;
    ti.hwnd = dueno;
    ti.uId = i;
    ti.rect = boton.rect;
    ti.lpszText = boton.ayuda.as_ptr() as *mut u16;
    ti
}

/// Pone los botones y sus ayudas.
pub fn poner_botones(dueno: HWND, v: Vec<Boton>) {
    let b = barras();
    let ayudas = unsafe {
        CreateWindowExW(0, TOOLTIPS_CLASSW, std::ptr::null(), WS_POPUP | TTS_ALWAYSTIP | TTS_NOPREFIX, 0, 0, 0, 0, dueno, std::ptr::null_mut(), GetModuleHandleW(std::ptr::null()), std::ptr::null())
    };
    b.ayudas.set(ayudas);
    *b.botones.borrow_mut() = v;
    for (i, boton) in b.botones.borrow().iter().enumerate() {
        let mut ti = herramienta(dueno, i, boton);
        enviar(ayudas, TTM_ADDTOOLW, 0, &mut ti as *mut _ as isize);
    }
}

fn dentro(r: &RECT, x: i32, y: i32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

fn boton_en(x: i32, y: i32) -> i32 {
    if dentro(&barras().mas.get(), x, y) {
        return MAS;
    }
    barras().botones.borrow().iter().position(|b| dentro(&b.rect, x, y)).map_or(-1, |i| i as i32)
}

fn invalidar_boton(h: HWND, i: i32) {
    if i == MAS {
        unsafe { InvalidateRect(h, &barras().mas.get(), 0) };
    } else if i >= 0
        && let Some(b) = barras().botones.borrow().get(i as usize) {
            unsafe { InvalidateRect(h, &b.rect, 0) };
        }
}

unsafe extern "system" fn botones_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    seguro(|| botones_mensaje(h, m, wp, lp), || 0)
}

fn botones_mensaje(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let b = barras();
    let (x, y) = ((lp & 0xFFFF) as i16 as i32, ((lp >> 16) & 0xFFFF) as i16 as i32);
    match m {
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = unsafe { std::mem::zeroed() };
            let dc = unsafe { BeginPaint(h, &mut ps) };
            pintar_botones(h, dc);
            unsafe { EndPaint(h, &ps) };
            0
        }
        WM_ERASEBKGND => 1,
        WM_MOUSEMOVE => {
            let i = boton_en(x, y);
            if i != b.encima.get() {
                invalidar_boton(h, b.encima.get());
                b.encima.set(i);
                invalidar_boton(h, i);
                let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: h, dwHoverTime: 0 };
                unsafe { TrackMouseEvent(&mut t) };
            }
            0
        }
        WM_MOUSELEAVE => {
            invalidar_boton(h, b.encima.get());
            b.encima.set(-1);
            0
        }
        WM_LBUTTONDOWN => {
            let i = boton_en(x, y);
            b.pulsado.set(i);
            invalidar_boton(h, i);
            unsafe { SetCapture(h) };
            0
        }
        WM_LBUTTONUP => {
            let antes = b.pulsado.replace(-1);
            unsafe { ReleaseCapture() };
            invalidar_boton(h, antes);
            if antes == MAS && boton_en(x, y) == MAS {
                menu_mas(h);
            } else if antes >= 0 && boton_en(x, y) == antes {
                let id = b.botones.borrow()[antes as usize].id;
                enviar(unsafe { GetParent(h) }, WM_COMMAND, id as usize, 0);
            }
            0
        }
        _ => unsafe { DefWindowProcW(h, m, wp, lp) },
    }
}

fn pintar_botones(h: HWND, dc: HDC) {
    let b = barras();
    let p = tema::paleta(b.noche.get());
    let mut rc = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { GetClientRect(h, &mut rc) };
    // Se pinta aparte y se copia de una vez: sin parpadeo al pasar el ratón.
    unsafe {
        let mem = CreateCompatibleDC(dc);
        let bmp = CreateCompatibleBitmap(dc, rc.right.max(1), rc.bottom.max(1));
        let viejo = SelectObject(mem, bmp as _);
        rellenar(mem, &rc, p.barra);
        rellenar(mem, &RECT { top: rc.bottom - 1, ..rc }, p.borde);
        SetBkMode(mem, TRANSPARENT as i32);
        let fuentes = b.fuentes.borrow();
        for (i, boton) in b.botones.borrow().iter().enumerate().filter(|(_, b)| b.rect.right > 0) {
            let i = i as i32;
            if i == b.pulsado.get() || boton.activo {
                rellenar(mem, &boton.rect, p.borde);
            } else if i == b.encima.get() {
                rellenar(mem, &boton.rect, p.resalte);
            }
            if boton.grupo && boton.rect.left > px(3) {
                let x = boton.rect.left - px(5);
                rellenar(mem, &RECT { left: x, right: x + 1, top: boton.rect.top + px(5), bottom: boton.rect.bottom - px(5) }, p.borde);
            }
            dibujar(mem, boton.icono, boton.rect, &fuentes, tema::texto_barra(b.noche.get()));
        }
        let mas = b.mas.get();
        if mas.right > 0 {
            if b.pulsado.get() == MAS {
                rellenar(mem, &mas, p.borde);
            } else if b.encima.get() == MAS {
                rellenar(mem, &mas, p.resalte);
            }
            SetTextColor(mem, tema::texto_barra(b.noche.get()));
            escribir(mem, fuentes[TITULO], &[0xBB], mas);
        }
        BitBlt(dc, 0, 0, rc.right, rc.bottom, mem, 0, 0, SRCCOPY);
        SelectObject(mem, viejo);
        DeleteObject(bmp as _);
        DeleteDC(mem);
    }
}

fn escribir(dc: HDC, f: HFONT, t: &[u16], mut r: RECT) {
    unsafe {
        let antes = SelectObject(dc, f as _);
        DrawTextW(dc, t.as_ptr(), t.len() as i32, &mut r, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
        SelectObject(dc, antes);
    }
}

fn dibujar(dc: HDC, icono: Icono, r: RECT, f: &[HFONT], color: u32) {
    unsafe { SetTextColor(dc, color) };
    match icono {
        Icono::Simbolo(c) => escribir(dc, f[ICONOS], &[c], r),
        Icono::Titulo(n) => escribir(dc, f[TITULO], &[b'H' as u16, (b'0' + n) as u16], r),
        // Las comillas se dibujan arriba de su renglón: se bajan para que queden en medio.
        Icono::Cita => escribir(dc, f[COMILLAS], &[0x201C], RECT { top: r.top + px(6), bottom: r.bottom + px(6), ..r }),
        // Como la lista de viñetas de la letra de iconos, con 1, 2 y 3 en vez de puntos.
        Icono::Numerada => {
            let (cx, cy) = ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
            for i in 0..3 {
                let y = cy + (i - 1) * px(6);
                let cifra = RECT { left: cx - px(10), right: cx - px(3), top: y - px(6), bottom: y + px(6) };
                escribir(dc, f[CIFRAS], &[(b'1' + i as u8) as u16], cifra);
                rellenar(dc, &RECT { left: cx - px(1), right: cx + px(8), top: y, bottom: y + px(1).max(1) }, color);
            }
        }
    }
}

// --- Barra de estado ---

pub fn alto_estado() -> i32 {
    let alto_letra = unsafe {
        let dc = GetDC(std::ptr::null_mut());
        let antes = SelectObject(dc, fuente_ui() as _);
        let s = medir(dc, &[b'M' as u16]);
        SelectObject(dc, antes);
        ReleaseDC(std::ptr::null_mut(), dc);
        s.cy
    };
    alto_letra + px(8)
}

/// Pone el texto de una parte y vuelve a pintar solo esa (o toda la barra si cambia de ancho).
pub fn poner(h: HWND, parte: usize, t: &str) {
    let b = barras();
    let v: Vec<u16> = t.encode_utf16().collect();
    if b.partes.borrow()[parte] == v {
        return;
    }
    b.partes.borrow_mut()[parte] = v;
    if colocar_partes(h) {
        unsafe { InvalidateRect(h, std::ptr::null(), 0) };
    } else {
        let r = b.rects.borrow()[parte];
        unsafe { InvalidateRect(h, &r, 0) };
    }
}

/// Lo menos que mide la posición, para que no baile al teclear: se mide una vez (la letra no cambia).
fn ancho_posicion(dc: HDC) -> i32 {
    thread_local!(static ANCHO: Cell<i32> = const { Cell::new(-1) });
    ANCHO.with(|a| {
        if a.get() < 0 {
            a.set(medir(dc, &ws("Lín. 0000, col. 000")).cx);
        }
        a.get()
    })
}

/// Reparte el ancho: las partes de la derecha miden lo que su texto y el mensaje se queda el resto. Devuelve si
/// alguna parte ha cambiado de sitio. Las partes no encogen mientras se teclea: así no bailan.
pub fn colocar_partes(h: HWND) -> bool {
    let b = barras();
    let mut rc = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { GetClientRect(h, &mut rc) };
    let partes = b.partes.borrow();
    let mut rects = b.rects.borrow_mut();
    let antes = *rects;
    let hueco = px(10);
    let mut x = rc.right - px(4);
    let mut anchos = [0; PARTES];
    unsafe {
        let dc = GetDC(h);
        let viejo = SelectObject(dc, fuente_ui() as _);
        for (i, ancho) in anchos.iter_mut().enumerate().skip(1) {
            *ancho = medir(dc, &partes[i]).cx + 2 * hueco;
            if i == 1 {
                *ancho = (*ancho).max(ancho_posicion(dc) + 2 * hueco);
            }
        }
        SelectObject(dc, viejo);
        ReleaseDC(h, dc);
    }
    // Las que no caben se esconden en vez de salirse por la izquierda (M3): el zoom, los saltos, la codificación y,
    // la última, la posición.
    for i in [2, 3, 4, 1] {
        if anchos.iter().sum::<i32>() > x {
            anchos[i] = 0;
        }
    }
    for i in (1..PARTES).rev() {
        let mut w = anchos[i];
        let previo = antes[i].right - antes[i].left;
        if w > 0 && previo > w && antes[i].right == x {
            w = previo;
        }
        rects[i] = RECT { left: x - w, top: 1, right: x, bottom: rc.bottom };
        x -= w;
    }
    rects[0] = RECT { left: 0, top: 1, right: x, bottom: rc.bottom };
    let igual = |a: &RECT, b: &RECT| (a.left, a.top, a.right, a.bottom) == (b.left, b.top, b.right, b.bottom);
    !rects[1..].iter().zip(&antes[1..]).all(|(a, b)| igual(a, b)) || rects[0].right != antes[0].right
}

/// Qué parte hay en ese punto de la barra de estado.
pub fn parte_en(x: i32) -> Option<usize> {
    barras().rects.borrow().iter().position(|r| x >= r.left && x < r.right)
}

unsafe extern "system" fn estado_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    seguro(|| estado_mensaje(h, m, wp, lp), || 0)
}

fn estado_mensaje(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = unsafe { std::mem::zeroed() };
            let dc = unsafe { BeginPaint(h, &mut ps) };
            pintar_estado(h, dc, &ps.rcPaint);
            unsafe { EndPaint(h, &ps) };
            0
        }
        WM_ERASEBKGND => 1,
        WM_SIZE => {
            colocar_partes(h);
            unsafe { InvalidateRect(h, std::ptr::null(), 0) };
            0
        }
        WM_LBUTTONUP => {
            let x = (lp & 0xFFFF) as i16 as i32;
            if let Some(parte) = parte_en(x) {
                let mut p = POINT { x, y: 0 };
                unsafe { ClientToScreen(h, &mut p) };
                crate::ventana::menu_estado(parte, p);
            }
            0
        }
        _ => unsafe { DefWindowProcW(h, m, wp, lp) },
    }
}

fn pintar_estado(h: HWND, dc: HDC, recorte: &RECT) {
    let b = barras();
    let p = tema::paleta(b.noche.get());
    let mut rc = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { GetClientRect(h, &mut rc) };
    unsafe {
        let mem = CreateCompatibleDC(dc);
        let bmp = CreateCompatibleBitmap(dc, rc.right.max(1), rc.bottom.max(1));
        let viejo = SelectObject(mem, bmp as _);
        rellenar(mem, recorte, p.barra);
        rellenar(mem, &RECT { bottom: 1, ..rc }, p.borde);
        SetBkMode(mem, TRANSPARENT as i32);
        SetTextColor(mem, tema::texto_barra(b.noche.get()));
        let f = SelectObject(mem, fuente_ui() as _);
        let hueco = px(10);
        for (i, (r, t)) in b.rects.borrow().iter().zip(b.partes.borrow().iter()).enumerate() {
            // Escondida porque no cabe (ver colocar_partes).
            if i > 0 && r.right <= r.left {
                continue;
            }
            if i > 0 {
                rellenar(mem, &RECT { left: r.left, right: r.left + 1, top: r.top + px(4), bottom: r.bottom - px(4) }, p.borde);
            }
            // Un texto vacío ni se dibuja: DrawText lee el puntero aunque el largo sea 0, y el de un Vec vacío no vale.
            if t.is_empty() {
                continue;
            }
            let mut a = RECT { left: r.left + hueco, right: r.right - hueco / 2, ..*r };
            DrawTextW(mem, t.as_ptr(), t.len() as i32, &mut a, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS);
        }
        SelectObject(mem, f);
        BitBlt(dc, recorte.left, recorte.top, recorte.right - recorte.left, recorte.bottom - recorte.top, mem, recorte.left, recorte.top, SRCCOPY);
        SelectObject(mem, viejo);
        DeleteObject(bmp as _);
        DeleteDC(mem);
    }
}
