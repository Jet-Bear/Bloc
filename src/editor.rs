// El cuadro de texto: el control enriquecido de Windows (msftedit.dll), el mismo en que se basa el Bloc de notas de
// Windows 11. Aquí está lo que le falta o sobra: pegar siempre como texto, copiar el texto tal cual, zoom con
// Ctrl+rueda propio y ningún atajo de Ctrl que cambie el formato por su cuenta.
use std::cell::{Cell, RefCell};

use windows_sys::Win32::Foundation::{GlobalFree, HWND, LPARAM, LRESULT, POINT, POINTL, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, ClientToScreen, CreateBitmap, CreateCompatibleBitmap, CreateCompatibleDC, CreateRectRgn, CreateSolidBrush, DeleteDC, DeleteObject, FillRect, GetDC, GetUpdateRect, GetUpdateRgn, InvalidateRect, InvalidateRgn, ReleaseDC, ValidateRect, HRGN, SelectObject, SetBkColor, SetTextColor, UpdateWindow, FF_MODERN, FF_SWISS, SRCCOPY,
};
use windows_sys::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
use windows_sys::Win32::System::Ole::{RevokeDragDrop, CF_UNICODETEXT};
use windows_sys::Win32::UI::Controls::{EM_CHARFROMPOS, EM_GETFIRSTVISIBLELINE, EM_GETLINECOUNT, EM_GETRECT, EM_LINEINDEX, EM_LINESCROLL, EM_POSFROMCHAR, EM_REPLACESEL, EM_SCROLLCARET, EM_SETMARGINS, EM_SETRECT, EM_SETSEL, EM_UNDO};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetCapture, GetFocus, GetKeyboardState, SetFocus, SetKeyboardState, VK_BACK, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F6, VK_HOME, VK_INSERT, VK_LEFT, VK_MENU, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_TAB, VK_UP};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, DragAcceptFiles, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::re::*;
use crate::ventana;
use crate::w::{enviar, pulsada, seguro, ws};

thread_local! {
    // Windows avisa dos veces al sustituir una selección (al borrar y al escribir). Mientras dura un mensaje que
    // edita, los avisos se juntan en uno: así cada acción es un solo cambio.
    static EN_CURSO: Cell<u32> = const { Cell::new(0) };
    static JUNTA: Cell<bool> = const { Cell::new(false) };
    static PENDIENTE: Cell<bool> = const { Cell::new(false) };
    static INTRO_TECLA: Cell<bool> = const { Cell::new(false) };
    static ESPACIO: Cell<bool> = const { Cell::new(false) };
    static RUEDA: Cell<i32> = const { Cell::new(0) };
    static RUEDA_ZOOM: Cell<i32> = const { Cell::new(0) };
    // La selección la está poniendo Bloc (ver `seleccionar`), no otro programa.
    static PROPIA: Cell<bool> = const { Cell::new(false) };
    // Cuántos mensajes está atendiendo RichEdit ahora mismo, y si alguno se ha pintado sin lo que dibuja Bloc encima
    // (ver `por_richedit`).
    static EN_RICHEDIT: Cell<u32> = const { Cell::new(0) };
    static SIN_ENCIMA: Cell<bool> = const { Cell::new(false) };
    // Dentro de `quieto`: la vista está congelada y lo que se dibujara encima saldría con el formato a medias.
    static QUIETO: Cell<u32> = const { Cell::new(0) };
    // Lo que ha repintado RichEdit desde que Bloc dibujó encima por última vez (ver `repintado`).
    static REPINTADO: Cell<Option<RECT>> = const { Cell::new(None) };
    // Los márgenes puestos (ver `margenes`).
    static MARGEN: Cell<(i32, i32, i32)> = const { Cell::new((-1, -1, -1)) };
    // El desplazamiento horizontal del último pintado (ver WM_PAINT).
    static VISTA_X: Cell<i32> = const { Cell::new(0) };
    static DESLIZ: Cell<Option<Desliz>> = const { Cell::new(None) };
    // Los párrafos con las marcas enseñadas por `seleccionar`: se vuelven a pintar cuando la selección sale de ellos.
    static DESTAPADOS: RefCell<Vec<(usize, usize)>> = const { RefCell::new(Vec::new()) };
}

/// Los párrafos con las marcas enseñadas por `seleccionar`, para mirarlos o cambiarlos.
pub fn destapados<R>(f: impl FnOnce(&mut Vec<(usize, usize)>) -> R) -> R {
    DESTAPADOS.with(|d| f(&mut d.borrow_mut()))
}

/// El cursor que se desliza al escribir y al borrar, como en Word (ver `deslizar`): de dónde sale y adónde va en su
/// línea, cuándo empezó y dónde lo ha puesto Bloc la última vez.
#[derive(Clone, Copy)]
struct Desliz {
    desde: i32,
    hasta: i32,
    y: i32,
    inicio: std::time::Instant,
    puesto: i32,
}

const RELOJ_CURSOR: usize = 7;
const DESLIZ_MS: f32 = 70.0;

fn cursor() -> POINT {
    let mut p = POINT { x: 0, y: 0 };
    unsafe { GetCaretPos(&mut p) };
    p
}

/// Tras escribir o borrar en la misma línea, el cursor va de donde estaba a su sitio en 70 ms en vez de saltar. No le
/// pregunta nada a RichEdit (la posición es la del cursor de Windows) y el temporizador solo existe mientras se mueve.
/// Si RichEdit lo pone en otro sitio a mitad de camino, manda él.
fn deslizar(h: HWND, antes: POINT) {
    let ahora = cursor();
    let (s, e) = seleccion(h);
    let d = ahora.x - antes.x;
    if s != e || ahora.y != antes.y || d == 0 || d.abs() > 400 {
        return;
    }
    unsafe { SetCaretPos(antes.x, antes.y) };
    DESLIZ.with(|x| x.set(Some(Desliz { desde: antes.x, hasta: ahora.x, y: ahora.y, inicio: std::time::Instant::now(), puesto: antes.x })));
    unsafe { SetTimer(h, RELOJ_CURSOR, 10, None) };
}

fn paso_cursor(h: HWND) {
    let parar = || {
        DESLIZ.with(|x| x.set(None));
        unsafe { KillTimer(h, RELOJ_CURSOR) };
    };
    let Some(mut d) = DESLIZ.with(Cell::get) else { return parar() };
    let p = cursor();
    if unsafe { GetFocus() } != h || p.x != d.puesto || p.y != d.y {
        return parar();
    }
    let t = (d.inicio.elapsed().as_secs_f32() * 1000.0 / DESLIZ_MS).min(1.0);
    let x = d.desde + ((d.hasta - d.desde) as f32 * (1.0 - (1.0 - t).powi(3))).round() as i32;
    unsafe { SetCaretPos(x, d.y) };
    d.puesto = x;
    if t >= 1.0 {
        parar();
    } else {
        DESLIZ.with(|c| c.set(Some(d)));
    }
}

/// Lo que hace RichEdit con un mensaje. A veces se pinta él mismo a mitad de su trabajo (al desplazarse mientras
/// escribe): en ese WM_PAINT no se le pregunta nada, porque preguntarle entonces por posiciones lo deja colgado para
/// siempre (prueba al azar, semilla 8). Las rayas y las tablas se dibujan en cuanto acaba, encima de lo que pintó.
fn por_richedit(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    EN_RICHEDIT.with(|n| n.set(n.get() + 1));
    let r = {
        let _fin = AlSalir(|| EN_RICHEDIT.with(|n| n.set(n.get() - 1)));
        unsafe { DefSubclassProc(h, m, wp, lp) }
    };
    if EN_RICHEDIT.with(Cell::get) == 0 && QUIETO.with(Cell::get) == 0 && SIN_ENCIMA.with(|s| s.replace(false)) {
        ventana::dibujar_rayas(h);
        ventana::dibujar_tablas(h);
        ventana::dibujar_imagenes(h);
    }
    r
}

/// ¿Está el control a mitad de un cambio? Entonces lo que se lea de él aún no es definitivo.
pub fn en_curso() -> bool {
    EN_CURSO.with(Cell::get) > 0
}

/// Aviso de cambio: se atiende ya o al acabar el mensaje que lo provoca.
pub fn al_cambiar() {
    if en_curso() {
        PENDIENTE.with(|p| p.set(true));
    } else {
        ventana::al_cambiar();
    }
}

/// Hace lo suyo al salir del bloque, también tras un pánico (que `w::seguro` deja seguir): así nada se queda a medias
/// para siempre, ni la vista congelada ni el control sin avisar de los cambios.
pub struct AlSalir<F: FnMut()>(pub F);

impl<F: FnMut()> Drop for AlSalir<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}

pub fn editando<R>(f: impl FnOnce() -> R) -> R {
    EN_CURSO.with(|e| e.set(e.get() + 1));
    let r = {
        let _fin = AlSalir(|| EN_CURSO.with(|e| e.set(e.get() - 1)));
        f()
    };
    if !en_curso() && PENDIENTE.with(|p| p.replace(false)) {
        ventana::al_cambiar();
    }
    r
}

pub fn crear(padre: HWND, id: usize) -> HWND {
    let clase = ws(CLASE);
    let h = unsafe {
        CreateWindowExW(
            0,
            clase.as_ptr(),
            std::ptr::null(),
            WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_HSCROLL | (ES_MULTILINE | ES_AUTOVSCROLL | ES_AUTOHSCROLL | ES_NOHIDESEL) as u32 | ES_NOOLEDRAGDROP,
            0,
            0,
            0,
            0,
            padre,
            id as _,
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        )
    };
    // Un control nuevo (la vista previa crea uno por archivo en el mismo hilo) lleva sus márgenes de entrada.
    MARGEN.with(|m| m.set((-1, -1, -1)));
    enviar(h, EM_EXLIMITTEXT, 0, 0x7FFF_FFFE);
    enviar(h, EM_SETEVENTMASK, 0, (ENM_CHANGE | ENM_SELCHANGE) as isize);
    enviar(h, 0x45B, 0, 0); // EM_AUTOURLDETECT: las direcciones no son enlaces que se abran solos
    let idioma = enviar(h, EM_GETLANGOPTIONS, 0, 0);
    enviar(h, EM_SETLANGOPTIONS, 0, idioma & !IMF_AUTOKEYBOARD); // que mover el cursor no cambie el teclado
    unsafe {
        RevokeDragDrop(h); // solo se sueltan archivos (para abrirlos), nunca texto con formato u objetos
        DragAcceptFiles(h, 1);
        SetWindowSubclass(h, Some(subclase), 1, 0);
    }
    h
}

