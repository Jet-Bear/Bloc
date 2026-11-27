// La vista previa del Explorador: Bloc.exe como servidor COM («Bloc.exe -Embedding», LocalServer32). El Explorador crea
// una Vista, le da el archivo como flujo (IInitializeWithStream: Bloc no lo abre por su ruta ni lo bloquea) y la
// ventana donde enseñarlo; Bloc lo enseña con su propia ventana en modo vista (ventana::vista), solo para leer. Una Vista
// a la vez por proceso; al soltarla, el proceso se ofrece otra vez con la ventana ya hecha y, si en cinco minutos nadie
// la pide, acaba. Diseño en privado\DISENO-K12.md.
use std::cell::{Cell, RefCell};
use std::ffi::c_void;

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::System::Com::{CoInitializeEx, CoRegisterClassObject, CoRevokeClassObject, CoTaskMemFree, CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED, REGCLS_SINGLEUSE};
use windows_sys::Win32::System::Ole::OleInitialize;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetFocus, SetFocus};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::com::Com;
use crate::fichero;
use crate::textos::t;
use crate::ventana;

const fn g(u: u128) -> GUID {
    GUID::from_u128(u)
}
/// Las dos clases de Bloc: la de los textos (Markdown si el archivo acaba en .md o .markdown) y la de los Markdown.
pub const CLSID_TEXTO: GUID = g(0xABEA560F_4304_4504_946A_22EBDA55A847);
pub const CLSID_MD: GUID = g(0x59237A87_8A0C_4A53_A263_DCC05BFC7936);
pub const IID_VISTA: GUID = g(0x8895B1C6_B41F_4C1C_A562_0D564250836F);
const IID_IUNKNOWN: GUID = g(0x00000000_0000_0000_C000_000000000046);
const IID_FABRICA: GUID = g(0x00000001_0000_0000_C000_000000000046);
const IID_FLUJO: GUID = g(0xB824B49D_22AC_4161_AC8A_9916E8FA3F7F);
const IID_SITIO: GUID = g(0xFC4801A3_2BA9_11CF_A229_00AA003D7352);
const IID_VENTANA: GUID = g(0x00000114_0000_0000_C000_000000000046);

const E_FAIL: i32 = 0x80004005_u32 as i32;
const E_NOINTERFACE: i32 = 0x80004002_u32 as i32;
const E_NOTIMPL: i32 = 0x80004001_u32 as i32;
const S_FALSE: i32 = 1;
const SIN_AGREGAR: i32 = 0x80040110_u32 as i32; // CLASS_E_NOAGGREGATION

/// Lo que se lee de un archivo como mucho, y de un Markdown lo que se le da formato: del resto, solo el principio.
const TOPE: usize = 1 << 20;
const TOPE_MD: usize = 256 << 10;

/// Atiende al Explorador hasta que suelta la Vista y, después, cinco minutos más por si pide otra.
pub fn servir() {
    unsafe {
        // OLE (para copiar) se pone después de enseñar el texto: tarda unos 18 ms.
        crate::w::SIN_AVISOS.store(true, std::sync::atomic::Ordering::Relaxed);
        CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
        ofrecer();
        rearmar();
        let mut m: MSG = std::mem::zeroed();
        while GetMessageW(&mut m, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&m);
            DispatchMessageW(&m);
        }
    }
}

unsafe extern "system" fn con_ole(_: HWND, _: u32, id: usize, _: u32) {
    unsafe {
        KillTimer(std::ptr::null_mut(), id);
        OleInitialize(std::ptr::null());
    }
}

/// Las dos clases, para la próxima Vista (de una sola vez cada una: con una Vista viva, otra iría a otro proceso).
fn ofrecer() {
    let mut fichas = [0u32; 2];
    for (k, f) in [&FABRICA_TEXTO, &FABRICA_MD].into_iter().enumerate() {
        let clsid = if k == 0 { &CLSID_TEXTO } else { &CLSID_MD };
        unsafe { CoRegisterClassObject(clsid, f as *const Fabrica as *mut c_void, CLSCTX_LOCAL_SERVER, REGCLS_SINGLEUSE as u32, &mut fichas[k]) };
    }
    FICHAS.with(|x| x.set(fichas));
}

fn retirar() {
    for f in FICHAS.with(|x| x.replace([0; 2])) {
        if f != 0 {
            unsafe { CoRevokeClassObject(f) };
        }
    }
}

thread_local!(static RELOJ: Cell<usize> = const { Cell::new(0) });

/// Un solo reloj: uno anterior que siguiese vivo cerraría el proceso antes de tiempo. Arrancar de nuevo cuesta unos
/// 250 ms al Explorador (pasar de un archivo a otro con el proceso vivo, 15 ms) y esperar, unos 3 MB: cinco minutos.
fn rearmar() {
    let antes = RELOJ.with(Cell::get);
    unsafe {
        if antes != 0 {
            KillTimer(std::ptr::null_mut(), antes);
        }
        RELOJ.with(|r| r.set(SetTimer(std::ptr::null_mut(), 0, 300_000, Some(sin_pedir))));
    }
}

unsafe extern "system" fn sin_pedir(_: HWND, _: u32, id: usize, _: u32) {
    unsafe { KillTimer(std::ptr::null_mut(), id) };
    RELOJ.with(|r| r.set(0));
    if !VIVA.with(Cell::get) {
        retirar();
        let h = LIBRE.with(|l| l.replace(std::ptr::null_mut()));
        // El WM_DESTROY de la ventana acaba el bucle de mensajes.
        unsafe {
            if h.is_null() {
                PostQuitMessage(0);
            } else {
                DestroyWindow(h);
            }
        }
    }
}

thread_local!(static VIVA: Cell<bool> = const { Cell::new(false) });
thread_local!(static OLE: Cell<bool> = const { Cell::new(false) });
thread_local!(static FICHAS: Cell<[u32; 2]> = const { Cell::new([0; 2]) });
// La ventana de la Vista anterior, escondida, para la siguiente.
thread_local!(static LIBRE: Cell<HWND> = const { Cell::new(std::ptr::null_mut()) });

type Qi = unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> i32;
type Cuenta = unsafe extern "system" fn(*mut c_void) -> u32;

fn igual(a: &GUID, b: &GUID) -> bool {
    a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
}

// --- La fábrica: estática, sin cuenta ---

#[repr(C)]
struct Fabrica {
    vt: &'static VtFabrica,
    md: bool,
}

#[repr(C)]
struct VtFabrica {
    qi: Qi,
    mas: Cuenta,
    menos: Cuenta,
    crear: unsafe extern "system" fn(*mut c_void, *mut c_void, *const GUID, *mut *mut c_void) -> i32,
    bloquear: unsafe extern "system" fn(*mut c_void, i32) -> i32,
}

static VT_FABRICA: VtFabrica = VtFabrica { qi: qi_fabrica, mas: uno, menos: uno, crear, bloquear };
static FABRICA_TEXTO: Fabrica = Fabrica { vt: &VT_FABRICA, md: false };
static FABRICA_MD: Fabrica = Fabrica { vt: &VT_FABRICA, md: true };

unsafe extern "system" fn uno(_: *mut c_void) -> u32 {
    1
}

unsafe extern "system" fn qi_fabrica(this: *mut c_void, iid: *const GUID, salida: *mut *mut c_void) -> i32 {
    let iid = unsafe { &*iid };
    let si = igual(iid, &IID_IUNKNOWN) || igual(iid, &IID_FABRICA);
    unsafe { *salida = if si { this } else { std::ptr::null_mut() } };
    if si { 0 } else { E_NOINTERFACE }
}

unsafe extern "system" fn crear(this: *mut c_void, fuera: *mut c_void, iid: *const GUID, salida: *mut *mut c_void) -> i32 {
    unsafe { *salida = std::ptr::null_mut() };
    if !fuera.is_null() {
        return SIN_AGREGAR;
    }
    let md = unsafe { &*(this as *const Fabrica) }.md;
    let v = Box::into_raw(Box::new(Vista {
        vt_previa: &VT_PREVIA,
        vt_flujo: &VT_FLUJO,
        vt_sitio: &VT_SITIO,
        vt_ventana: &VT_VENTANA,
        cuenta: Cell::new(1),
        md,
        padre: Cell::new(std::ptr::null_mut()),
        rect: Cell::new(RECT { left: 0, top: 0, right: 0, bottom: 0 }),
        flujo: RefCell::new(None),
        sitio: RefCell::new(None),
        ventana: Cell::new(LIBRE.with(|l| l.replace(std::ptr::null_mut()))),
    }));
    VIVA.with(|x| x.set(true));
    retirar();
    let r = qi_vista(unsafe { &*v }, iid, salida);
    unsafe { soltar(v as *mut c_void) };
    r
}