pub fn largo(h: HWND) -> usize {
    let g = GETTEXTLENGTHEX { flags: GTL_NUMCHARS | GTL_PRECISE, codepage: 1200 };
    enviar(h, EM_GETTEXTLENGTHEX, &g as *const _ as usize, 0).max(0) as usize
}

/// Todo el texto, con un \r por salto (así cuadran las posiciones), en un búfer que se reutiliza.
pub fn leer(h: HWND, b: &mut Vec<u16>) {
    let n = largo(h);
    b.clear();
    b.resize(n + 1, 0);
    let g = GETTEXTEX { cb: ((n + 1) * 2) as u32, flags: 0, codepage: 1200, lpDefaultChar: std::ptr::null(), lpUsedDefChar: std::ptr::null_mut() };
    let r = enviar(h, EM_GETTEXTEX, &g as *const _ as usize, b.as_mut_ptr() as isize).max(0) as usize;
    b.truncate(r.min(n));
    como_archivo(b);
}

/// Si el control lleva los saltos blandos como md::JUNTO (lo decide ventana al cargar y al cambiar las marcas).
pub fn juntar(si: bool) {
    JUNTA.with(|j| j.set(si));
}

pub fn junta() -> bool {
    JUNTA.with(Cell::get)
}

/// El texto del control como el del archivo: cada JUNTO es el salto que sustituye.
pub fn como_archivo(t: &mut [u16]) {
    if junta() {
        for c in t.iter_mut().filter(|c| **c == crate::md::JUNTO) {
            *c = 13;
        }
    }
}

pub fn texto(h: HWND) -> Vec<u16> {
    let mut b = Vec::new();
    leer(h, &mut b);
    b
}

pub fn rango(h: HWND, a: usize, b: usize) -> Vec<u16> {
    if b <= a {
        return Vec::new();
    }
    let mut t = vec![0u16; b - a + 1];
    let mut r = TEXTRANGEW { chrg: CHARRANGE { cpMin: a as i32, cpMax: b as i32 }, lpstrText: t.as_mut_ptr() };
    let n = enviar(h, EM_GETTEXTRANGE, 0, &mut r as *mut _ as isize).max(0) as usize;
    t.truncate(n.min(b - a));
    t
}

pub fn seleccion(h: HWND) -> (usize, usize) {
    let mut r = CHARRANGE::default();
    enviar(h, EM_EXGETSEL, 0, &mut r as *mut _ as isize);
    (r.cpMin.max(0) as usize, r.cpMax.max(0) as usize)
}

/// La selección, puesta por Bloc (ver PROPIA), sin mirar si RichEdit la ha sacado del texto oculto.
fn poner_seleccion(h: HWND, a: usize, b: usize) {
    let r = CHARRANGE { cpMin: a as i32, cpMax: b as i32 };
    let antes = PROPIA.with(|p| p.replace(true));
    let _fin = AlSalir(|| PROPIA.with(|p| p.set(antes)));
    enviar(h, EM_EXSETSEL, 0, &r as *const _ as isize);
}

pub fn seleccionar(h: HWND, a: usize, b: usize) {
    rastro("Bloc pone la selección");
    poner_seleccion(h, a, b);
    // RichEdit saca los extremos de dentro del texto oculto (las marcas de un .md, ver md::linea). Si alguno ha caído
    // dentro, las marcas de sus líneas se enseñan (siguen de un punto y del color del fondo hasta que la selección sale
    // de ellas: ver ventana::tapar) y se pone otra vez.
    let (p, q) = (a.min(b), a.max(b));
    let fuera = seleccion(h) != (p, q);
    if let Some(n) = fuera.then(|| largo(h)).filter(|&n| q <= n) {
        let salto = |c: u16| c == 13 || c == crate::md::JUNTO;
        let mut desde = p;
        while desde > 0 {
            let x = desde.saturating_sub(256);
            match rango(h, x, desde).iter().rposition(|&c| salto(c)) {
                Some(i) => {
                    desde = x + i + 1;
                    break;
                }
                None => desde = x,
            }
        }
        let mut hasta = q;
        while hasta < n {
            let y = (hasta + 256).min(n);
            match rango(h, hasta, y).iter().position(|&c| salto(c)) {
                Some(i) => {
                    hasta += i;
                    break;
                }
                None => hasta = y,
            }
        }
        poner_seleccion(h, desde, hasta);
        sin_ocultar(h);
        destapados(|d| d.push((desde, hasta)));
        poner_seleccion(h, a, b);
    }
}

/// Enseña el texto oculto de la selección.
fn sin_ocultar(h: HWND) {
    let mut f: CHARFORMAT2W = unsafe { std::mem::zeroed() };
    f.cbSize = std::mem::size_of::<CHARFORMAT2W>() as u32;
    f.dwMask = CFM_HIDDEN;
    enviar(h, EM_SETCHARFORMAT, SCF_SELECTION, &f as *const _ as isize);
}

/// Sustituye la selección como si se hubiera tecleado. Si la selección acaba en un salto de línea y lo que entra no
/// lleva ninguno, RichEdit se queda con el salto (como Word), y deshacer o un botón dejarían una línea de más: entonces
/// entra también lo que sigue al salto; al final del texto basta con coger el salto final, que RichEdit no quita.
pub fn sustituir(h: HWND, t: &[u16]) {
    // EM_REPLACESEL no respeta el solo lectura: en la vista previa no se cambia nada.
    if ventana::solo_ver() {
        return;
    }
    const CR: u16 = 13;
    let (a, b) = seleccion(h);
    let mut z = Vec::with_capacity(t.len() + 3);
    z.extend_from_slice(t);
    let salto = b > a && !t.is_empty() && !t.contains(&CR) && rango(h, b - 1, b) == [CR];
    if salto {
        let tras = rango(h, b, b + 2);
        let n = if tras.first().is_some_and(|c| (0xD800..0xDC00).contains(c)) { tras.len() } else { 1 };
        seleccionar(h, a, b + n);
        z.extend_from_slice(&tras[..n.min(tras.len())]);
    }
    z.push(0);
    enviar(h, EM_REPLACESEL, 1, z.as_ptr() as isize);
    if salto {
        seleccionar(h, a + t.len(), a + t.len());
    }
}

pub fn poner_texto(h: HWND, t: &[u16]) {
    // RichEdit lee como RTF un texto que empieza por «{\rtf» o «{\urtf», también en UTF-16 si sus bytes lo forman
    // (0x5C7B es «{\» byte a byte): ese entra como texto llano, sin que nadie pueda colar formato desde un archivo.
    if matches!(t.first(), Some(0x5C7B | 0x757B)) || t.starts_with(&[0x7B, 0x5C]) {
        let b: Vec<u8> = t.iter().flat_map(|c| c.to_le_bytes()).collect();
        meter(h, &b, SF_TEXT | SF_UNICODE);
        return;
    }
    let mut z = t.to_vec();
    z.push(0);
    let s = SETTEXTEX { flags: ST_DEFAULT, codepage: 1200 };
    enviar(h, EM_SETTEXTEX, &s as *const _ as usize, z.as_ptr() as isize);
}

pub fn ver_cursor(h: HWND) {
    enviar(h, EM_SCROLLCARET, 0, 0);
}

pub fn desplazamiento(h: HWND) -> POINT {
    let mut p = POINT { x: 0, y: 0 };
    enviar(h, EM_GETSCROLLPOS, 0, &mut p as *mut _ as isize);
    p
}

pub fn desplazar(h: HWND, p: POINT) {
    enviar(h, EM_SETSCROLLPOS, 0, &p as *const _ as isize);
}

pub fn ajuste(h: HWND, si: bool) {
    enviar(h, EM_SETTARGETDEVICE, 0, if si { 0 } else { 1 });
}

pub fn zoom(h: HWND, z: i32) {
    if z == 100 { enviar(h, EM_SETZOOM, 0, 0) } else { enviar(h, EM_SETZOOM, z as usize, 100) };
    rehacer_alto(h);
}

/// El zoom que tiene el control, en tanto por ciento.
pub fn zoom_actual(h: HWND) -> i32 {
    let (mut n, mut d) = (0i32, 0i32);
    enviar(h, EM_GETZOOM, &mut n as *mut i32 as usize, &mut d as *mut i32 as isize);
    if n > 0 && d > 0 { n * 100 / d } else { 100 }
}

/// Los márgenes del texto: `px` a cada lado y `arriba` bajo la barra, en píxeles a 100 % de zoom (RichEdit los escala
/// con él, ver `formato`). Con el rectángulo del texto, del que RichEdit conserva lo que deja a cada lado al
/// redimensionar, y no con EM_SETMARGINS, que no deja hueco arriba.
pub fn margenes(h: HWND, px: i32, arriba: i32) {
    let mut c = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { GetClientRect(h, &mut c) };
    // Igual que estaban (al cambiar solo el alto de la ventana), RichEdit volvería a colocar el texto para nada.
    let antes = MARGEN.with(|m| m.replace((px, arriba, c.right)));
    if antes == (px, arriba, c.right) {
        return;
    }
    let r = RECT { left: c.left + px, top: c.top + arriba, right: (c.right - px).max(c.left + px + 1), bottom: c.bottom.max(c.top + arriba + 1) };
    // Al cambiar de tamaño RichEdit ya lo ha dejado así y ha colocado el texto una vez: otra costaría lo mismo (100 ms
    // en cada paso con un .md de 1 MB en la vista previa).
    let mut ya = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    enviar(h, EM_GETRECT, 0, &mut ya as *mut _ as isize);
    if antes.0 == px && antes.1 == arriba && (ya.left, ya.top, ya.right, ya.bottom) == (r.left, r.top, r.right, r.bottom) {
        return;
    }
    // Los de RichEdit, a cero una sola vez: cada EM_SETMARGINS vuelve a colocar todo el texto, y EM_SETRECT otra vez.
    if antes.0 < 0 {
        enviar(h, EM_SETMARGINS, 3, 0); // EC_LEFTMARGIN | EC_RIGHTMARGIN
    }
    enviar(h, EM_SETRECT, 0, &r as *const _ as isize);
    rehacer_alto(h);
}