unsafe extern "system" fn bloquear(_: *mut c_void, _: i32) -> i32 {
    0
}

// --- La Vista: cuatro tablas seguidas (IPreviewHandler, IInitializeWithStream, IObjectWithSite, IOleWindow) ---

#[repr(C)]
struct Vista {
    vt_previa: &'static VtPrevia,
    vt_flujo: &'static VtFlujo,
    vt_sitio: &'static VtSitio,
    vt_ventana: &'static VtVentana,
    // Todo llega por el hilo de la ventana (apartamento de un hilo): sin candados.
    cuenta: Cell<u32>,
    md: bool,
    padre: Cell<HWND>,
    rect: Cell<RECT>,
    flujo: RefCell<Option<Com>>,
    sitio: RefCell<Option<Com>>,
    ventana: Cell<HWND>,
}

#[repr(C)]
struct VtPrevia {
    qi: Qi,
    mas: Cuenta,
    menos: Cuenta,
    poner_ventana: unsafe extern "system" fn(*mut c_void, HWND, *const RECT) -> i32,
    poner_rect: unsafe extern "system" fn(*mut c_void, *const RECT) -> i32,
    ensenar: unsafe extern "system" fn(*mut c_void) -> i32,
    descargar: unsafe extern "system" fn(*mut c_void) -> i32,
    dar_foco: unsafe extern "system" fn(*mut c_void) -> i32,
    foco: unsafe extern "system" fn(*mut c_void, *mut HWND) -> i32,
    atajo: unsafe extern "system" fn(*mut c_void, *const MSG) -> i32,
}

#[repr(C)]
struct VtFlujo {
    qi: Qi,
    mas: Cuenta,
    menos: Cuenta,
    iniciar: unsafe extern "system" fn(*mut c_void, *mut c_void, u32) -> i32,
}

#[repr(C)]
struct VtSitio {
    qi: Qi,
    mas: Cuenta,
    menos: Cuenta,
    poner: unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32,
    dar: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> i32,
}

#[repr(C)]
struct VtVentana {
    qi: Qi,
    mas: Cuenta,
    menos: Cuenta,
    dar: unsafe extern "system" fn(*mut c_void, *mut HWND) -> i32,
    ayuda: unsafe extern "system" fn(*mut c_void, i32) -> i32,
}

static VT_PREVIA: VtPrevia = VtPrevia { qi: qi0, mas: mas0, menos: menos0, poner_ventana, poner_rect, ensenar, descargar, dar_foco, foco, atajo };
static VT_FLUJO: VtFlujo = VtFlujo { qi: qi1, mas: mas1, menos: menos1, iniciar };
static VT_SITIO: VtSitio = VtSitio { qi: qi2, mas: mas2, menos: menos2, poner: poner_sitio, dar: dar_sitio };
static VT_VENTANA: VtVentana = VtVentana { qi: qi3, mas: mas3, menos: menos3, dar: dar_ventana, ayuda };

/// La Vista a partir del puntero a una de sus tablas (de la 0 a la 3).
fn vista<'a>(this: *mut c_void, tabla: usize) -> &'a Vista {
    unsafe { &*((this as *mut usize).sub(tabla) as *const Vista) }
}

fn qi_vista(v: &Vista, iid: *const GUID, salida: *mut *mut c_void) -> i32 {
    let iid = unsafe { &*iid };
    let base = v as *const Vista as *mut usize;
    let k = if igual(iid, &IID_IUNKNOWN) || igual(iid, &IID_VISTA) {
        0
    } else if igual(iid, &IID_FLUJO) {
        1
    } else if igual(iid, &IID_SITIO) {
        2
    } else if igual(iid, &IID_VENTANA) {
        3
    } else {
        unsafe { *salida = std::ptr::null_mut() };
        return E_NOINTERFACE;
    };
    v.cuenta.set(v.cuenta.get() + 1);
    unsafe { *salida = base.add(k) as *mut c_void };
    0
}