/// Tras cambiar el zoom o los márgenes, RichEdit calcula a ojo el alto de la barra de desplazamiento y lo corrige a
/// trozos mientras coloca las líneas; contarlas le obliga a hacerlo ya. En un documento largo costaría segundos (1,5 s
/// un .md de 900 KB, en cada paso del zoom) y se deja que lo haga él: en unos segundos, sin parar la ventana.
fn rehacer_alto(h: HWND) {
    if largo(h) <= 20_000 {
        enviar(h, EM_GETLINECOUNT, 0, 0);
    }
}

pub fn fondo(h: HWND, color: u32) {
    enviar(h, EM_SETBKGNDCOLOR, 0, color as isize);
}

/// Una letra, un tamaño y un color, sin nada más.
fn formato_simple(letra: &str, tamano: i32, color: u32) -> CHARFORMAT2W {
    let mut f: CHARFORMAT2W = unsafe { std::mem::zeroed() };
    f.cbSize = std::mem::size_of::<CHARFORMAT2W>() as u32;
    // Sin CFM_CHARSET: con la letra, obliga a revisar carácter a carácter qué letra lo dibuja (38 s con 5 MB).
    f.dwMask = CFM_FACE | CFM_SIZE | CFM_COLOR | CFM_BOLD | CFM_ITALIC | CFM_UNDERLINE | CFM_STRIKEOUT | CFM_BACKCOLOR | CFM_HIDDEN;
    f.dwEffects = CFE_AUTOBACKCOLOR;
    f.yHeight = tamano * 20;
    f.crTextColor = color;
    for (i, c) in letra.encode_utf16().take(31).enumerate() {
        f.szFaceName[i] = c;
    }
    f
}

/// El formato de todo el texto plano (y de lo que se escriba).
pub fn formato_plano(h: HWND, letra: &str, tamano: i32, color: u32) {
    let f = formato_simple(letra, tamano, color);
    for alcance in [SCF_DEFAULT, SCF_ALL] {
        enviar(h, EM_SETCHARFORMAT, alcance, &f as *const _ as isize);
    }
}

/// El formato de letra de un tramo del Markdown, en lo seleccionado: deja lo mismo que el RTF de `md::pintar` (comprobado
/// campo a campo; el subrayado sencillo es el tipo de todas sus letras, aunque no lo lleven). Las letras que la fuente
/// no tiene conservan la de reserva que les puso RichEdit al entrar.
pub fn letra(h: HWND, e: &crate::md::Estilo, c: &crate::md::Colores) {
    let familia = if e.mono { FF_MODERN } else { FF_SWISS };
    let mut f = formato_simple(crate::md::nombre_letra(e), 0, crate::md::color(c, e.color));
    f.dwMask |= CFM_SPACING | CFM_UNDERLINETYPE;
    f.yHeight = e.tam as i32 * 10; // medios puntos
    f.bPitchAndFamily = familia;
    for (si, efecto) in [(e.negrita && e.mono, CFE_BOLD), (e.cursiva, CFE_ITALIC), (e.subrayado, CFE_UNDERLINE), (e.tachado, CFE_STRIKEOUT), (e.oculto, CFE_HIDDEN)] {
        if si {
            f.dwEffects |= efecto;
        }
    }
    if e.fondo != 0 {
        f.dwEffects &= !CFE_AUTOBACKCOLOR;
        f.crBackColor = crate::md::color(c, e.fondo);
    }
    f.sSpacing = e.espacio as i16;
    f.bUnderlineType = CFU_UNDERLINE;
    enviar(h, EM_SETCHARFORMAT, SCF_SELECTION, &f as *const _ as isize);
}

/// El párrafo de las líneas que toca la selección: la viñeta o el número que dibuja el control, las sangrías, si va
/// centrado y si se pliega (interlineado exacto: de 1 punto, o el alto de la fila de una tabla dibujada o de una valla).
/// Con `vista` (las marcas ocultas), el interlineado normal es el de md::cabecera, 1,15, y el del código, 1,2: Consolas
/// trae menos que Segoe UI y así los dos quedan como en la vista previa.
pub fn parrafo(h: HWND, p: &crate::md::Parrafo, vista: bool) {
    let mut f: PARAFORMAT2 = unsafe { std::mem::zeroed() };
    f.cbSize = std::mem::size_of::<PARAFORMAT2>() as u32;
    f.dwMask = PFM_NUMBERING
        | PFM_NUMBERINGSTART
        | PFM_NUMBERINGSTYLE
        | PFM_STARTINDENT
        | PFM_RIGHTINDENT
        | PFM_OFFSET
        | PFM_ALIGNMENT
        | PFM_LINESPACING
        | PFM_SPACEBEFORE
        | PFM_SPACEAFTER;
    f.dySpaceBefore = p.antes;
    f.dySpaceAfter = p.despues;
    f.wNumbering = p.numero;
    f.wNumberingStart = p.inicio;
    f.wNumberingStyle = p.estilo;
    f.dxStartIndent = p.sangria;
    f.dxRightIndent = p.derecha;
    f.dxOffset = p.colgado;
    f.wAlignment = if p.raya { PFA_CENTER } else { PFA_LEFT };
    (f.bLineSpacingRule, f.dyLineSpacing) = if p.alto > 0 {
        (4, p.alto)
    } else if p.plegado {
        (4, 20)
    } else if vista && p.codigo {
        (5, 24)
    } else if vista && !p.sencillo {
        (5, 23)
    } else {
        (0, 0)
    };
    enviar(h, EM_SETPARAFORMAT, 0, &f as *const _ as isize);
}

/// Solo el alto de la línea que empieza en `p` (interlineado exacto, en twips): el de una fila de una tabla plegada al
/// cambiar el zoom o el ancho (0, el de un punto, como en `parrafo`). Si ya lo tiene, no se pone: cambiarlo obliga a
/// RichEdit a volver a colocar lo de después. Las filas no tienen texto oculto: la selección se pone sin comprobarla
/// (ver `seleccionar`), que con miles de filas es la mitad de los mensajes.
pub fn interlineado(h: HWND, p: usize, twips: i32) {
    poner_seleccion(h, p, p);
    let mut f: PARAFORMAT2 = unsafe { std::mem::zeroed() };
    f.cbSize = std::mem::size_of::<PARAFORMAT2>() as u32;
    let alto = if twips > 0 { twips } else { 20 };
    enviar(h, EM_GETPARAFORMAT, 0, &mut f as *mut _ as isize);
    if (f.bLineSpacingRule, f.dyLineSpacing) == (4, alto) {
        return;
    }
    f.dwMask = PFM_LINESPACING;
    (f.bLineSpacingRule, f.dyLineSpacing) = (4, alto);
    enviar(h, EM_SETPARAFORMAT, 0, &f as *const _ as isize);
}

/// La letra en `p` es de un punto (la de una fila plegada, ver md::linea). Como en `interlineado`, sin comprobar la
/// selección.
pub fn de_un_punto(h: HWND, p: usize) -> bool {
    poner_seleccion(h, p, p + 1);
    let mut f: CHARFORMAT2W = unsafe { std::mem::zeroed() };
    f.cbSize = std::mem::size_of::<CHARFORMAT2W>() as u32;
    f.dwMask = CFM_SIZE;
    enviar(h, EM_GETCHARFORMAT, SCF_SELECTION, &mut f as *mut _ as isize);
    f.yHeight == 20
}

/// La línea de la raya `---` seleccionada: subrayado grueso gris bajo sus guiones, que van del color del fondo.
pub fn raya(h: HWND) {
    let mut f: CHARFORMAT2W = unsafe { std::mem::zeroed() };
    f.cbSize = std::mem::size_of::<CHARFORMAT2W>() as u32;
    f.dwMask = CFM_UNDERLINE | CFM_UNDERLINETYPE;
    f.dwEffects = CFE_UNDERLINE;
    f.bUnderlineType = CFU_UNDERLINETHICK;
    f.bUnderlineColor = 15; // gris oscuro de la paleta fija del control: se ve igual en claro y en oscuro
    enviar(h, EM_SETCHARFORMAT, SCF_SELECTION, &f as *const _ as isize);
}

/// Deshacer de RichEdit: en texto plano, el suyo; en los Markdown, el de Bloc (el de RichEdit vería el formato).
pub fn deshacer_nativo(h: HWND, si: bool) {
    enviar(h, EM_SETUNDOLIMIT, if si { 500 } else { 0 }, 0);
}

pub fn corrector(h: HWND, si: bool) {
    let idioma = enviar(h, EM_GETLANGOPTIONS, 0, 0);
    enviar(h, EM_SETLANGOPTIONS, 0, if si { idioma | IMF_SPELLCHECKING } else { idioma & !IMF_SPELLCHECKING });
    if si {
        let estilo = SES_USECTF | SES_CTFALLOWPROOFING;
        enviar(h, EM_SETEDITSTYLE, estilo, estilo as isize);
    }
}

type Doc = *mut std::ffi::c_void;

/// ITextDocument, la interfaz de objetos del propio control (TOM), mientras dura `f` (nulo si no la da).
fn con_documento<R>(h: HWND, f: impl FnOnce(Doc) -> R) -> R {
    const EM_GETOLEINTERFACE: u32 = 0x43C;
    const IID_ITEXTDOCUMENT: windows_sys::core::GUID = windows_sys::core::GUID::from_u128(0x8CC497C0_A1DF_11CE_8098_00AA0047BE5D);
    type Qi = unsafe extern "system" fn(Doc, *const windows_sys::core::GUID, *mut Doc) -> i32;
    unsafe {
        let mut ole: Doc = std::ptr::null_mut();
        enviar(h, EM_GETOLEINTERFACE, 0, &mut ole as *mut _ as isize);
        let mut doc: Doc = std::ptr::null_mut();
        if !ole.is_null() {
            let qi: Qi = std::mem::transmute(metodo(ole, 0));
            qi(ole, &IID_ITEXTDOCUMENT, &mut doc);
            soltar(ole);
        }
        let r = f(doc);
        if !doc.is_null() {
            soltar(doc);
        }
        r
    }
}