/// Suelta una referencia; con la última, la Vista se va y con ella el proceso.
unsafe fn soltar(this: *mut c_void) -> u32 {
    let v = vista(this, 0);
    let n = v.cuenta.get() - 1;
    v.cuenta.set(n);
    if n == 0 {
        let h = v.ventana.get();
        drop(unsafe { Box::from_raw(v as *const Vista as *mut Vista) });
        // Fuera de la ventana del Explorador antes de que la cierre (con ella se iría la nuestra).
        if !h.is_null() {
            unsafe {
                ShowWindow(h, SW_HIDE);
                SetParent(h, std::ptr::null_mut());
            }
            LIBRE.with(|l| l.set(h));
        }
        VIVA.with(|x| x.set(false));
        ofrecer();
        rearmar();
    }
    n
}

macro_rules! cuentas {
    ($qi:ident, $mas:ident, $menos:ident, $k:expr) => {
        unsafe extern "system" fn $qi(this: *mut c_void, iid: *const GUID, s: *mut *mut c_void) -> i32 {
            qi_vista(vista(this, $k), iid, s)
        }
        unsafe extern "system" fn $mas(this: *mut c_void) -> u32 {
            let v = vista(this, $k);
            v.cuenta.set(v.cuenta.get() + 1);
            v.cuenta.get()
        }
        unsafe extern "system" fn $menos(this: *mut c_void) -> u32 {
            unsafe { soltar((this as *mut usize).sub($k) as *mut c_void) }
        }
    };
}
cuentas!(qi0, mas0, menos0, 0);
cuentas!(qi1, mas1, menos1, 1);
cuentas!(qi2, mas2, menos2, 2);
cuentas!(qi3, mas3, menos3, 3);

/// Lo de dentro, con un pánico como E_FAIL: uno que cruzara la llamada de Windows acabaría con el proceso.
fn hecho(f: impl FnOnce() -> i32) -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or(E_FAIL)
}

unsafe extern "system" fn poner_ventana(this: *mut c_void, padre: HWND, r: *const RECT) -> i32 {
    hecho(|| {
        let v = vista(this, 0);
        v.padre.set(padre);
        if !r.is_null() {
            v.rect.set(unsafe { *r });
        }
        let h = v.ventana.get();
        if !h.is_null() {
            let r = v.rect.get();
            unsafe {
                SetParent(h, padre);
                MoveWindow(h, r.left, r.top, r.right - r.left, r.bottom - r.top, 1);
            }
        }
        0
    })
}

unsafe extern "system" fn poner_rect(this: *mut c_void, r: *const RECT) -> i32 {
    hecho(|| {
        let v = vista(this, 0);
        if r.is_null() {
            return E_FAIL;
        }
        v.rect.set(unsafe { *r });
        // El Explorador espera a que vuelva cada cambio de tamaño mientras se arrastra el borde: se vuelve ya y la ventana
        // se coloca después, con el último tamaño, cuando este hilo no tiene nada más que atender (un reloj llega el
        // último). Así no espera a que RichEdit vuelva a colocar el texto, y los pasos seguidos se juntan en uno. Si
        // colocarlo es lento (un .md de 1 MB, 100 ms o más), el siguiente paso esperaría a que acabe: entonces se coloca
        // solo cuando el borde se para un momento.
        let h = v.ventana.get();
        if !h.is_null() {
            TAMANO.with(|t| {
                let (_, mut reloj) = t.get();
                if reloj != 0 && LENTO.with(Cell::get) {
                    unsafe { KillTimer(std::ptr::null_mut(), reloj) };
                    reloj = 0;
                }
                if reloj == 0 {
                    let espera = if LENTO.with(Cell::get) { PARADO_MS } else { 0 };
                    reloj = unsafe { SetTimer(std::ptr::null_mut(), 0, espera, Some(poner_tamano)) };
                }
                t.set((Some((h, v.rect.get())), reloj));
            });
        }
        0
    })
}

// IPreviewHandlerFrame del Explorador: a él van las teclas que la vista no usa.
thread_local!(static MARCO: RefCell<Option<Com>> = const { RefCell::new(None) });
const IID_MARCO: GUID = g(0xFEC87AAF_35F9_447A_ADB7_20234491401A);

/// Una tecla que la vista no usa, al Explorador (Tab y F6 le devuelven el foco, F5 refresca…), como pide Windows a las
/// vistas con integridad baja. Dice si la ha atendido él.
pub fn al_explorador(m: &MSG) -> bool {
    type Atajo = unsafe extern "system" fn(*mut c_void, *const MSG) -> i32;
    // Una referencia propia: durante la llamada pueden llegar otras (otro SetSite) que cambien MARCO.
    let Some(marco) = MARCO.with(|x| x.borrow().as_ref().and_then(|f| f.pedir(&IID_MARCO))) else { return false };
    let atajo: Atajo = unsafe { std::mem::transmute(marco.metodo(4)) };
    unsafe { atajo(marco.0, m) == 0 }
}

thread_local!(static TAMANO: Cell<(Option<(HWND, RECT)>, usize)> = const { Cell::new((None, 0)) });
// Si colocar el texto tarda más que LENTO_MS (al cargarlo o al cambiar de tamaño la última vez).
thread_local!(static LENTO: Cell<bool> = const { Cell::new(false) });
const LENTO_MS: u128 = 30;
const PARADO_MS: u32 = 150;

unsafe extern "system" fn poner_tamano(_: HWND, _: u32, id: usize, _: u32) {
    unsafe { KillTimer(std::ptr::null_mut(), id) };
    let (pendiente, _) = TAMANO.with(|t| t.replace((None, 0)));
    if let Some((h, r)) = pendiente {
        unsafe {
            if IsWindow(h) != 0 {
                let t0 = std::time::Instant::now();
                MoveWindow(h, r.left, r.top, r.right - r.left, r.bottom - r.top, 1);
                LENTO.with(|x| x.set(t0.elapsed().as_millis() > LENTO_MS));
            }
        }
    }
}

// Lo que falta por poner de un texto largo y dónde va.
thread_local!(static RESTO: RefCell<(Vec<u16>, usize)> = const { RefCell::new((Vec::new(), 0)) });
const TROZO: usize = 64 << 10;

/// Un trozo más, acabado en un salto, y otro reloj si queda. Un reloj llega cuando no hay nada más: los cambios de
/// tamaño y las teclas pasan antes.
unsafe extern "system" fn poner_resto(_: HWND, _: u32, id: usize, _: u32) {
    unsafe { KillTimer(std::ptr::null_mut(), id) };
    let trozo = RESTO.with(|r| {
        let (t, desde) = &mut *r.borrow_mut();
        let fin = if t.len() - *desde > TROZO {
            t[*desde..*desde + TROZO].iter().rposition(|&c| c == 13).map_or(*desde + TROZO, |i| *desde + i + 1)
        } else {
            t.len()
        };
        let trozo = t[*desde..fin].to_vec();
        *desde = fin;
        trozo
    });
    if trozo.is_empty() {
        return;
    }
    ventana::vista_anadir(&trozo);
    if RESTO.with(|r| r.borrow().1 < r.borrow().0.len()) {
        unsafe { SetTimer(std::ptr::null_mut(), 0, 0, Some(poner_resto)) };
    } else {
        parar_resto();
    }
}

/// Otro archivo o ninguno: lo que faltaba del anterior ya no se pone (el reloj que quede no encuentra nada).
fn parar_resto() {
    RESTO.with(|r| *r.borrow_mut() = (Vec::new(), 0));
}