/// El método n de la tabla de una interfaz (en ITextDocument: IUnknown 3 + IDispatch 4 + los de antes).
unsafe fn metodo(o: Doc, n: usize) -> usize {
    unsafe { *(*(o as *const *const usize)).add(n) }
}

unsafe fn soltar(o: Doc) {
    type Release = unsafe extern "system" fn(Doc) -> u32;
    unsafe {
        let r: Release = std::mem::transmute(metodo(o, 2));
        r(o);
    }
}

/// Cambia el formato sin que entre en el deshacer de RichEdit: si no, el primer Ctrl+Z desharía un cambio de color
/// en vez de lo último escrito (ITextDocument::Undo con tomSuspend y tomResume).
pub fn sin_deshacer<R>(h: HWND, f: impl FnOnce() -> R) -> R {
    type Undo = unsafe extern "system" fn(Doc, i32, *mut i32) -> i32;
    let undo = |doc: Doc, n: i32| unsafe {
        if !doc.is_null() {
            let u: Undo = std::mem::transmute(metodo(doc, 22));
            u(doc, n, std::ptr::null_mut());
        }
    };
    con_documento(h, |doc| {
        undo(doc, -9999995);
        let _fin = AlSalir(|| undo(doc, -9999994));
        f()
    })
}

/// ITextSelection mientras dura `f` (ITextDocument::GetSelection, 8), o nada si el control no la da.
fn con_seleccion<R>(h: HWND, f: impl FnOnce(Doc) -> R) -> Option<R> {
    type Dar = unsafe extern "system" fn(Doc, *mut Doc) -> i32;
    con_documento(h, |doc| unsafe {
        let mut sel: Doc = std::ptr::null_mut();
        if doc.is_null() || (std::mem::transmute::<usize, Dar>(metodo(doc, 8)))(doc, &mut sel) != 0 || sel.is_null() {
            return None;
        }
        let r = f(sel);
        soltar(sel);
        Some(r)
    })
}

/// La selección con su sentido, (ancla, extremo que se mueve): EM_EXGETSEL no dice hacia dónde se alargó.
/// ITextSelection::GetFlags (58), con tomSelStartActive.
pub fn seleccion_con_sentido(h: HWND) -> (usize, usize) {
    type Dar = unsafe extern "system" fn(Doc, *mut i32) -> i32;
    let (s, e) = seleccion(h);
    con_seleccion(h, |sel| unsafe {
        let mut f = 0;
        (std::mem::transmute::<usize, Dar>(metodo(sel, 58)))(sel, &mut f);
        if f & 1 != 0 { (e, s) } else { (s, e) }
    })
    .unwrap_or((s, e))
}

/// Pone la selección de `ancla` a `activo`, con el que se mueve donde se dice (ITextRange::SetRange, 28).
pub fn seleccionar_con_sentido(h: HWND, ancla: usize, activo: usize) {
    type Poner = unsafe extern "system" fn(Doc, i32, i32) -> i32;
    if con_seleccion(h, |sel| unsafe { (std::mem::transmute::<usize, Poner>(metodo(sel, 28)))(sel, ancla as i32, activo as i32) }).is_none() {
        seleccionar(h, ancla, activo);
    }
}

/// Lo de abajo de la línea en la que está `pos`, en la ventana (ITextDocument::Range, 24, e ITextRange::GetPoint, 54,
/// con TA_BOTTOM, tomClientCoord y tomAllowOffClient): para la última línea, que no tiene otra debajo por la que medir.
pub fn bajo_de_linea(h: HWND, pos: usize) -> Option<i32> {
    type Rango = unsafe extern "system" fn(Doc, i32, i32, *mut Doc) -> i32;
    type Punto = unsafe extern "system" fn(Doc, i32, *mut i32, *mut i32) -> i32;
    con_documento(h, |doc| unsafe {
        let mut r: Doc = std::ptr::null_mut();
        if doc.is_null() || (std::mem::transmute::<usize, Rango>(metodo(doc, 24)))(doc, pos as i32, pos as i32, &mut r) != 0 || r.is_null() {
            return None;
        }
        let (mut x, mut y) = (0, 0);
        let bien = (std::mem::transmute::<usize, Punto>(metodo(r, 54)))(r, 8 | 0x100 | 0x200, &mut x, &mut y) == 0;
        soltar(r);
        bien.then_some(y)
    })
}

/// Dónde empieza la línea a la vista en la que está `pos` (con el ajuste de línea, un párrafo tiene varias).
pub fn inicio_a_la_vista(h: HWND, pos: usize) -> usize {
    enviar(h, EM_LINEINDEX, enviar(h, EM_EXLINEFROMCHAR, 0, pos as isize) as usize, 0).max(0) as usize
}

/// ITextDocument::Freeze (18) y Unfreeze (19): con la vista congelada no se pinta nada hasta descongelarla.
fn congelar(doc: Doc, si: bool) {
    type Freeze = unsafe extern "system" fn(Doc, *mut i32) -> i32;
    if !doc.is_null() {
        unsafe {
            let m: Freeze = std::mem::transmute(metodo(doc, if si { 18 } else { 19 }));
            m(doc, std::ptr::null_mut());
        }
    }
}

/// Añade texto llano al final, sin mover la selección ni lo que se ve. Como texto llano también si empieza por «{\rtf».
pub fn anadir(h: HWND, t: &[u16]) {
    quieto(h, || {
        let n = largo(h);
        seleccionar(h, n, n);
        let b: Vec<u8> = t.iter().flat_map(|c| c.to_le_bytes()).collect();
        meter(h, &b, SF_TEXT | SF_UNICODE | SFF_SELECTION);
    });
}

/// Mete RTF en lugar de la selección.
pub fn meter_rtf(h: HWND, rtf: &[u8]) {
    meter(h, rtf, SF_RTF | SFF_SELECTION);
}

fn meter(h: HWND, datos: &[u8], como: usize) {
    struct Fuente {
        datos: *const u8,
        largo: usize,
        pos: usize,
    }
    unsafe extern "system" fn dar(cookie: usize, buf: *mut u8, cb: i32, hecho: *mut i32) -> u32 {
        unsafe {
            let f = &mut *(cookie as *mut Fuente);
            let n = (f.largo - f.pos).min(cb.max(0) as usize);
            std::ptr::copy_nonoverlapping(f.datos.add(f.pos), buf, n);
            f.pos += n;
            *hecho = n as i32;
        }
        0
    }
    let mut f = Fuente { datos: datos.as_ptr(), largo: datos.len(), pos: 0 };
    let mut es = EDITSTREAM { dwCookie: &mut f as *mut Fuente as usize, dwError: 0, pfnCallback: Some(dar) };
    enviar(h, EM_STREAMIN, como, &mut es as *mut _ as isize);
}

/// Hace algo que cambia el control sin que se vea a medias ni avise, y deja la selección y el desplazamiento como
/// estaban. Congela la vista con ITextDocument::Freeze: con WM_SETREDRAW la barra de desplazamiento se quedaba como
/// antes del cambio y, con el ajuste de línea, partía las líneas a otro ancho.
pub fn quieto<R>(h: HWND, f: impl FnOnce() -> R) -> R {
    QUIETO.with(|q| q.set(q.get() + 1));
    let _quieto = AlSalir(|| QUIETO.with(|q| q.set(q.get() - 1)));
    let mascara = enviar(h, EM_GETEVENTMASK, 0, 0);
    enviar(h, EM_SETEVENTMASK, 0, 0);
    let _avisos = AlSalir(|| {
        enviar(h, EM_SETEVENTMASK, 0, mascara);
    });
    let (a, b) = seleccion(h);
    let p = desplazamiento(h);
    // El cursor, escondido mientras tanto: la selección va de un tramo a otro para darles formato y, con la vista
    // congelada, el cursor se quedaba dibujado en alguno de esos sitios (al escribir deprisa, varias barras a la vez).
    unsafe { HideCaret(h) };
    let _cursor = AlSalir(|| unsafe {
        ShowCaret(h);
    });
    let r = con_documento(h, |doc| {
        congelar(doc, true);
        let _deshielo = AlSalir(|| congelar(doc, false));
        let r = f();
        seleccionar(h, a, b);
        desplazar(h, p);
        r
    });
    // Ya descongelado, otra vez: así RichEdit rehace el alto del documento para la barra de desplazamiento.
    seleccionar(h, a, b);
    desplazar(h, p);
    r
}

/// Lo que el control tiene pendiente de repintar ahora, para `acotar`.
pub fn pendiente(h: HWND) -> HRGN {
    unsafe {
        let r = CreateRectRgn(0, 0, 0, 0);
        GetUpdateRgn(h, r, 0);
        r
    }
}

/// Tras `quieto`, que al descongelar manda repintar el control entero: deja por repintar solo `pendiente` (lo que ya
/// había) y las líneas de [ini, fin] y, si la de después ya no empieza a la altura `pie`, hasta abajo. Si RichEdit aún no
/// ha colocado esas líneas, o se ha desplazado desde `antes` (lo que destapa al desplazarse queda por pintar), se queda
/// entero. Devuelve si lo de después de `fin` sigue donde estaba.
pub fn acotar(h: HWND, pendiente: HRGN, antes: POINT, ini: usize, fin: usize, total: usize, pie: Option<i32>) -> bool {
    let mut quieto = None;
    let d = desplazamiento(h);
    unsafe {
        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        GetClientRect(h, &mut r);
        if (d.x, d.y) == (antes.x, antes.y) && colocada(h, ini) {
            quieto = pie.filter(|&y| fin < total && colocada(h, fin + 1) && posicion(h, fin + 1).y == y);
            let lineas = RECT { top: posicion(h, ini).y.max(r.top), bottom: quieto.unwrap_or(r.bottom).min(r.bottom), ..r };
            ValidateRect(h, std::ptr::null());
            InvalidateRgn(h, pendiente, 0);
            if lineas.top < lineas.bottom {
                InvalidateRect(h, &lineas, 0);
            }
        }
        DeleteObject(pendiente);
    }
    quieto.is_some()
}