unsafe extern "system" fn ensenar(this: *mut c_void) -> i32 {
    hecho(|| {
        let v = vista(this, 0);
        // El flujo se suelta en cuanto se ha leído: el archivo se puede mover o borrar mientras se ve.
        let Some(flujo) = v.flujo.borrow_mut().take() else { return E_FAIL };
        parar_resto();
        let (bytes, nombre) = leer(&flujo);
        drop(flujo);
        let r = v.rect.get();
        match v.ventana.get() {
            h if h.is_null() => v.ventana.set(ventana::vista(v.padre.get(), &r)),
            h => unsafe {
                SetParent(h, v.padre.get());
                MoveWindow(h, r.left, r.top, r.right - r.left, r.bottom - r.top, 0);
                ventana::vista_tema();
            },
        }
        // Sin el panel del Explorador (cerrado mientras se leía), nada: la ventana quedaría suelta en el escritorio.
        let h = v.ventana.get();
        if h.is_null() || unsafe { GetParent(h) } != v.padre.get() {
            return E_FAIL;
        }
        // Por la extensión del archivo, no por la clave bajo la que está registrada la vista previa (una clave de un
        // programa elegido para .md y .json a la vez es una sola); sin nombre, por la clave.
        let archivo = nombre.rsplit(['\\', '/']).next().unwrap_or_default();
        let md = match archivo.rsplit_once('.') {
            Some((_, e)) => e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"),
            None => v.md,
        };
        let t0 = std::time::Instant::now();
        let mut l = fichero::decodificar(&bytes);
        if binario(&l) {
            l = fichero::decodificar(t().no_es_texto.as_bytes());
            ventana::vista_cargar(l, false);
        } else if md {
            if l.texto.len() > TOPE_MD {
                let corte = l.texto[..TOPE_MD].iter().rposition(|&c| c == 13).unwrap_or(TOPE_MD);
                l.texto.truncate(corte);
            }
            ventana::vista_cargar(l, true);
        } else {
            // Poner 1 MB en RichEdit cuesta 140 ms, y el Explorador espera a que vuelva: el principio ahora y el resto a
            // trozos cuando el hilo está libre.
            let resto = if l.texto.len() > TROZO {
                let corte = l.texto[..TROZO].iter().rposition(|&c| c == 13).map_or(TROZO, |i| i + 1);
                l.texto.split_off(corte)
            } else {
                Vec::new()
            };
            ventana::vista_cargar(l, false);
            if !resto.is_empty() {
                RESTO.with(|r| *r.borrow_mut() = (resto, 0));
                unsafe { SetTimer(std::ptr::null_mut(), 0, 0, Some(poner_resto)) };
            }
        }
        // Con un resto por poner, el texto entero será largo: también lento de colocar al cambiar de tamaño.
        let grande = RESTO.with(|r| !r.borrow().0.is_empty());
        LENTO.with(|x| x.set(grande || t0.elapsed().as_millis() > LENTO_MS));
        // Después de cargar: antes se vería un momento el archivo anterior con el nombre del nuevo.
        unsafe { ShowWindow(h, SW_SHOWNA) };
        // Sonda de L5 (1.3), una vez por proceso y después de volver al Explorador: se quita al hacer la edición.
        if SONDA.with(|x| x.borrow().is_none()) {
            let sitio = v.sitio.borrow().as_ref().and_then(|s| s.pedir(&IID_IUNKNOWN));
            SONDA.with(|x| *x.borrow_mut() = Some((sitio, nombre.clone())));
            unsafe { SetTimer(std::ptr::null_mut(), 0, 500, Some(sondear)) };
        }
        if !OLE.with(|o| o.replace(true)) {
            unsafe { SetTimer(std::ptr::null_mut(), 0, 1, Some(con_ole)) };
        }
        0
    })
}

unsafe extern "system" fn descargar(this: *mut c_void) -> i32 {
    hecho(|| {
        let v = vista(this, 0);
        v.flujo.borrow_mut().take();
        parar_resto();
        let h = v.ventana.get();
        if !h.is_null() {
            ventana::vista_parar();
            unsafe { ShowWindow(h, SW_HIDE) };
        }
        0
    })
}

unsafe extern "system" fn dar_foco(this: *mut c_void) -> i32 {
    hecho(|| {
        let h = vista(this, 0).ventana.get();
        if !h.is_null() {
            unsafe { SetFocus(h) };
        }
        0
    })
}

unsafe extern "system" fn foco(_: *mut c_void, h: *mut HWND) -> i32 {
    hecho(|| {
        let f = unsafe { GetFocus() };
        unsafe { *h = f };
        if f.is_null() { S_FALSE } else { 0 }
    })
}

unsafe extern "system" fn atajo(_: *mut c_void, _: *const MSG) -> i32 {
    S_FALSE
}

unsafe extern "system" fn iniciar(this: *mut c_void, flujo: *mut c_void, _: u32) -> i32 {
    hecho(|| {
        let v = vista(this, 1);
        if flujo.is_null() {
            return E_FAIL;
        }
        let f = std::mem::ManuallyDrop::new(Com(flujo));
        // Una referencia propia: la de quien llama es suya.
        let Some(propio) = f.pedir(&IID_IUNKNOWN).and_then(|u| u.pedir(&g(0x0000000C_0000_0000_C000_000000000046))) else { return E_FAIL };
        *v.flujo.borrow_mut() = Some(propio);
        0
    })
}