/// Hace algo con el control oculto, si `si`: oculto no coloca el texto tras cada cambio de párrafo, aunque esté
/// congelado (18.000 párrafos de un .md de 900 KB, de 4 s a menos de uno). Al volver a enseñarlo, recupera el foco.
pub fn oculto<R>(h: HWND, si: bool, f: impl FnOnce() -> R) -> R {
    if !si {
        return f();
    }
    let con_foco = unsafe { GetFocus() } == h;
    unsafe { ShowWindow(h, SW_HIDE) };
    let _vuelta = AlSalir(|| unsafe {
        ShowWindow(h, SW_SHOWNA);
        if con_foco {
            SetFocus(h);
        }
    });
    f()
}

pub fn posicion(h: HWND, c: usize) -> POINT {
    let mut p = POINT { x: 0, y: 0 };
    enviar(h, EM_POSFROMCHAR, &mut p as *mut POINT as usize, c as isize);
    p
}

/// Dónde empieza la primera línea que se ve y dónde la que va `n` más abajo (o el final), sin preguntar por píxeles.
pub fn lineas_visibles(h: HWND, n: usize) -> (usize, usize) {
    let l = enviar(h, EM_GETFIRSTVISIBLELINE, 0, 0).max(0) as usize;
    let inicio = |l: usize| enviar(h, EM_LINEINDEX, l, 0);
    let fin = inicio(l + n);
    (inicio(l).max(0) as usize, if fin < 0 { usize::MAX } else { fin as usize })
}

/// Dónde empieza la primera línea que se ve y el último carácter que se ve, el de abajo a la derecha del texto. Lo de
/// más abajo no se pregunta: cada posición de una línea que RichEdit aún no ha colocado le obliga a colocar hasta ella.
pub fn lo_que_se_ve(h: HWND) -> (usize, usize) {
    let l = enviar(h, EM_GETFIRSTVISIBLELINE, 0, 0).max(0) as usize;
    let r = formato(h);
    let abajo = POINTL { x: r.right - 1, y: r.bottom - 1 };
    let fin = enviar(h, EM_CHARFROMPOS, 0, &abajo as *const POINTL as isize);
    (enviar(h, EM_LINEINDEX, l, 0).max(0) as usize, fin.max(0) as usize)
}

/// ¿Ha colocado ya RichEdit la línea que empieza en `a`? Justo tras algunos cambios (romper una valla de código)
/// responde con las líneas de antes.
pub fn colocada(h: HWND, a: usize) -> bool {
    enviar(h, EM_LINEINDEX, enviar(h, EM_EXLINEFROMCHAR, 0, a as isize) as usize, 0) == a as isize
}

/// El rectángulo donde va el texto, como lo coloca RichEdit: con zoom agranda o encoge lo que deja a cada lado de la
/// ventana (los márgenes y el hueco de arriba), y EM_GETRECT lo da sin zoom. Al 80 %, lo que Bloc pintaba en el margen
/// tapaba la primera y la última letra de cada línea.
pub fn formato(h: HWND) -> RECT {
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    enviar(h, EM_GETRECT, 0, &mut r as *mut RECT as isize);
    let (mut n, mut d) = (0i32, 0i32);
    enviar(h, EM_GETZOOM, &mut n as *mut i32 as usize, &mut d as *mut i32 as isize);
    if n <= 0 || d <= 0 || n == d {
        return r;
    }
    let mut c = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { GetClientRect(h, &mut c) };
    let z = |x: i32| (2 * x * n + d) / (2 * d); // como RichEdit, la mitad hacia arriba
    RECT { left: c.left + z(r.left - c.left), top: c.top + z(r.top - c.top), right: c.right - z(c.right - r.right), bottom: c.bottom - z(c.bottom - r.bottom) }
}

/// El centro (sin el desplazamiento horizontal) y el alto de la línea de una raya.
pub type FormaRaya = (i32, i32);

/// Dónde va cada raya `---` (dónde empieza y acaba su texto) encima de lo que ha pintado RichEdit: de lado a lado del
/// texto, de `grueso` píxeles y a media altura de su línea. Las que quedan por debajo de lo que se ve no.
///
/// `forma`, el centro (sin el desplazamiento horizontal) y el alto de la línea, es igual en todas las rayas mientras
/// no cambien el ancho, el zoom ni la letra: si no se sabe, se mide en la primera y se devuelve. Así cada raya pregunta
/// solo dónde está su línea: en un documento grande con mucho formato, cada pregunta de posición le cuesta a RichEdit
/// hasta 1,5 ms, y más cuanto más abajo.
///
/// `bajo`, dónde acaba cada título que lleva raya debajo: va al pie de su espacio de después (ver md::Parrafo), justo
/// encima de la línea siguiente.
///
/// Devuelve None, sin dibujar nada, si RichEdit aún no ha colocado alguna de esas líneas: justo tras algunos cambios
/// (romper una valla de código) responde con las líneas de antes y la raya saldría una línea más arriba.
pub fn rayas(h: HWND, rayas: &[(usize, usize)], bajo: &[usize], total: usize, grueso: i32, mut forma: Option<FormaRaya>) -> Option<(Option<FormaRaya>, Vec<RECT>)> {
    let texto = formato(h);
    let abajo = texto.bottom;
    let dx = desplazamiento(h).x;
    let mut hechas = Vec::new();
    for &(a, b) in rayas {
        if !colocada(h, a) {
            return None;
        }
        let ia = posicion(h, a);
        if ia.y >= abajo {
            break;
        }
        let (cx, alto) = forma.unwrap_or_else(|| {
            let ib = posicion(h, b);
            // En la última línea, el alto de la de encima (casi siempre una vacía, de la misma letra).
            let alto = if b < total {
                posicion(h, b + 1).y - ia.y
            } else if a > 0 {
                ia.y - posicion(h, a - 1).y
            } else {
                grueso * 10
            };
            ((ia.x + ib.x) / 2 + dx, alto)
        });
        if b < total {
            forma = Some((cx, alto));
        }
        let y = ia.y + alto / 2;
        hechas.push(RECT { left: texto.left, top: y, right: texto.right, bottom: y + grueso });
    }
    for &f in bajo {
        if !colocada(h, f + 1) {
            return None;
        }
        let y = posicion(h, f + 1).y - grueso;
        if y >= abajo {
            break;
        }
        hechas.push(RECT { left: texto.left, top: y, right: texto.right, bottom: y + grueso });
    }
    Some((forma, hechas))
}

/// Pinta del color `fondo` lo que queda a los lados de `texto` (el rectángulo del texto del control) y encima.
pub fn limpiar_margenes(h: HWND, texto: &RECT, fondo: u32) {
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { GetClientRect(h, &mut r) };
    let lados = [RECT { right: texto.left, ..r }, RECT { left: texto.right, ..r }, RECT { bottom: texto.top, ..r }];
    rellenar(h, &lados.into_iter().filter(|x| x.left < x.right && x.top < x.bottom).collect::<Vec<_>>(), fondo);
}

pub fn rellenar(h: HWND, rayas: &[RECT], color: u32) {
    if rayas.is_empty() {
        return;
    }
    unsafe {
        let dc = GetDC(h);
        let pincel = CreateSolidBrush(color);
        for r in rayas {
            FillRect(dc, r, pincel);
        }
        DeleteObject(pincel);
        ReleaseDC(h, dc);
    }
}

/// Lo que ha repintado RichEdit desde la última vez que se preguntó: fuera de ahí sigue lo que Bloc dibujó encima. None
/// si no se sabe (no ha pasado por WM_PAINT): entonces vale toda la ventana.
pub fn repintado() -> Option<RECT> {
    REPINTADO.with(|x| x.take())
}

/// Cambia el color `de` (el fondo de la página) por el de cada zona en lo que ha pintado RichEdit: el fondo de los
/// bloques de código y de las citas, que él no pinta a lo ancho ni entre línea y línea. El borde suavizado de las letras
/// se queda mezclado con el fondo de la página, que de tan parecido no se nota. En oscuro, RichEdit pinta el fondo de
/// detrás de las letras un punto más claro que el de la página (#1f1f1f sobre #1e1e1e): ese también cambia.
///
/// Leer de la pantalla espera a que se acabe de pintar (unos 3 ms): se lee una vez lo que abarcan todas las zonas, se
/// cambia en memoria y se vuelve a poner de una vez.
pub fn colorear_fondo(h: HWND, zonas: &[(RECT, u32)], de: u32) {
    let zonas: Vec<&(RECT, u32)> = zonas.iter().filter(|z| z.1 != de).collect();
    if zonas.is_empty() {
        return;
    }
    let casi = if de & 0xFF < 0xFF && de >> 8 & 0xFF < 0xFF && de >> 16 & 0xFF < 0xFF { de + 0x01_0101 } else { de };
    unsafe {
        let mut cliente = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        GetClientRect(h, &mut cliente);
        let corte = |r: &RECT| RECT { left: r.left.max(cliente.left), top: r.top.max(cliente.top), right: r.right.min(cliente.right), bottom: r.bottom.min(cliente.bottom) };
        let caja = zonas.iter().map(|z| corte(&z.0)).fold(RECT { left: i32::MAX, top: i32::MAX, right: i32::MIN, bottom: i32::MIN }, |c, r| RECT {
            left: c.left.min(r.left),
            top: c.top.min(r.top),
            right: c.right.max(r.right),
            bottom: c.bottom.max(r.bottom),
        });
        let (ancho, alto) = (caja.right - caja.left, caja.bottom - caja.top);
        if ancho <= 0 || alto <= 0 {
            return;
        }
        // El cursor, escondido: va dibujado invirtiendo lo que tiene debajo y, al apagarse, dejaría una raya del fondo
        // de antes.
        let escondido = HideCaret(h) != 0;
        let dc = GetDC(h);
        let mem = CreateCompatibleDC(dc);
        let copia = CreateCompatibleBitmap(dc, ancho, alto);
        let copia_antes = SelectObject(mem, copia);
        BitBlt(mem, 0, 0, ancho, alto, dc, caja.left, caja.top, SRCCOPY);
        let mascara_dc = CreateCompatibleDC(dc);
        for (r, a) in zonas.iter().map(|z| (corte(&z.0), z.1)) {
            let (x, y, w, alto_z) = (r.left - caja.left, r.top - caja.top, r.right - r.left, r.bottom - r.top);
            if w <= 0 || alto_z <= 0 {
                continue;
            }
            let pincel = CreateSolidBrush(a);
            let pincel_antes = SelectObject(mem, pincel);
            let mascara = CreateBitmap(w, alto_z, 1, 1, std::ptr::null());
            let mascara_antes = SelectObject(mascara_dc, mascara);
            for &color in &[de, casi][..if casi == de { 1 } else { 2 }] {
                // De color a blanco y negro: blanco lo que es del color de fondo del origen.
                SetBkColor(mem, color);
                BitBlt(mascara_dc, 0, 0, w, alto_z, mem, x, y, SRCCOPY);
                // De blanco y negro a color, el blanco va al color de fondo del destino y el negro al del texto: con
                // DSPDxax, el pincel donde la máscara es blanca y lo que había donde es negra.
                SetBkColor(mem, 0xFF_FFFF);
                SetTextColor(mem, 0);
                BitBlt(mem, x, y, w, alto_z, mascara_dc, 0, 0, 0x00E2_0746);
            }
            SelectObject(mascara_dc, mascara_antes);
            DeleteObject(mascara);
            SelectObject(mem, pincel_antes);
            DeleteObject(pincel);
        }
        BitBlt(dc, caja.left, caja.top, ancho, alto, mem, 0, 0, SRCCOPY);
        DeleteDC(mascara_dc);
        SelectObject(mem, copia_antes);
        DeleteObject(copia);
        DeleteDC(mem);
        ReleaseDC(h, dc);
        if escondido {
            ShowCaret(h);
        }
    }
}

/// Lo que trae el portapapeles si es solo una imagen: el PNG si viene, si no el mapa de bits (CF_DIB). Con texto o con
/// archivos, nada: lo que copia Excel o Word lleva también una imagen y se pega como texto.
fn imagen_del_portapapeles(h: HWND) -> Option<crate::imagen::Imagen> {
    use crate::imagen::Imagen;
    static PNG: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT as u32) != 0 || IsClipboardFormatAvailable(15) != 0 {
            return None;
        }
        let png = *PNG.get_or_init(|| RegisterClipboardFormatW(ws("PNG").as_ptr()));
        if IsClipboardFormatAvailable(png) == 0 && IsClipboardFormatAvailable(8) == 0 {
            return None;
        }
        if !abrir_portapapeles(h) {
            return None;
        }
        let bytes = |f: u32| leer_memoria(GetClipboardData(f), |p, n| std::slice::from_raw_parts(p as *const u8, n).to_vec());
        let im = bytes(png).map(Imagen::Png).or_else(|| bytes(8).map(Imagen::Dib)); // CF_DIB
        CloseClipboard();
        im
    }
}

/// Los archivos y carpetas copiados en el Explorador (lo que trae CF_HDROP), en su orden.
unsafe fn archivos_copiados(d: windows_sys::Win32::Foundation::HANDLE) -> Vec<Vec<u16>> {
    use windows_sys::Win32::UI::Shell::{DragQueryFileW, HDROP};
    if d.is_null() {
        return Vec::new();
    }
    let n = unsafe { DragQueryFileW(d as HDROP, u32::MAX, std::ptr::null_mut(), 0) };
    (0..n)
        .map(|i| {
            let largo = unsafe { DragQueryFileW(d as HDROP, i, std::ptr::null_mut(), 0) } as usize;
            let mut b = vec![0u16; largo + 1];
            unsafe { DragQueryFileW(d as HDROP, i, b.as_mut_ptr(), b.len() as u32) };
            b.truncate(largo);
            b
        })
        .filter(|b| !b.is_empty())
        .collect()
}

/// El texto del portapapeles. `con_formato`: si trae HTML (navegadores, Word, Outlook) con algo que Markdown sepa
/// decir, código de un editor o una tabla con tabuladores, ya convertido; si no, el texto tal cual, que entonces ni se
/// copia. Con `true`, lo convertido es un bloque y va en sus propias líneas.
fn texto_del_portapapeles(h: HWND, con_formato: bool) -> Option<(Vec<u16>, bool)> {
    static HTML: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    static VSCODE: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    // Se cierra pase lo que pase: abierto, ningún programa podría copiar ni pegar.
    struct Cerrar;
    impl Drop for Cerrar {
        fn drop(&mut self) {
            unsafe { CloseClipboard() };
        }
    }
    let (html, vscode, texto, archivos) = unsafe {
        if !abrir_portapapeles(h) {
            return None;
        }
        let _cerrar = Cerrar;
        let archivos = archivos_copiados(GetClipboardData(15)); // CF_HDROP
        let html = if con_formato {
            let formato = *HTML.get_or_init(|| RegisterClipboardFormatW(ws("HTML Format").as_ptr()));
            leer_memoria(GetClipboardData(formato), |p, n| std::slice::from_raw_parts(p as *const u8, n).to_vec())
        } else {
            None
        };
        // La lengua del código copiado en VS Code (un JSON corto).
        let vscode = if con_formato {
            let formato = *VSCODE.get_or_init(|| RegisterClipboardFormatW(ws("vscode-editor-data").as_ptr()));
            leer_memoria(GetClipboardData(formato), |p, n| String::from_utf8_lossy(std::slice::from_raw_parts(p as *const u8, n.min(4096))).into_owned())
        } else {
            None
        };
        let texto = leer_memoria(GetClipboardData(CF_UNICODETEXT as u32), |p, n| {
            let p = p as *const u16;
            let n = (0..n / 2).take_while(|&i| *p.add(i) != 0).count();
            std::slice::from_raw_parts(p, n).to_vec()
        });
        (html, vscode, texto, archivos)
    };
    // Archivos copiados en el Explorador (M4): sus rutas, o sus enlaces en un .md.
    if !archivos.is_empty() {
        return Some((ventana::archivos_al_pegar(&archivos, con_formato), false));
    }
    // Se convierte ya cerrado: lo copiado de una web grande tardaría y los demás programas no podrían copiar ni pegar.
    // Más de 4 MB de HTML (un rango grande de Excel, una página entera) se pega como texto: convertirlo pararía la ventana.
    if !con_formato || html.as_ref().is_some_and(|b| b.len() > 4 << 20) {
        return texto.map(|t| (t, false));
    }
    let llano = texto.as_ref().map(|t| String::from_utf16_lossy(t));
    let lengua = vscode.map(|v| crate::html::lengua_de_vscode(&v)).unwrap_or_default();
    match crate::html::pegado(html.map(|b| crate::html::fragmento(&b)).as_deref(), llano.as_deref(), &lengua) {
        Some((md, bloque)) => Some((md.encode_utf16().collect(), bloque)),
        None => texto.map(|t| (t, false)),
    }
}

/// Lo que hay en un bloque de memoria del portapapeles, leído con `leer(puntero, bytes)`.
unsafe fn leer_memoria<T>(d: windows_sys::Win32::Foundation::HANDLE, leer: impl FnOnce(*const std::ffi::c_void, usize) -> T) -> Option<T> {
    if d.is_null() {
        return None;
    }
    unsafe {
        let p = GlobalLock(d);
        if p.is_null() {
            return None;
        }
        let r = leer(p, GlobalSize(d));
        GlobalUnlock(d);
        Some(r)
    }
}