unsafe extern "system" fn poner_sitio(this: *mut c_void, sitio: *mut c_void) -> i32 {
    hecho(|| {
        let v = vista(this, 2);
        let s = std::mem::ManuallyDrop::new(Com(sitio));
        *v.sitio.borrow_mut() = if sitio.is_null() { None } else { s.pedir(&IID_IUNKNOWN) };
        MARCO.with(|m| *m.borrow_mut() = if sitio.is_null() { None } else { s.pedir(&IID_MARCO) });
        0
    })
}

unsafe extern "system" fn dar_sitio(this: *mut c_void, iid: *const GUID, salida: *mut *mut c_void) -> i32 {
    hecho(|| {
        unsafe { *salida = std::ptr::null_mut() };
        let v = vista(this, 2);
        let s = v.sitio.borrow();
        let Some(s) = s.as_ref() else { return E_FAIL };
        match s.pedir(unsafe { &*iid }) {
            Some(x) => {
                unsafe { *salida = x.0 };
                std::mem::forget(x);
                0
            }
            None => E_NOINTERFACE,
        }
    })
}

unsafe extern "system" fn dar_ventana(this: *mut c_void, h: *mut HWND) -> i32 {
    hecho(|| {
        let v = vista(this, 3).ventana.get();
        unsafe { *h = v };
        if v.is_null() { E_FAIL } else { 0 }
    })
}

unsafe extern "system" fn ayuda(_: *mut c_void, _: i32) -> i32 {
    E_NOTIMPL
}

// --- Sonda de L5 (1.3): para editar en la vista previa hace falta la ruta del archivo, y el flujo solo da el nombre.
// Se apunta, una vez por proceso, el nombre que da el flujo y si la ruta sale del Explorador por el sitio (su vista de
// carpeta y lo seleccionado). Solo pregunta; no cambia nada. ---

thread_local!(static SONDA: RefCell<Option<(Option<Com>, String)>> = const { RefCell::new(None) });

unsafe extern "system" fn sondear(_: HWND, _: u32, id: usize, _: u32) {
    unsafe { KillTimer(std::ptr::null_mut(), id) };
    let Some((sitio, nombre)) = SONDA.with(|x| x.borrow_mut().as_mut().map(|(s, n)| (s.take(), n.clone()))) else { return };
    // Sin el nombre ni la ruta: solo si se sacan y si son del mismo archivo.
    let ultimo = |s: &str| s.rsplit(['\\', '/']).next().unwrap_or_default().to_lowercase();
    let por_sitio = match sitio.as_ref().map_or(Err("sin sitio".to_string()), ruta_por_sitio) {
        Ok(r) if ultimo(&r) == ultimo(&nombre) => "la ruta del mismo archivo".to_string(),
        Ok(_) => "una ruta de otro archivo".to_string(),
        Err(e) => e,
    };
    let flujo = if nombre.is_empty() {
        "no da nombre"
    } else if nombre.contains(['\\', '/']) {
        "da la ruta entera"
    } else {
        "da solo el nombre"
    };
    crate::w::apuntar(&format!("Vista previa (prueba para editar en ella): el flujo {flujo}; por el Explorador, {por_sitio}"));
}