/// OpenClipboard con unos reintentos: otro programa (el historial de Win+V, un escritorio remoto) puede tenerlo abierto
/// un momento, y a la primera Ctrl+C no copiaría ni Ctrl+V pegaría, sin avisar.
fn abrir_portapapeles(h: HWND) -> bool {
    for i in 0..10 {
        if unsafe { OpenClipboard(h) } != 0 {
            return true;
        }
        if i < 9 {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    false
}

fn al_portapapeles(h: HWND, t: &[u16]) -> bool {
    unsafe {
        if !abrir_portapapeles(h) {
            return false;
        }
        EmptyClipboard();
        let m = GlobalAlloc(GMEM_MOVEABLE, (t.len() + 1) * 2);
        let mut bien = false;
        if !m.is_null() {
            let p = GlobalLock(m) as *mut u16;
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(t.as_ptr(), p, t.len());
                *p.add(t.len()) = 0;
                GlobalUnlock(m);
                bien = !SetClipboardData(CF_UNICODETEXT as u32, m).is_null();
            }
            // Solo si el portapapeles se ha quedado con ella deja de ser nuestra.
            if !bien {
                GlobalFree(m);
            }
        }
        CloseClipboard();
        bien
    }
}

/// `con_formato`: en un Markdown (y fuera de un bloque de código), lo copiado con formato se pega convertido.
pub fn pegar(h: HWND, con_formato: bool) {
    // Una imagen sola (sin texto ni archivos): en un .md se guarda junto a él (M5).
    if con_formato && let Some(im) = imagen_del_portapapeles(h) {
        ventana::imagen_al_pegar(&im);
        return;
    }
    // Una dirección sobre lo seleccionado: el enlace, aunque lo copiado traiga también el suyo con título en HTML.
    let (a, b) = seleccion(h);
    let enlace = (con_formato && a != b).then(|| texto_del_portapapeles(h, false)).flatten().and_then(|(t, _)| ventana::enlace_al_pegar(&crate::fichero::normalizar(&t)));
    let t = enlace.or_else(|| {
        texto_del_portapapeles(h, con_formato && ventana::pegar_con_formato()).map(|(t, bloque)| {
            let t = crate::fichero::normalizar(&t);
            if bloque { ventana::en_sus_lineas(&t) } else { t }
        })
    });
    if let Some(t) = t {
        // Lo pegado se deshace solo, sin lo escrito justo antes ni justo después.
        ventana::cortar_deshacer();
        editando(|| sustituir(h, &t));
        ventana::cortar_deshacer();
    }
}

/// Copia la selección como texto de Windows (saltos CRLF). Devuelve si había algo.
pub fn copiar(h: HWND) -> bool {
    let (a, b) = seleccion(h);
    if a == b {
        return false;
    }
    let mut t = rango(h, a, b);
    como_archivo(&mut t);
    al_portapapeles(h, &crate::fichero::con_saltos(&t, crate::fichero::Saltos::Crlf))
}

/// Las teclas que usa la vista previa: moverse y seleccionar (con Mayús), Ctrl+A y Ctrl+C.
fn de_la_vista(tecla: u16) -> bool {
    matches!(tecla, VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN | VK_HOME | VK_END | VK_PRIOR | VK_NEXT | VK_SHIFT | VK_CONTROL)
        || pulsada(VK_CONTROL) && !pulsada(VK_MENU) && matches!(tecla, 0x41 | 0x43 | VK_INSERT)
}

/// El botón derecho en la vista previa: solo Copiar y Seleccionar todo.
fn menu_de_la_vista(h: HWND, lp: LPARAM) {
    let t = crate::textos::t();
    let (mut x, mut y) = ((lp & 0xFFFF) as i16 as i32, ((lp >> 16) & 0xFFFF) as i16 as i32);
    if lp == -1 {
        let mut p = POINT { x: 0, y: 0 };
        unsafe {
            GetCaretPos(&mut p);
            ClientToScreen(h, &mut p);
        }
        (x, y) = (p.x, p.y);
    }
    let (a, b) = seleccion(h);
    unsafe {
        let m = CreatePopupMenu();
        let copiar_s = ws(t.copiar);
        let todo_s = ws(t.seleccionar_todo);
        AppendMenuW(m, MF_STRING | if a == b { MF_GRAYED } else { 0 }, 1, copiar_s.as_ptr());
        AppendMenuW(m, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(m, MF_STRING, 2, todo_s.as_ptr());
        let elegido = TrackPopupMenuEx(m, TPM_RIGHTBUTTON | TPM_RETURNCMD, x, y, h, std::ptr::null());
        DestroyMenu(m);
        match elegido {
            1 => {
                copiar(h);
            }
            2 => seleccionar(h, 0, largo(h)),
            _ => {}
        }
    }
}

pub fn cortar(h: HWND) {
    if copiar(h) {
        editando(|| sustituir(h, &[]));
    }
}

/// Lo girado con la rueda, en pasos enteros de `por_muesca` cada 120: lo que sobra de una rueda fina o de un panel
/// táctil se guarda en `resto` para la vez siguiente, y se olvida al girar al revés.
fn muescas(resto: &'static std::thread::LocalKey<Cell<i32>>, delta: i32, por_muesca: i32) -> i32 {
    resto.with(|r| {
        let mut a = r.get();
        if (a < 0) != (delta < 0) {
            a = 0;
        }
        a += delta * por_muesca;
        r.set(a % 120);
        a / 120
    })
}

// Quita el carácter que Windows ya ha preparado para la tecla que se acaba de atender aquí.
fn quitar_caracter(h: HWND) {
    unsafe {
        let mut m: MSG = std::mem::zeroed();
        while PeekMessageW(&mut m, h, WM_CHAR, WM_CHAR, PM_REMOVE) != 0 {}
    }
}

unsafe extern "system" fn subclase(h: HWND, m: u32, wp: WPARAM, lp: LPARAM, _: usize, _: usize) -> LRESULT {
    seguro(|| mensaje(h, m, wp, lp), || 0)
}

// --- El clic que activa la ventana (L10, 1.3) ---
// Con otra ventana delante, pulsar y arrastrar sobre el texto no seleccionaba, y no se ha podido reproducir sin un ratón de
// verdad. Desde el clic que activa la ventana hasta soltar el botón se apunta el orden de los mensajes; si se arrastró y no
// quedó nada seleccionado, va en una línea a errores.log (sin texto ni rutas). Los demás clics no apuntan nada.

struct Rastro {
    pasos: Vec<&'static str>,
    desde: std::time::Instant,
    pulsado: Option<(i32, i32)>,
    arrastres: u32,
}

thread_local!(static RASTRO: RefCell<Option<Rastro>> = const { RefCell::new(None) });

pub fn rastro(paso: &'static str) {
    RASTRO.with(|r| {
        if let Some(r) = r.borrow_mut().as_mut() {
            r.pasos.push(paso);
        }
    });
}

fn rastro_mensaje(h: HWND, m: u32, lp: LPARAM) {
    let punto = ((lp & 0xFFFF) as i16 as i32, (lp >> 16) as i16 as i32);
    match m {
        // En la vista previa, cada clic: allí no se ha podido seleccionar nunca (25/11).
        WM_MOUSEACTIVATE if ventana::solo_ver() || unsafe { GetForegroundWindow() != GetAncestor(h, GA_ROOT) } => {
            let primero = if ventana::solo_ver() { "vista previa" } else { "activar" };
            RASTRO.with(|r| *r.borrow_mut() = Some(Rastro { pasos: vec![primero], desde: std::time::Instant::now(), pulsado: None, arrastres: 0 }));
        }
        WM_MOUSEMOVE => RASTRO.with(|r| {
            if let Some(r) = r.borrow_mut().as_mut().filter(|r| r.pulsado.is_some()) {
                r.arrastres += 1;
            }
        }),
        WM_SETFOCUS => rastro("editor:foco"),
        WM_KILLFOCUS => rastro("editor:sin foco"),
        WM_CAPTURECHANGED => rastro("captura perdida"),
        WM_LBUTTONDOWN => RASTRO.with(|r| {
            if let Some(r) = r.borrow_mut().as_mut() {
                r.pasos.push(if unsafe { GetFocus() } == h { "botón (con foco)" } else { "botón (sin foco)" });
                r.pulsado = Some(punto);
            }
        }),
        WM_LBUTTONUP => {
            let Some(r) = RASTRO.with(|r| r.borrow_mut().take()) else { return };
            let (s, e) = seleccion(h);
            let movido = r.pulsado.is_some_and(|(x, y)| (x - punto.0).abs() + (y - punto.1).abs() > 8);
            if movido && s == e && r.desde.elapsed().as_secs() < 30 {
                let n = r.arrastres;
                crate::w::apuntar(&format!("Clic que activa la ventana, arrastrado sin seleccionar: {} → {n} movimientos con el botón", r.pasos.join(" → ")));
            }
        }
        _ => {}
    }
}

/// ↑ en la primera línea a la vista lleva al principio del documento, y ↓ en la última al final, con Mayús alargando la
/// selección, como el Bloc de notas y Word: RichEdit se queda donde está. Si la flecha no ha movido nada, se hace su
/// Ctrl+Inicio o Ctrl+Fin, con Ctrl puesto en el estado del teclado de este hilo mientras tanto.
fn flecha(h: HWND, wp: WPARAM, lp: LPARAM, pasar: impl FnOnce() -> LRESULT) -> LRESULT {
    let vk = wp as u16;
    if !matches!(vk, VK_UP | VK_DOWN) || pulsada(VK_CONTROL) || pulsada(VK_MENU) {
        return pasar();
    }
    let antes = seleccion(h);
    let r = pasar();
    if seleccion(h) == antes {
        let mut k = [0u8; 256];
        unsafe {
            GetKeyboardState(k.as_mut_ptr());
            let ctrl = k[VK_CONTROL as usize];
            k[VK_CONTROL as usize] |= 0x80;
            SetKeyboardState(k.as_ptr());
            por_richedit(h, WM_KEYDOWN, if vk == VK_UP { VK_HOME } else { VK_END } as WPARAM, lp);
            k[VK_CONTROL as usize] = ctrl;
            SetKeyboardState(k.as_ptr());
        }
    }
    r
}

fn mensaje(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if matches!(m, WM_MOUSEACTIVATE | WM_SETFOCUS | WM_KILLFOCUS | WM_CAPTURECHANGED | WM_LBUTTONDOWN | WM_LBUTTONUP | WM_MOUSEMOVE) {
        rastro_mensaje(h, m, lp);
    }
    if ventana::solo_ver() {
        match m {
            // Copiar, como en Bloc: el texto del archivo, sin los saltos que pone el ajuste de línea.
            WM_KEYDOWN if pulsada(VK_CONTROL) && matches!(wp as u16, 0x43 | VK_INSERT) => {
                copiar(h);
                return 0;
            }
            // Lo que no es moverse ni seleccionar va primero al Explorador (Tab y F6 le devuelven el foco). Esc, Tab y
            // F6 no hacen nada aquí aunque él no los quiera.
            WM_KEYDOWN | WM_SYSKEYDOWN if !de_la_vista(wp as u16) => {
                let mut msg: MSG = unsafe { std::mem::zeroed() };
                (msg.hwnd, msg.message, msg.wParam, msg.lParam) = (h, m, wp, lp);
                if crate::previa::al_explorador(&msg) || matches!(wp as u16, VK_TAB | VK_F6 | VK_ESCAPE) {
                    return 0;
                }
                return unsafe { DefSubclassProc(h, m, wp, lp) };
            }
            WM_CHAR if matches!(wp, 9 | 27) => return 0,
            WM_CONTEXTMENU => {
                menu_de_la_vista(h, lp);
                return 0;
            }
            WM_KEYDOWN => return flecha(h, wp, lp, || unsafe { DefSubclassProc(h, m, wp, lp) }),
            // Lo demás del teclado y lo que edita, a RichEdit, que respeta el solo lectura (y da Ctrl+A y las
            // flechas): las acciones de Bloc escriben con EM_REPLACESEL, que no lo respeta.
            WM_KEYUP | WM_CHAR | WM_SYSKEYDOWN | WM_PASTE | WM_CUT | WM_CLEAR | WM_UNDO | EM_UNDO => {
                return unsafe { DefSubclassProc(h, m, wp, lp) };
            }
            _ => {}
        }
    }
    if matches!(m, WM_KEYDOWN | WM_LBUTTONDOWN) {
        ventana::recordar_tabla();
    }
    match m {
        WM_PASTE => {
            pegar(h, true);
            return 0;
        }
        WM_COPY => {
            copiar(h);
            return 0;
        }
        WM_CUT => {
            cortar(h);
            return 0;
        }
        WM_UNDO | EM_UNDO if ventana::deshacer_propio() => {
            ventana::deshacer();
            return 0;
        }
        WM_SYSKEYDOWN if wp == VK_BACK as usize && ventana::deshacer_propio() => {
            ventana::deshacer();
            return 0;
        }
        WM_CONTEXTMENU => {
            ventana::menu_contextual(lp);
            return 0;
        }
        WM_TIMER if wp == RELOJ_CURSOR => {
            paso_cursor(h);
            return 0;
        }
        WM_PAINT => {
            // Al desplazarse en horizontal (sin ajuste de línea), RichEdit mueve la ventana entera, márgenes incluidos, y
            // solo repinta la tira que destapa: el margen entraba en el texto y el texto (y lo de encima) en el margen.
            let x = desplazamiento(h).x;
            if VISTA_X.with(|v| v.replace(x)) != x {
                unsafe { InvalidateRect(h, std::ptr::null(), 0) };
            }
            let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            if unsafe { GetUpdateRect(h, &mut r, 0) } != 0 {
                REPINTADO.with(|x| {
                    x.set(Some(x.get().map_or(r, |a| RECT { left: a.left.min(r.left), top: a.top.min(r.top), right: a.right.max(r.right), bottom: a.bottom.max(r.bottom) })))
                });
            }
            // A mitad del trabajo de RichEdit o con la vista congelada (ver `quieto`), lo de encima se dibuja después.
            let anidado = EN_RICHEDIT.with(Cell::get) > 0 || QUIETO.with(Cell::get) > 0;
            let r = por_richedit(h, m, wp, lp);
            if anidado {
                SIN_ENCIMA.with(|s| s.set(true));
            } else {
                ventana::dibujar_rayas(h);
                ventana::dibujar_tablas(h);
                ventana::dibujar_imagenes(h);
            }
            return r;
        }
        // En la vista previa, el clic va entero al texto: la ventana del Explorador que la aloja podría tragárselo, y
        // una tabla no se abre (solo se ve).
        WM_MOUSEACTIVATE if ventana::solo_ver() => return MA_ACTIVATE as LRESULT,
        WM_LBUTTONDOWN if !pulsada(VK_SHIFT) && !ventana::solo_ver() && ventana::clic_en_tabla(h, (lp & 0xFFFF) as i16 as i32, (lp >> 16) as i16 as i32) => return 0,
        // El clic que activa la ventana puede llegar antes de que el foco haya vuelto al texto (lo devuelve la ventana
        // en su WM_SETFOCUS): sin foco, RichEdit no empieza a seleccionar al arrastrar. Se le da antes del botón.
        WM_LBUTTONDOWN if unsafe { GetFocus() } != h => {
            unsafe { SetFocus(h) };
            let r = por_richedit(h, m, wp, lp);
            rastro(if unsafe { GetCapture() } == h { "captura" } else { "sin captura" });
            return r;
        }
        WM_DROPFILES => {
            ventana::soltar(wp as _);
            return 0;
        }
        WM_MOUSEWHEEL if pulsada(VK_CONTROL) => {
            let n = muescas(&RUEDA_ZOOM, (wp >> 16) as i16 as i32, 1);
            if n != 0 {
                ventana::zoom(n);
            }
            return 0;
        }
        // RichEdit anima cada muesca y pone las siguientes en cola: girar deprisa se quedaba atrás (15 muescas seguían
        // moviendo la vista más de un segundo). Se mueve aquí y al momento, con las líneas por muesca de Windows.
        WM_MOUSEWHEEL if !pulsada(VK_SHIFT) => {
            let mut lineas = 3u32;
            unsafe { SystemParametersInfoW(SPI_GETWHEELSCROLLLINES, 0, &mut lineas as *mut u32 as *mut _, 0) };
            let delta = (wp >> 16) as i16 as i32;
            if lineas == u32::MAX {
                // Una pantalla por muesca.
                let n = muescas(&RUEDA, delta, 1);
                for _ in 0..n.abs() {
                    enviar(h, WM_VSCROLL, if n > 0 { SB_PAGEUP } else { SB_PAGEDOWN } as usize, 0);
                }
            } else {
                let n = muescas(&RUEDA, delta, lineas.min(100) as i32);
                if n != 0 {
                    enviar(h, EM_LINESCROLL, 0, -n as isize);
                }
            }
            return 0;
        }
        WM_CHAR if (wp < 0x20 && !matches!(wp, 8 | 9 | 13)) || wp == 0x7F => return 0, // Ctrl+letra y Ctrl+Retroceso
        // RichEdit solo salta de línea con la tecla Intro. El Intro que llega solo como carácter (teclados en pantalla,
        // programas que escriben) se atiende aquí; el que sigue a la tecla ya está hecho.
        WM_CHAR if wp == 13 => {
            if !INTRO_TECLA.with(|t| t.replace(false)) && !ventana::accion(|| editando(ventana::intro)) {
                sustituir(h, &[13]);
            }
            return 0;
        }
        // Deshacer por palabras, como en los Markdown: RichEdit junta todo lo escrito seguido en un solo paso; un
        // espacio tras algo que no lo es empieza otro.
        WM_CHAR if !ventana::deshacer_propio() => {
            let espacio = wp == 32;
            if espacio && !ESPACIO.with(Cell::get) {
                enviar(h, EM_STOPGROUPTYPING, 0, 0);
            }
            ESPACIO.with(|e| e.set(espacio));
        }
        WM_KEYDOWN => {
            let (ctrl, mayus, alt) = (pulsada(VK_CONTROL), pulsada(VK_SHIFT), pulsada(VK_MENU));
            let vk = wp as u16;
            INTRO_TECLA.with(|t| t.set(false));
            if vk == VK_HOME && !ctrl && !alt && ventana::inicio(mayus) {
                return 0;
            }
            if vk == VK_INSERT && mayus && !ctrl {
                pegar(h, true);
                return 0;
            }
            if vk == VK_INSERT && ctrl && !mayus {
                copiar(h);
                return 0;
            }
            if vk == VK_DELETE && mayus && !ctrl {
                cortar(h);
                return 0;
            }
            if vk == VK_BACK && !ctrl && !alt && !mayus && ventana::accion(ventana::retroceso) {
                quitar_caracter(h);
                return 0;
            }
            if vk == VK_DELETE && !ctrl && !alt && !mayus && ventana::accion(ventana::suprimir) {
                return 0;
            }
            // Tab con algo seleccionado, o en un elemento de lista, mete las líneas en vez de escribir un tabulador.
            if vk == VK_TAB && !ctrl && !alt {
                if ventana::accion(|| ventana::celda(!mayus)) {
                    quitar_caracter(h);
                    return 0;
                }
                let (s, e) = seleccion(h);
                if mayus || s != e || ventana::en_lista() {
                    quitar_caracter(h);
                    ventana::accion(|| ventana::sangrar(!mayus));
                    return 0;
                }
            }
            if vk == VK_RETURN && !ctrl && !alt {
                if ventana::accion(|| editando(ventana::intro)) {
                    quitar_caracter(h);
                    return 0;
                }
                INTRO_TECLA.with(|t| t.set(true));
            }
            // RichEdit trae atajos de Ctrl que cambian el formato (centrar, interlineado, tamaño...). Solo pasan los
            // de moverse y borrar; los de Bloc los atiende antes su menú.
            let permitida = matches!(vk, VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN | VK_HOME | VK_END | VK_PRIOR | VK_NEXT | VK_BACK | VK_DELETE | VK_CONTROL | VK_SHIFT | VK_MENU);
            if ctrl && !alt && !permitida {
                quitar_caracter(h);
                return 0;
            }
        }
        _ => {}
    }
    // Lo que entra de una vez coge la letra del sitio en que cae, y la diferencia de textos puede no verlo entero (si
    // repite lo que lo sigue o es igual que lo que había): se pinta entero. Vale para lo de Bloc (pegar, deshacer,
    // botones) y para lo que escriba otro programa.
    // Un JUNTO de verdad, tecleado o pegado, no se distinguiría de un salto blando: ese documento deja de juntar.
    let junto = crate::md::JUNTO;
    if junta()
        && (m == WM_CHAR && wp == junto as usize
            || m == EM_REPLACESEL && lp != 0 && (0..).map(|i| unsafe { *(lp as *const u16).add(i) }).take_while(|&c| c != 0).any(|c| c == junto))
    {
        ventana::sin_juntar();
    }
    if m == EM_REPLACESEL && lp != 0 {
        let n = (0..).take_while(|&i| unsafe { *(lp as *const u16).add(i) } != 0).count();
        let (a, _) = seleccion(h);
        ventana::forzar(a, a + n);
    }
    let edita = matches!(m, WM_CHAR | WM_KEYDOWN | WM_CLEAR | WM_IME_CHAR | WM_IME_COMPOSITION | WM_IME_ENDCOMPOSITION) || m == EM_REPLACESEL;
    let escribe = m == WM_CHAR && wp >= 0x20 || m == WM_KEYDOWN && matches!(wp as u16, VK_BACK | VK_DELETE);
    let antes = (escribe && unsafe { GetFocus() } == h).then(cursor);
    let pasar = || if edita { editando(|| por_richedit(h, m, wp, lp)) } else { por_richedit(h, m, wp, lp) };
    let r = if m == WM_KEYDOWN { flecha(h, wp, lp, pasar) } else { pasar() };
    if m == WM_LBUTTONDOWN {
        rastro(if unsafe { GetCapture() } == h { "captura" } else { "sin captura" });
    }
    if matches!(m, WM_KEYDOWN | WM_LBUTTONUP | WM_RBUTTONUP) {
        ventana::al_mover();
        ventana::mirar_tabla();
        ventana::mirar_tablas();
    }
    // Lo que escribe o selecciona otro programa abre o cierra las tablas ya: lo siguiente que mande puede llegar antes
    // que el aviso que se deja Bloc al cambiar la selección.
    if (edita || matches!(m, EM_SETSEL | EM_EXSETSEL) && !PROPIA.with(Cell::get)) && EN_RICHEDIT.with(Cell::get) == 0 {
        ventana::mirar_tablas();
    }
    // Lo que ha cambiado una tecla, pintado antes de la siguiente, como pinta RichEdit cada letra: con las teclas
    // seguidas (escribir deprisa) el pintado se quedaba para el final y el cursor se veía en cada sitio por el que pasaba.
    if matches!(m, WM_CHAR | WM_KEYDOWN | WM_IME_CHAR) {
        unsafe { UpdateWindow(h) };
    }
    if let Some(p) = antes {
        deslizar(h, p);
    }
    r
}