fn ruta_por_sitio(sitio: &Com) -> Result<String, String> {
    type Servicio = unsafe extern "system" fn(*mut c_void, *const GUID, *const GUID, *mut *mut c_void) -> i32;
    type Seleccion = unsafe extern "system" fn(*mut c_void, i32, *mut *mut c_void) -> i32;
    type Elemento = unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> i32;
    type Nombre = unsafe extern "system" fn(*mut c_void, u32, *mut *mut u16) -> i32;
    let servicios = sitio.pedir(&g(0x6D5140C1_7436_11CE_8034_00AA006009FA)).ok_or("sin IServiceProvider")?;
    let pedir: Servicio = unsafe { std::mem::transmute(servicios.metodo(3)) };
    let mut p = std::ptr::null_mut();
    // SID_STopLevelBrowser, IShellBrowser
    let r = unsafe { pedir(servicios.0, &g(0x4C96BE40_915C_11CF_99D3_00AA004AE837), &g(0x000214E2_0000_0000_C000_000000000046), &mut p) };
    let navegador = Com::de(r, p).ok_or(format!("sin IShellBrowser ({r:#x})"))?;
    let vista = navegador.dar(15).ok_or("sin la vista de carpeta")?; // QueryActiveShellView
    let carpeta = vista.pedir(&g(0x1AF3A467_214F_4298_908E_06B03E0B39F9)).ok_or("sin IFolderView2")?;
    let seleccion: Seleccion = unsafe { std::mem::transmute(carpeta.metodo(32)) };
    let mut p = std::ptr::null_mut();
    let r = unsafe { seleccion(carpeta.0, 0, &mut p) };
    let lista = Com::de(r, p).ok_or(format!("sin selección ({r:#x})"))?;
    let elemento: Elemento = unsafe { std::mem::transmute(lista.metodo(8)) };
    let mut p = std::ptr::null_mut();
    let r = unsafe { elemento(lista.0, 0, &mut p) };
    let item = Com::de(r, p).ok_or(format!("sin el primero ({r:#x})"))?;
    let nombre: Nombre = unsafe { std::mem::transmute(item.metodo(5)) };
    let mut s = std::ptr::null_mut();
    let r = unsafe { nombre(item.0, 0x8005_8000, &mut s) }; // SIGDN_FILESYSPATH
    if r < 0 || s.is_null() {
        return Err(format!("sin ruta de disco ({r:#x})"));
    }
    let n = (0..).take_while(|&i| unsafe { *s.add(i) } != 0).count();
    let ruta = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(s, n) });
    unsafe { CoTaskMemFree(s as *const c_void) };
    Ok(ruta)
}

/// Lo que da el flujo, hasta TOPE y cortado en un salto de línea si no cabe entero, y el nombre del archivo.
fn leer(flujo: &Com) -> (Vec<u8>, String) {
    type Leer = unsafe extern "system" fn(*mut c_void, *mut c_void, u32, *mut u32) -> i32;
    type Datos = unsafe extern "system" fn(*mut c_void, *mut [u64; 10], u32) -> i32;
    let leer: Leer = unsafe { std::mem::transmute(flujo.metodo(3)) };
    let datos: Datos = unsafe { std::mem::transmute(flujo.metodo(12)) };
    let mut b = Vec::new();
    let mut trozo = vec![0u8; 256 << 10];
    let mut entero = false;
    while b.len() < TOPE {
        let mut n = 0u32;
        let r = unsafe { leer(flujo.0, trozo.as_mut_ptr() as *mut c_void, trozo.len() as u32, &mut n) };
        b.extend_from_slice(&trozo[..(n as usize).min(trozo.len())]);
        if r != 0 || n == 0 {
            entero = r >= 0;
            break;
        }
    }
    if !entero || b.len() >= TOPE {
        b.truncate(TOPE);
        // Hasta el último salto: ni media letra ni media línea. En UTF-16 (con marca o sin ella) el salto es 0A 00, en
        // posición par. Si el último cae lejos (una línea enorme), se corta donde cae, en par si es UTF-16.
        let par = b.len() & !1;
        let utf16 = b.starts_with(&[0xFF, 0xFE]) || crate::fichero::parece_utf16(&b[..par]) == Some(false);
        let salto = if utf16 {
            (0..par).step_by(2).rev().find(|&k| b[k] == b'\n' && b[k + 1] == 0).map(|k| k + 2)
        } else {
            b.iter().rposition(|&c| c == b'\n').map(|k| k + 1)
        };
        let fin = salto.filter(|&f| f + (64 << 10) >= b.len()).unwrap_or(if utf16 { par } else { b.len() });
        b.truncate(fin);
    }
    // STATSTG (80 bytes, alineado a 8): el nombre va al principio, en memoria de COM.
    let mut st = [0u64; 10];
    let nombre = if unsafe { datos(flujo.0, &mut st, 0) } >= 0 {
        let p = st[0] as usize as *mut u16;
        if p.is_null() {
            String::new()
        } else {
            let n = (0..).take_while(|&i| unsafe { *p.add(i) } != 0).count();
            let s = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, n) });
            unsafe { CoTaskMemFree(p as *const c_void) };
            s
        }
    } else {
        String::new()
    };
    (b, nombre)
}

/// Nulos (sin ser UTF-16) o más de un 1 % de caracteres de control: no es texto.
fn binario(l: &fichero::Leido) -> bool {
    let control = l.texto.iter().filter(|&&c| c < 0x20 && !matches!(c, 9 | 10 | 12 | 13)).count();
    l.nulos || control * 100 > l.texto.len().max(1)
}
