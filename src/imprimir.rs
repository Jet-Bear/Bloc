// Imprimir. Con la ventana de imprimir de Windows (Windows.Graphics.Printing), que enseña las páginas de Bloc en su
// vista previa: cada página se compone en el hilo de la ventana en un metarchivo (EMF), con EM_FORMATRANGE y el pie
// «Página n», y Direct2D la pinta en la vista previa y en lo que se imprime. Nada de eso se carga hasta imprimir. Si
// no se puede (un Windows sin ella o un escritorio de pruebas), el diálogo clásico, que imprime directo como siempre.
// Márgenes de 2 cm; siempre en claro y sin marcas de Markdown a la vista. Diseño en privado\DISENO-K5.md.
use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{CloseHandle, GlobalFree, HANDLE, HWND, LPARAM, RECT};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::Storage::Xps::{EndDoc, EndPage, StartDocW, StartPage, DOCINFOW};
use windows_sys::Win32::System::Com::StructuredStorage::{CreateStreamOnHGlobal, GetHGlobalFromStream};
use windows_sys::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows_sys::Win32::System::Threading::{CreateEventW, GetCurrentThreadId, SetEvent, WaitForSingleObject};
use windows_sys::Win32::UI::Controls::Dialogs::{PrintDlgW, PD_NOPAGENUMS, PD_NOSELECTION, PD_RETURNDC, PD_USEDEVMODECOPIESANDCOLLATE, PRINTDLGW};
use windows_sys::Win32::UI::WindowsAndMessaging::{KillTimer, PostMessageW, SetTimer, WM_APP, WM_SETREDRAW};

use crate::com::Com;
use crate::editor;
use crate::re::{CHARRANGE, EM_FORMATRANGE, FORMATRANGE};
use crate::textos::t;
use crate::{md, tabla, tema};
use crate::ventana::{self, app};
use crate::w::{con, enviar, funcion, ws};

pub const WM_PAGINAR: u32 = WM_APP + 4;
/// La impresión ha fallado: la ventana lo avisa.
pub const WM_NO_IMPRESO: u32 = WM_APP + 5;
pub const RELOJ: usize = 4;

const fn g(u: u128) -> GUID {
    GUID::from_u128(u)
}
const IID_IUNKNOWN: GUID = g(0x00000000_0000_0000_C000_000000000046);
const IID_IINSPECTABLE: GUID = g(0xAF86E2E0_B12D_4C6A_9C5A_D7AA65101E90);
const IID_IAGILE: GUID = g(0x94EA2B94_E9CC_49E0_C0FF_EE64CA8F5B90);
const IID_INTEROP: GUID = g(0xC5435A42_8D43_4E7B_A68A_EF311E392087);
const IID_ASINCRONA_BOOL: GUID = g(0xCDB5EFB3_5788_509D_9BE1_71CCB8A3362A);
const IID_IPRINT_MANAGER: GUID = g(0xFF2A9694_8C99_44FD_AE4A_19D9AA9A0F0A);
const IID_PEDIDO: GUID = g(0x8A8CB877_70C5_54CE_8B42_D790E2914859);
const IID_FUENTE_PEDIDA: GUID = g(0x6C109FA8_5CB6_4B3A_8663_F39CB02DC9B4);
const IID_ACABADO: GUID = g(0xB0B02549_B9AD_5226_898A_7B563B46640C);
const IID_DOCUMENTO: GUID = g(0xDEDC0C30_F1EB_47DF_AAE6_ED5427511F01);
const IID_PAGINAS: GUID = g(0xA96BB1DB_172E_4667_82B5_AD97A252318F);
const IID_COLECCION: GUID = g(0x0B31CC62_D7EC_4747_9D6E_F2537D870F2B);
const IID_OPCIONES: GUID = g(0x1BDBB474_4ED1_41EB_BE3C_72D18ED67337);
const IID_OPCIONES2: GUID = g(0xEB9B1606_9A36_4B59_8617_B217849262E1);
const IID_OPCIONES_VISIBLES: GUID = g(0x62E69E23_9A1E_4336_B74F_3CC7F4CFF709);
const IID_ESTANDAR3: GUID = g(0xBBF68E86_3858_41B3_A799_55DD9888D475);
const IID_PREVIA_DXGI: GUID = g(0x1A6DD0AD_1E2A_4E99_A5BA_91F17818290E);
const IID_DXGI_DEVICE: GUID = g(0x54EC77FA_1377_44E6_8C32_88FD5F44C84C);
const IID_DXGI_SURFACE: GUID = g(0xCAFCB56C_6AC3_4889_BF47_9E23BBD260EC);
const IID_D2D_FACTORY1: GUID = g(0xBB12D362_DAEE_4B9A_AA1D_14BA401CFA1F);
const IID_DWRITE: GUID = g(0xB859EE5A_D838_4B5B_A2E8_1ADC7D93DB48);
const CLSID_WIC: GUID = g(0xCACAF262_9370_4615_A13B_9F5539DA4C0A);
const IID_WIC: GUID = g(0xEC5EC8A9_C395_4314_9C77_54D7A935FF70);
const CLSID_FABRICA_TRABAJOS: GUID = g(0x348EF17D_6C81_4982_92B4_EE188A43867A);
const IID_FABRICA_TRABAJOS: GUID = g(0xD2959BF7_B31B_4A3D_9600_712EB1335BA4);

const E_FAIL: i32 = 0x80004005_u32 as i32;
const E_NOINTERFACE: i32 = 0x80004002_u32 as i32;
const E_NOTIMPL: i32 = 0x80004001_u32 as i32;
/// Los ppp de la vista previa: nítida en una pantalla al 150 %.
const PPP_PREVIA: f32 = 144.0;

/// Una función de la tabla de una interfaz, con su forma.
macro_rules! f {
    ($c:expr, $n:expr, fn($($t:ty),*) -> $r:ty) => {{
        #[allow(unused_unsafe)]
        let f: unsafe extern "system" fn(*mut c_void $(, $t)*) -> $r = unsafe { std::mem::transmute($c.metodo($n)) };
        f
    }};
}

#[path = "hoja.rs"]
mod hoja;

pub fn imprimir() {
    let a = app();
    VENTANA.store(a.hwnd.get() as isize, Ordering::SeqCst);
    HILO_VENTANA.store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);
    *TITULO.lock().unwrap_or_else(|e| e.into_inner()) = ws(&ventana::titulo_documento());
    if let Some(dir) = std::env::var_os("BLOC_IMPRIMIR") {
        std::thread::spawn(move || prueba(std::path::PathBuf::from(dir)));
        return;
    }
    if !(en_escritorio_de_verdad() && moderno()) {
        clasico();
    }
}

// --- La ventana de imprimir de Windows ---

static VENTANA: AtomicIsize = AtomicIsize::new(0);
static HILO_VENTANA: AtomicU32 = AtomicU32::new(0);
static TITULO: Mutex<Vec<u16>> = Mutex::new(Vec::new());
/// El IPrintManager de la ventana, con su aviso apuntado: una vez para siempre (dos avisos serían dos trabajos).
static GESTOR: AtomicIsize = AtomicIsize::new(0);

/// La ventana de Windows es otro programa: desde un escritorio de pruebas saldría en el de quien usa el PC.
fn en_escritorio_de_verdad() -> bool {
    type Escritorio = unsafe extern "system" fn(u32) -> HANDLE;
    type Nombre = unsafe extern "system" fn(HANDLE, i32, *mut c_void, u32, *mut u32) -> i32;
    let (Some(escritorio), Some(nombre)) = (funcion("user32.dll", "GetThreadDesktop"), funcion("user32.dll", "GetUserObjectInformationW")) else { return false };
    let (escritorio, nombre): (Escritorio, Nombre) = unsafe { (std::mem::transmute::<usize, Escritorio>(escritorio), std::mem::transmute::<usize, Nombre>(nombre)) };
    let mut b = [0u16; 64];
    let mut n = 0;
    let h = unsafe { escritorio(GetCurrentThreadId()) };
    let leido = unsafe { nombre(h, 2, b.as_mut_ptr() as *mut c_void, 128, &mut n) } != 0;
    leido && String::from_utf16_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(0)]).eq_ignore_ascii_case("Default")
}

struct Cadena(*mut c_void);

impl Cadena {
    fn nueva(s: &[u16]) -> Option<Cadena> {
        let crear: unsafe extern "system" fn(*const u16, u32, *mut *mut c_void) -> i32 = unsafe { std::mem::transmute(funcion("combase.dll", "WindowsCreateString")?) };
        let n = s.iter().position(|&c| c == 0).unwrap_or(s.len());
        let mut h = std::ptr::null_mut();
        (unsafe { crear(s.as_ptr(), n as u32, &mut h) } >= 0).then_some(Cadena(h))
    }
}

impl Drop for Cadena {
    fn drop(&mut self) {
        if let Some(borrar) = funcion("combase.dll", "WindowsDeleteString") {
            let borrar: unsafe extern "system" fn(*mut c_void) -> i32 = unsafe { std::mem::transmute(borrar) };
            unsafe { borrar(self.0) };
        }
    }
}

/// La fábrica de una clase de Windows (WinRT), con la interfaz `iid`.
fn fabrica(clase: &str, iid: &GUID) -> Option<Com> {
    let crear: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> i32 = unsafe { std::mem::transmute(funcion("combase.dll", "RoGetActivationFactory")?) };
    let clase = Cadena::nueva(&ws(clase))?;
    let mut p = std::ptr::null_mut();
    Com::de(unsafe { crear(clase.0, iid, &mut p) }, p)
}

fn moderno() -> bool {
    (|| -> Option<()> {
        let interop = fabrica("Windows.Graphics.Printing.PrintManager", &IID_INTEROP)?;
        let h = VENTANA.load(Ordering::SeqCst) as HWND;
        if GESTOR.load(Ordering::SeqCst) == 0 {
            let mut p = std::ptr::null_mut();
            let gestor = Com::de(unsafe { f!(interop, 6, fn(HWND, *const GUID, *mut *mut c_void) -> i32)(interop.0, h, &IID_IPRINT_MANAGER, &mut p) }, p)?;
            let mut token = 0i64;
            if unsafe { f!(gestor, 6, fn(*const Delegado, *mut i64) -> i32)(gestor.0, &PEDIDO, &mut token) } < 0 {
                return None;
            }
            GESTOR.store(gestor.0 as isize, Ordering::SeqCst);
            std::mem::forget(gestor);
        }
        let mut p = std::ptr::null_mut();
        Com::de(unsafe { f!(interop, 7, fn(HWND, *const GUID, *mut *mut c_void) -> i32)(interop.0, h, &IID_ASINCRONA_BOOL, &mut p) }, p).map(drop)
    })()
    .is_some()
}

type Qi = unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> i32;
type Cuenta = unsafe extern "system" fn(*mut c_void) -> u32;

#[repr(C)]
struct VtAviso1 {
    qi: Qi,
    mas: Cuenta,
    menos: Cuenta,
    invocar: unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32,
}

#[repr(C)]
struct VtAviso2 {
    qi: Qi,
    mas: Cuenta,
    menos: Cuenta,
    invocar: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void) -> i32,
}

/// Un aviso de Windows (delegado): estático, sin cuenta de referencias, y de los que valen desde cualquier hilo.
#[repr(C)]
struct Delegado {
    vt: *const c_void,
    iid: GUID,
}

unsafe impl Sync for Delegado {}

static VT_PEDIDO: VtAviso2 = VtAviso2 { qi: qi_delegado, mas: uno, menos: uno, invocar: pedido };
static VT_FUENTE_PEDIDA: VtAviso1 = VtAviso1 { qi: qi_delegado, mas: uno, menos: uno, invocar: fuente_pedida };
static VT_ACABADO: VtAviso2 = VtAviso2 { qi: qi_delegado, mas: uno, menos: uno, invocar: acabado };
static PEDIDO: Delegado = Delegado { vt: &VT_PEDIDO as *const VtAviso2 as *const c_void, iid: IID_PEDIDO };
static FUENTE_PEDIDA: Delegado = Delegado { vt: &VT_FUENTE_PEDIDA as *const VtAviso1 as *const c_void, iid: IID_FUENTE_PEDIDA };
static ACABADO: Delegado = Delegado { vt: &VT_ACABADO as *const VtAviso2 as *const c_void, iid: IID_ACABADO };

fn igual(a: &GUID, b: &GUID) -> bool {
    a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
}

unsafe extern "system" fn qi_delegado(this: *mut c_void, iid: *const GUID, salida: *mut *mut c_void) -> i32 {
    let d = unsafe { &*(this as *const Delegado) };
    let iid = unsafe { &*iid };
    if [IID_IUNKNOWN, IID_IAGILE, d.iid].iter().any(|x| igual(x, iid)) {
        unsafe { *salida = this };
        0
    } else {
        unsafe { *salida = std::ptr::null_mut() };
        E_NOINTERFACE
    }
}

unsafe extern "system" fn uno(_: *mut c_void) -> u32 {
    1
}

/// Se ha pedido imprimir: el trabajo, con el nombre del documento.
unsafe extern "system" fn pedido(_: *mut c_void, _: *mut c_void, args: *mut c_void) -> i32 {
    let r = std::panic::catch_unwind(|| -> Option<()> {
        let args = std::mem::ManuallyDrop::new(Com(args));
        let peticion = args.dar(6)?;
        let titulo = Cadena::nueva(&TITULO.lock().ok()?.clone())?;
        let mut p = std::ptr::null_mut();
        let tarea = Com::de(unsafe { f!(peticion, 7, fn(*mut c_void, *const Delegado, *mut *mut c_void) -> i32)(peticion.0, titulo.0, &FUENTE_PEDIDA, &mut p) }, p)?;
        let mut token = 0i64;
        unsafe { f!(tarea, 15, fn(*const Delegado, *mut i64) -> i32)(tarea.0, &ACABADO, &mut token) };
        // Sin él la ventana imprime igual, todas.
        con_intervalo(&tarea);
        Some(())
    });
    if matches!(r, Ok(Some(()))) { 0 } else { E_FAIL }
}

/// En la ventana, «Páginas»: todas o las que se escriban (1-3, 5).
fn con_intervalo(tarea: &Com) -> Option<()> {
    let opciones = tarea.dar(8)?;
    let paginas = opciones.pedir(&IID_OPCIONES2)?.dar(6)?;
    unsafe {
        f!(paginas, 6, fn(bool) -> i32)(paginas.0, true);
        f!(paginas, 10, fn(bool) -> i32)(paginas.0, true);
    }
    let visibles = opciones.pedir(&IID_OPCIONES_VISIBLES)?.dar(6)?;
    let estandar = fabrica("Windows.Graphics.Printing.StandardPrintTaskOptions", &IID_ESTANDAR3)?;
    let mut h = std::ptr::null_mut();
    if unsafe { f!(estandar, 6, fn(*mut *mut c_void) -> i32)(estandar.0, &mut h) } < 0 {
        return None;
    }
    let nombre = Cadena(h);
    let (mut donde, mut esta) = (0u32, false);
    unsafe {
        f!(visibles, 9, fn(*mut c_void, *mut u32, *mut bool) -> i32)(visibles.0, nombre.0, &mut donde, &mut esta);
        (esta || f!(visibles, 13, fn(*mut c_void) -> i32)(visibles.0, nombre.0) >= 0).then_some(())
    }
}

/// Las páginas que se han pedido en la ventana, en su orden; sin intervalo, todas.
fn elegir(opciones: *mut c_void, paginas: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let rangos = (|| {
        let o = std::mem::ManuallyDrop::new(Com(opciones));
        let lista = o.pedir(&IID_OPCIONES2)?.dar(7)?;
        let mut n = 0u32;
        unsafe { f!(lista, 7, fn(*mut u32) -> i32)(lista.0, &mut n) };
        (0..n)
            .map(|i| {
                let mut p = std::ptr::null_mut();
                let rango = Com::de(unsafe { f!(lista, 6, fn(u32, *mut *mut c_void) -> i32)(lista.0, i, &mut p) }, p)?;
                let (mut de, mut a) = (0, 0);
                unsafe {
                    f!(rango, 6, fn(*mut i32) -> i32)(rango.0, &mut de);
                    f!(rango, 7, fn(*mut i32) -> i32)(rango.0, &mut a);
                }
                Some((de, a))
            })
            .collect::<Option<Vec<_>>>()
    })()
    .unwrap_or_default();
    del_intervalo(paginas.len(), &rangos).into_iter().map(|k| paginas[k].clone()).collect()
}

/// Los índices (desde 0) de las páginas de los intervalos (desde 1, ambos incluidos), de las que hay.
fn del_intervalo(hay: usize, rangos: &[(i32, i32)]) -> Vec<usize> {
    if rangos.is_empty() {
        return (0..hay).collect();
    }
    rangos.iter().flat_map(|&(de, a)| de.max(1)..=a.min(hay as i32)).map(|n| n as usize - 1).collect()
}

/// Windows pide las páginas: se le da una Fuente.
unsafe extern "system" fn fuente_pedida(_: *mut c_void, args: *mut c_void) -> i32 {
    let r = std::panic::catch_unwind(|| {
        let args = std::mem::ManuallyDrop::new(Com(args));
        let fuente = Fuente::nueva();
        let r = unsafe { f!(args, 7, fn(*mut c_void) -> i32)(args.0, fuente as *mut c_void) };
        unsafe { liberar(fuente as *mut c_void) };
        r
    });
    r.unwrap_or(E_FAIL)
}

/// Impreso, cancelado o fallido: la Fuente la suelta Windows, y con ella todo lo de Direct2D. Si ha fallado, se avisa.
unsafe extern "system" fn acabado(_: *mut c_void, _: *mut c_void, args: *mut c_void) -> i32 {
    let _ = std::panic::catch_unwind(|| {
        let args = std::mem::ManuallyDrop::new(Com(args));
        let mut como = 0;
        if unsafe { f!(args, 6, fn(*mut i32) -> i32)(args.0, &mut como) } >= 0 && como == 2 {
            unsafe { PostMessageW(VENTANA.load(Ordering::SeqCst) as HWND, WM_NO_IMPRESO, 0, 0) };
        }
    });
    0
}

// --- La Fuente: las páginas para la vista previa y para imprimir ---

/// Una sola cosa con tres tablas: IPrintDocumentSource, IPrintDocumentPageSource e IPrintPreviewPageCollection.
#[repr(C)]
struct Fuente {
    vt_documento: &'static VtDocumento,
    vt_paginas: &'static VtPaginas,
    vt_coleccion: &'static VtColeccion,
    cuenta: AtomicU32,
    estado: Mutex<Estado>,
}

#[repr(C)]
struct VtDocumento {
    qi: Qi,
    mas: Cuenta,
    menos: Cuenta,
    iids: unsafe extern "system" fn(*mut c_void, *mut u32, *mut *mut GUID) -> i32,
    nombre: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32,
    confianza: unsafe extern "system" fn(*mut c_void, *mut i32) -> i32,
}

#[repr(C)]
struct VtPaginas {
    qi: Qi,
    mas: Cuenta,
    menos: Cuenta,
    coleccion: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut *mut c_void) -> i32,
    documento: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void) -> i32,
}

#[repr(C)]
struct VtColeccion {
    qi: Qi,
    mas: Cuenta,
    menos: Cuenta,
    paginar: unsafe extern "system" fn(*mut c_void, u32, *mut c_void) -> i32,
    pagina: unsafe extern "system" fn(*mut c_void, u32, f32, f32) -> i32,
}

#[derive(Default)]
struct Estado {
    papel: (f32, f32),
    paginas: Option<Arc<Vec<Vec<u8>>>>,
    previa: Option<Com>,
    graficos: Option<Graficos>,
}

struct Graficos {
    d3d: Com,
    fabrica: Com,
    dispositivo: Com,
    contexto: Com,
    /// IDWriteGdiInterop, y las letras de los metarchivos ya hechas de DirectWrite.
    gdi: Com,
    letras: RefCell<Vec<hoja::Letra>>,
}

static TABLA_DOCUMENTO: VtDocumento = VtDocumento { qi: qi_fuente0, mas: mas0, menos: liberar0, iids: sin_iids, nombre: sin_nombre, confianza };
static TABLA_PAGINAS: VtPaginas = VtPaginas { qi: qi_fuente1, mas: mas1, menos: liberar1, coleccion, documento };
static TABLA_COLECCION: VtColeccion = VtColeccion { qi: qi_fuente2, mas: mas2, menos: liberar2, paginar, pagina };

impl Fuente {
    fn nueva() -> *mut Fuente {
        Box::into_raw(Box::new(Fuente {
            vt_documento: &TABLA_DOCUMENTO,
            vt_paginas: &TABLA_PAGINAS,
            vt_coleccion: &TABLA_COLECCION,
            cuenta: AtomicU32::new(1),
            estado: Mutex::new(Estado::default()),
        }))
    }
}

/// La Fuente a partir del puntero a una de sus tablas (la 0, 1 o 2).
fn fuente<'a>(this: *mut c_void, tabla: usize) -> &'a Fuente {
    unsafe { &*((this as *mut usize).sub(tabla) as *const Fuente) }
}

fn qi_fuente(f: &Fuente, iid: *const GUID, salida: *mut *mut c_void) -> i32 {
    let iid = unsafe { &*iid };
    let base = f as *const Fuente as *mut usize;
    let p = if [IID_IUNKNOWN, IID_IINSPECTABLE, IID_DOCUMENTO, IID_IAGILE].iter().any(|x| igual(x, iid)) {
        base
    } else if igual(iid, &IID_PAGINAS) {
        unsafe { base.add(1) }
    } else if igual(iid, &IID_COLECCION) {
        unsafe { base.add(2) }
    } else {
        unsafe { *salida = std::ptr::null_mut() };
        return E_NOINTERFACE;
    };
    f.cuenta.fetch_add(1, Ordering::SeqCst);
    unsafe { *salida = p as *mut c_void };
    0
}

unsafe fn liberar(this: *mut c_void) -> u32 {
    let f = fuente(this, 0);
    let n = f.cuenta.fetch_sub(1, Ordering::SeqCst) - 1;
    if n == 0 {
        drop(unsafe { Box::from_raw(f as *const Fuente as *mut Fuente) });
    }
    n
}

unsafe extern "system" fn qi_fuente0(this: *mut c_void, iid: *const GUID, s: *mut *mut c_void) -> i32 {
    qi_fuente(fuente(this, 0), iid, s)
}
unsafe extern "system" fn qi_fuente1(this: *mut c_void, iid: *const GUID, s: *mut *mut c_void) -> i32 {
    qi_fuente(fuente(this, 1), iid, s)
}
unsafe extern "system" fn qi_fuente2(this: *mut c_void, iid: *const GUID, s: *mut *mut c_void) -> i32 {
    qi_fuente(fuente(this, 2), iid, s)
}
unsafe extern "system" fn mas0(this: *mut c_void) -> u32 {
    fuente(this, 0).cuenta.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "system" fn mas1(this: *mut c_void) -> u32 {
    fuente(this, 1).cuenta.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "system" fn mas2(this: *mut c_void) -> u32 {
    fuente(this, 2).cuenta.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "system" fn liberar0(this: *mut c_void) -> u32 {
    unsafe { liberar(this) }
}
unsafe extern "system" fn liberar1(this: *mut c_void) -> u32 {
    unsafe { liberar((this as *mut usize).sub(1) as *mut c_void) }
}
unsafe extern "system" fn liberar2(this: *mut c_void) -> u32 {
    unsafe { liberar((this as *mut usize).sub(2) as *mut c_void) }
}
unsafe extern "system" fn sin_iids(_: *mut c_void, n: *mut u32, iids: *mut *mut GUID) -> i32 {
    unsafe {
        *n = 0;
        *iids = std::ptr::null_mut();
    }
    0
}
unsafe extern "system" fn sin_nombre(_: *mut c_void, _: *mut *mut c_void) -> i32 {
    E_NOTIMPL
}
unsafe extern "system" fn confianza(_: *mut c_void, nivel: *mut i32) -> i32 {
    unsafe { *nivel = 0 };
    0
}

/// Lo que se haga dentro, con los fallos (también un pánico) como E_FAIL para Windows.
fn hecho(f: impl FnOnce() -> Option<()>) -> i32 {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(Some(())) => 0,
        _ => E_FAIL,
    }
}

/// IPrintDocumentPageSource::GetPreviewPageCollection: el destino de la vista previa, y la colección somos nosotros.
unsafe extern "system" fn coleccion(this: *mut c_void, destino: *mut c_void, salida: *mut *mut c_void) -> i32 {
    hecho(|| {
        let f = fuente(this, 1);
        let destino = std::mem::ManuallyDrop::new(Com(destino));
        let mut p = std::ptr::null_mut();
        let previa = Com::de(unsafe { f!(destino, 4, fn(*const GUID, *const GUID, *mut *mut c_void) -> i32)(destino.0, &IID_PREVIA_DXGI, &IID_PREVIA_DXGI, &mut p) }, p)?;
        f.estado.lock().ok()?.previa = Some(previa);
        (qi_fuente(f, &IID_COLECCION, salida) >= 0).then_some(())
    })
}

#[repr(C)]
#[derive(Default)]
struct Descripcion {
    ancho: f32,
    alto: f32,
    imprimible: [f32; 4],
    ppp_x: u32,
    ppp_y: u32,
}

/// El papel de las opciones de imprimir, en DIP (1/96 de pulgada).
fn papel(opciones: *mut c_void) -> Option<(f32, f32)> {
    let o = std::mem::ManuallyDrop::new(Com(opciones));
    let core = o.pedir(&IID_OPCIONES)?;
    let mut d = Descripcion::default();
    (unsafe { f!(core, 6, fn(u32, *mut Descripcion) -> i32)(core.0, 1, &mut d) } >= 0 && d.ancho > 0.0 && d.alto > 0.0).then_some((d.ancho, d.alto))
}

/// IPrintPreviewPageCollection::Paginate: las páginas para este papel (otras si se ha girado la hoja).
unsafe extern "system" fn paginar(this: *mut c_void, _: u32, opciones: *mut c_void) -> i32 {
    hecho(|| {
        let f = fuente(this, 2);
        let papel = papel(opciones)?;
        let paginas = paginas(papel)?;
        let n = paginas.len() as u32;
        let mut e = f.estado.lock().ok()?;
        e.papel = papel;
        e.paginas = Some(paginas);
        let previa = e.previa.as_ref()?;
        (unsafe { f!(previa, 3, fn(u32, u32) -> i32)(previa.0, 0, n) } >= 0).then_some(())
    })
}

/// IPrintPreviewPageCollection::MakePage: una página de la vista previa, pintada por Direct2D en una textura.
unsafe extern "system" fn pagina(this: *mut c_void, numero: u32, ancho: f32, alto: f32) -> i32 {
    hecho(|| {
        let f = fuente(this, 2);
        let mut e = f.estado.lock().ok()?;
        let n = if numero == u32::MAX { 1 } else { numero } as usize;
        let emf = e.paginas.as_ref()?.get(n.checked_sub(1)?)?.clone();
        let papel = e.papel;
        graficos(&mut e)?;
        let g = e.graficos.as_ref()?;
        let superficie = pintar_textura(g, &emf, papel, ancho, alto)?.1;
        let previa = e.previa.as_ref()?;
        (unsafe { f!(previa, 4, fn(u32, *mut c_void, f32, f32) -> i32)(previa.0, n as u32, superficie.0, PPP_PREVIA, PPP_PREVIA) } >= 0).then_some(())
    })
}

/// IPrintDocumentPageSource::MakeDocument: lo que se imprime, con Direct2D, página a página.
unsafe extern "system" fn documento(this: *mut c_void, opciones: *mut c_void, destino: *mut c_void) -> i32 {
    hecho(|| {
        let f = fuente(this, 1);
        let papel = papel(opciones)?;
        let mut e = f.estado.lock().ok()?;
        if e.papel != papel || e.paginas.is_none() {
            drop(e);
            let p = paginas(papel)?;
            e = f.estado.lock().ok()?;
            e.papel = papel;
            e.paginas = Some(p);
        }
        let paginas = elegir(opciones, &e.paginas.clone()?);
        graficos(&mut e)?;
        imprimir_documento(e.graficos.as_ref()?, destino, &paginas, papel, 600.0)
    })
}

// --- Direct2D ---

/// Lo de Direct2D, la primera vez que hace falta.
fn graficos(e: &mut Estado) -> Option<()> {
    if e.graficos.is_none() {
        e.graficos = Some(crear_graficos()?);
    }
    Some(())
}

fn crear_graficos() -> Option<Graficos> {
    type Crear3d = unsafe extern "system" fn(*mut c_void, u32, *mut c_void, u32, *const u32, u32, u32, *mut *mut c_void, *mut u32, *mut *mut c_void) -> i32;
    type Crear2d = unsafe extern "system" fn(u32, *const GUID, *const c_void, *mut *mut c_void) -> i32;
    let crear3d: Crear3d = unsafe { std::mem::transmute(funcion("d3d11.dll", "D3D11CreateDevice")?) };
    let crear2d: Crear2d = unsafe { std::mem::transmute(funcion("d2d1.dll", "D2D1CreateFactory")?) };
    // Con la tarjeta gráfica y, si no se puede, por programa (WARP). BGRA, para Direct2D.
    let d3d = [1u32, 5].iter().find_map(|&tipo| {
        let mut p = std::ptr::null_mut();
        Com::de(unsafe { crear3d(std::ptr::null_mut(), tipo, std::ptr::null_mut(), 0x20, std::ptr::null(), 0, 7, &mut p, std::ptr::null_mut(), std::ptr::null_mut()) }, p)
    })?;
    let mut p = std::ptr::null_mut();
    let fabrica = Com::de(unsafe { crear2d(1, &IID_D2D_FACTORY1, std::ptr::null(), &mut p) }, p)?; // multihilo
    let dxgi = d3d.pedir(&IID_DXGI_DEVICE)?;
    let mut p = std::ptr::null_mut();
    let dispositivo = Com::de(unsafe { f!(fabrica, 17, fn(*mut c_void, *mut *mut c_void) -> i32)(fabrica.0, dxgi.0, &mut p) }, p)?;
    let mut p = std::ptr::null_mut();
    let contexto = Com::de(unsafe { f!(dispositivo, 4, fn(u32, *mut *mut c_void) -> i32)(dispositivo.0, 0, &mut p) }, p)?;
    type CrearLetras = unsafe extern "system" fn(u32, *const GUID, *mut *mut c_void) -> i32;
    let crear_letras: CrearLetras = unsafe { std::mem::transmute(funcion("dwrite.dll", "DWriteCreateFactory")?) };
    let mut p = std::ptr::null_mut();
    let gdi = Com::de(unsafe { crear_letras(0, &IID_DWRITE, &mut p) }, p)?.dar(17)?;
    Some(Graficos { d3d, fabrica, dispositivo, contexto, gdi, letras: RefCell::new(Vec::new()) })
}

/// Un metarchivo para Direct2D, a partir de sus bytes.
fn metarchivo(g: &Graficos, emf: &[u8]) -> Option<Com> {
    let mut flujo = std::ptr::null_mut();
    unsafe { CreateStreamOnHGlobal(std::ptr::null_mut(), 1, &mut flujo) };
    let flujo = Com::de(0, flujo)?;
    let escribir = f!(flujo, 4, fn(*const c_void, u32, *mut u32) -> i32);
    let mut n = 0;
    unsafe { escribir(flujo.0, emf.as_ptr() as *const c_void, emf.len() as u32, &mut n) };
    let ir = f!(flujo, 5, fn(i64, u32, *mut u64) -> i32); // Seek al principio
    unsafe { ir(flujo.0, 0, 0, std::ptr::null_mut()) };
    let mut p = std::ptr::null_mut();
    Com::de(unsafe { f!(g.fabrica, 21, fn(*mut c_void, *mut *mut c_void) -> i32)(g.fabrica.0, flujo.0, &mut p) }, p)
}

#[repr(C)]
struct Matriz([f32; 6]);

/// La página, a escala para que la hoja ocupe `ancho` × `alto` DIP, sobre blanco.
fn dibujar(g: &Graficos, emf: &[u8], papel: (f32, f32), ancho: f32, blanco: bool) -> Option<()> {
    // Orden a orden, para que el texto siga siendo texto; si la página trae algo más, el metarchivo entero.
    let ordenes = hoja::leer(g, emf);
    let mf = if ordenes.is_none() { Some(metarchivo(g, emf)?) } else { None };
    let c = &g.contexto;
    unsafe {
        f!(c, 48, fn() -> ())(c.0);
        if blanco {
            f!(c, 47, fn(*const [f32; 4]) -> ())(c.0, &[1.0, 1.0, 1.0, 1.0]);
        }
        let k = ancho / papel.0;
        f!(c, 30, fn(*const Matriz) -> ())(c.0, &Matriz([k, 0.0, 0.0, k, 0.0, 0.0]));
        let pinceles = match (&ordenes, &mf) {
            (Some(o), _) => hoja::pintar(g, o),
            (_, Some(mf)) => {
                f!(c, 84, fn(*mut c_void, *const c_void) -> ())(c.0, mf.0, std::ptr::null());
                Some(Vec::new())
            }
            _ => None,
        };
        f!(c, 30, fn(*const Matriz) -> ())(c.0, &Matriz([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]));
        let fin = f!(c, 49, fn(*mut u64, *mut u64) -> i32)(c.0, std::ptr::null_mut(), std::ptr::null_mut()) >= 0;
        (fin && pinceles.is_some()).then_some(())
    }
}

#[repr(C)]
struct DescTextura {
    ancho: u32,
    alto: u32,
    mips: u32,
    capas: u32,
    formato: u32,
    muestras: [u32; 2],
    uso: u32,
    enlaces: u32,
    cpu: u32,
    otros: u32,
}

#[repr(C)]
struct PropsBitmap {
    formato: u32,
    alfa: u32,
    ppp_x: f32,
    ppp_y: f32,
    opciones: u32,
    color: *mut c_void,
}

/// La página en una textura de `ancho` × `alto` DIP a `PPP_PREVIA`: la textura y su superficie DXGI.
fn pintar_textura(g: &Graficos, emf: &[u8], papel: (f32, f32), ancho: f32, alto: f32) -> Option<(Com, Com)> {
    let px = |x: f32| (x * PPP_PREVIA / 96.0).ceil().max(1.0) as u32;
    let d = DescTextura { ancho: px(ancho), alto: px(alto), mips: 1, capas: 1, formato: 87, muestras: [1, 0], uso: 0, enlaces: 0x20 | 0x8, cpu: 0, otros: 0 };
    let mut p = std::ptr::null_mut();
    let textura = Com::de(unsafe { f!(g.d3d, 5, fn(*const DescTextura, *const c_void, *mut *mut c_void) -> i32)(g.d3d.0, &d, std::ptr::null(), &mut p) }, p)?;
    let superficie = textura.pedir(&IID_DXGI_SURFACE)?;
    let props = PropsBitmap { formato: 87, alfa: 1, ppp_x: PPP_PREVIA, ppp_y: PPP_PREVIA, opciones: 1 | 2, color: std::ptr::null_mut() };
    let c = &g.contexto;
    let mut p = std::ptr::null_mut();
    let destino = Com::de(unsafe { f!(c, 62, fn(*mut c_void, *const PropsBitmap, *mut *mut c_void) -> i32)(c.0, superficie.0, &props, &mut p) }, p)?;
    unsafe {
        f!(c, 74, fn(*mut c_void) -> ())(c.0, destino.0);
        f!(c, 51, fn(f32, f32) -> ())(c.0, PPP_PREVIA, PPP_PREVIA);
    }
    let r = dibujar(g, emf, papel, ancho, true);
    unsafe { f!(c, 74, fn(*mut c_void) -> ())(c.0, std::ptr::null_mut()) };
    r.map(|_| (textura, superficie))
}

#[repr(C)]
struct Tamano(f32, f32);

#[repr(C)]
struct PropsImpresion {
    letras: u32,
    ppp: f32,
    color: u32,
}

/// Todas las páginas al trabajo de impresión: cada una en una lista de órdenes de Direct2D.
fn imprimir_documento(g: &Graficos, destino: *mut c_void, paginas: &[Vec<u8>], papel: (f32, f32), ppp: f32) -> Option<()> {
    let wic = Com::crear(&CLSID_WIC, &IID_WIC)?;
    let props = PropsImpresion { letras: 0, ppp, color: 1 };
    let mut p = std::ptr::null_mut();
    let control = Com::de(unsafe { f!(g.dispositivo, 5, fn(*mut c_void, *mut c_void, *const PropsImpresion, *mut *mut c_void) -> i32)(g.dispositivo.0, wic.0, destino, &props, &mut p) }, p)?;
    let c = &g.contexto;
    let mut bien = true;
    for emf in paginas {
        let mut p = std::ptr::null_mut();
        let Some(lista) = Com::de(unsafe { f!(c, 67, fn(*mut *mut c_void) -> i32)(c.0, &mut p) }, p) else {
            bien = false;
            break;
        };
        unsafe { f!(c, 74, fn(*mut c_void) -> ())(c.0, lista.0) };
        let dibujada = dibujar(g, emf, papel, papel.0, false);
        unsafe { f!(c, 74, fn(*mut c_void) -> ())(c.0, std::ptr::null_mut()) };
        let cerrada = unsafe { f!(lista, 5, fn() -> i32)(lista.0) } >= 0;
        if dibujada.is_none() || !cerrada || unsafe { f!(control, 3, fn(*mut c_void, Tamano, *mut c_void, *mut c_void, *mut c_void) -> i32)(control.0, lista.0, Tamano(papel.0, papel.1), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut()) } < 0 {
            bien = false;
            break;
        }
    }
    // Siempre: si no, el trabajo se queda abierto.
    let cerrado = unsafe { f!(control, 4, fn() -> i32)(control.0) } >= 0;
    (bien && cerrado).then_some(())
}

// --- Las páginas, compuestas en el hilo de la ventana (RichEdit es suyo) ---

struct Encargo {
    papel: (f32, f32),
    hecho: Mutex<Option<Arc<Vec<Vec<u8>>>>>,
    evento: HANDLE,
}

unsafe impl Send for Encargo {}
unsafe impl Sync for Encargo {}

impl Drop for Encargo {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.evento) };
    }
}

thread_local! {
    /// Los encargos que llegaron con la ventana ocupada (a medias de una edición): se hacen en cuanto acabe.
    static ESPERANDO: RefCell<Vec<Arc<Encargo>>> = const { RefCell::new(Vec::new()) };
}

/// Las páginas para este papel. Desde otro hilo, se piden a la ventana y se esperan (como mucho un minuto).
fn paginas(papel: (f32, f32)) -> Option<Arc<Vec<Vec<u8>>>> {
    if unsafe { GetCurrentThreadId() } == HILO_VENTANA.load(Ordering::SeqCst) {
        return Some(Arc::new(componer_aqui(papel)));
    }
    let e = Arc::new(Encargo { papel, hecho: Mutex::new(None), evento: unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) } });
    let suyo = Arc::into_raw(e.clone());
    if unsafe { PostMessageW(VENTANA.load(Ordering::SeqCst) as HWND, WM_PAGINAR, 0, suyo as isize) } == 0 {
        drop(unsafe { Arc::from_raw(suyo) });
        return None;
    }
    unsafe { WaitForSingleObject(e.evento, 60_000) };
    e.hecho.lock().ok()?.clone()
}

/// WM_PAGINAR, en la ventana.
pub fn paginar_aqui(lp: LPARAM) {
    let e = unsafe { Arc::from_raw(lp as *const Encargo) };
    ESPERANDO.with(|x| x.borrow_mut().push(e));
    reintentar();
}

/// Los encargos pendientes, si la ventana ya no está a medias de algo; si lo está, otra vez dentro de poco.
pub fn reintentar() {
    let h = VENTANA.load(Ordering::SeqCst) as HWND;
    if ventana::ocupada() {
        unsafe { SetTimer(h, RELOJ, 50, None) };
        return;
    }
    unsafe { KillTimer(h, RELOJ) };
    for e in ESPERANDO.with(|x| std::mem::take(&mut *x.borrow_mut())) {
        let p = Arc::new(componer_aqui(e.papel));
        if let Ok(mut h) = e.hecho.lock() {
            *h = Some(p);
        }
        unsafe { SetEvent(e.evento) };
    }
}

/// Un contexto de dispositivo de información de la impresora PDF de Windows (600 ppp), de la predeterminada o, si no
/// hay ninguna, de la pantalla: con él se miden las letras como al imprimir.
fn referencia() -> HDC {
    let pdf = ws("Microsoft Print to PDF");
    let ic = unsafe { CreateICW(std::ptr::null(), pdf.as_ptr(), std::ptr::null(), std::ptr::null()) };
    if !ic.is_null() {
        return ic;
    }
    let pred = (|| {
        let f: unsafe extern "system" fn(*mut u16, *mut u32) -> i32 = unsafe { std::mem::transmute(funcion("winspool.drv", "GetDefaultPrinterW")?) };
        let mut b = [0u16; 512];
        let mut n = b.len() as u32;
        (unsafe { f(b.as_mut_ptr(), &mut n) } != 0).then_some(b)
    })();
    let ic = pred.map_or(std::ptr::null_mut(), |b| unsafe { CreateICW(std::ptr::null(), b.as_ptr(), std::ptr::null(), std::ptr::null()) });
    if !ic.is_null() {
        return ic;
    }
    unsafe { CreateICW(ws("DISPLAY").as_ptr(), std::ptr::null(), std::ptr::null(), std::ptr::null()) }
}

/// Las páginas medidas con la impresora de referencia.
fn componer_aqui(papel: (f32, f32)) -> Vec<Vec<u8>> {
    let ic = referencia();
    let r = componer(papel, ic);
    unsafe { DeleteDC(ic) };
    r
}

/// Cada página en un metarchivo, medida con `ic`: los fondos de código y de cita, el texto con EM_FORMATRANGE (2 cm de
/// margen), las barras de cita y las tablas, como en pantalla, y el pie «Página n». `papel`, en DIP.
fn componer(papel: (f32, f32), ic: HDC) -> Vec<Vec<u8>> {
    let ed = app().ed.get();
    let mut r = Vec::new();
    unsafe {
        let (px, py) = (GetDeviceCaps(ic, LOGPIXELSX as i32), GetDeviceCaps(ic, LOGPIXELSY as i32));
        // 1 DIP = 1/96 de pulgada = 15 twips.
        let pagina = RECT { left: 0, top: 0, right: (papel.0 * 15.0) as i32, bottom: (papel.1 * 15.0) as i32 };
        let margen = 1134; // 2 cm
        let zona = RECT { left: margen, top: margen, right: pagina.right - margen, bottom: pagina.bottom - margen };
        // El marco del metarchivo, en centésimas de milímetro: así Direct2D lo coloca por la hoja y no por lo dibujado.
        let marco = RECT { left: 0, top: 0, right: (papel.0 * 2540.0 / 96.0) as i32, bottom: (papel.1 * 2540.0 / 96.0) as i32 };
        let (ancho_px, alto_px) = ((papel.0 * px as f32 / 96.0) as i32, (papel.1 * py as f32 / 96.0) as i32);
        // De twips a puntos de la impresora.
        let x = |tw: i32| tw * px / 1440;
        let y = |tw: i32| tw * py / 1440;
        enviar(ed, WM_SETREDRAW, 0, 0);
        ventana::para_imprimir(Some((py, y(zona.right - zona.left))));
        let _vuelta = editor::AlSalir(|| {
            ventana::para_imprimir(None);
            enviar(ed, WM_SETREDRAW, 1, 0);
            InvalidateRect(ed, std::ptr::null(), 1);
        });
        let colores = tema::colores_md(false);
        let fuente = CreateFontW(-(9 * py / 72), 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 0, 0, ws("Segoe UI").as_ptr());
        let total = editor::largo(ed) as i32;
        let (mut desde, mut n) = (0, 1);
        loop {
            // Lo que cabe en la hoja y, de cada posición, dónde empieza su línea: el pie de lo de antes, medido.
            let medir = |hasta: i32| {
                let mut fr = FORMATRANGE { hdc: ic, hdcTarget: ic, rc: zona, rcPage: pagina, chrg: CHARRANGE { cpMin: desde, cpMax: hasta } };
                let s = enviar(ed, EM_FORMATRANGE, 0, &mut fr as *mut _ as isize) as i32;
                (s, fr.rc.bottom)
            };
            let (siguiente, pie) = medir(-1);
            let arriba = |p: usize| match p as i32 {
                p if p <= desde => y(zona.top),
                p if p >= siguiente => y(pie),
                p => y(medir(p).1),
            };
            let dc = CreateEnhMetaFileW(ic, std::ptr::null(), &marco, std::ptr::null());
            if dc.is_null() {
                break;
            }
            let (encima, tablas) = ventana::para_hoja(desde as usize, (siguiente.max(desde + 1) - 1) as usize);
            let (izquierda, derecha) = (x(zona.left), x(zona.right));
            let rellenar = |r: RECT, color: u32| {
                let p = CreateSolidBrush(color);
                FillRect(dc, &r, p);
                DeleteObject(p as _);
            };
            for &(l, cierre) in &encima.codigo {
                let abajo = cierre.map_or(y(pie), |(_, f)| arriba(f + 1));
                rellenar(RECT { left: izquierda, top: arriba(l), right: derecha, bottom: abajo }, colores.fondo_codigo);
            }
            let mut barras = Vec::new();
            for &(l, _, f, nivel) in &encima.citas {
                let (de, a) = (arriba(l), arriba(f + 1));
                rellenar(RECT { left: izquierda + x(md::MARGEN_CITA), top: de, right: derecha - x(md::MARGEN_CITA), bottom: a }, colores.fondo_cita);
                for k in 0..nivel.min(8) as i32 {
                    let i = izquierda + x(k * md::CITA + md::MARGEN_CITA);
                    barras.push(RECT { left: i, top: de, right: i + x(md::BARRA_CITA).max(1), bottom: a });
                }
            }
            let guardado = SaveDC(dc);
            let mut fr = FORMATRANGE { hdc: dc, hdcTarget: ic, rc: zona, rcPage: pagina, chrg: CHARRANGE { cpMin: desde, cpMax: -1 } };
            enviar(ed, EM_FORMATRANGE, 1, &mut fr as *mut _ as isize);
            RestoreDC(dc, guardado);
            for b in barras {
                rellenar(b, colores.barra_cita);
            }
            // De cada tabla, las filas que empiezan en esta hoja.
            for (filas, dis) in &tablas {
                let en_hoja: Vec<usize> = (0..filas.len()).filter(|&k| (desde..siguiente).contains(&(filas[k] as i32))).collect();
                if let (Some(&k0), Some(&k1)) = (en_hoja.first(), en_hoja.last()) {
                    tabla::dibujar(dc, dis, izquierda, arriba(filas[k0]), &colores, false, Some(k0..k1 + 1));
                }
            }
            let antes = SelectObject(dc, fuente as _);
            let numero = ws(&con(t().pagina, n));
            let mut rp = RECT { left: 0, top: alto_px - py * 2 / 3, right: ancho_px, bottom: alto_px - py / 3 };
            SetBkMode(dc, TRANSPARENT as i32);
            DrawTextW(dc, numero.as_ptr(), (numero.len() - 1) as i32, &mut rp, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
            SelectObject(dc, antes);
            let emf = CloseEnhMetaFile(dc);
            let largo = GetEnhMetaFileBits(emf, 0, std::ptr::null_mut());
            let mut b = vec![0u8; largo as usize];
            GetEnhMetaFileBits(emf, largo, b.as_mut_ptr());
            DeleteEnhMetaFile(emf);
            r.push(b);
            // Una línea que no cabe en la hoja (una fila de tabla enorme) sale cortada y se sigue en la siguiente: sin
            // esto, el resto del documento no se imprimiría. Nunca un bucle sin fin: siempre se avanza o se acaba.
            let avance = if siguiente > desde {
                siguiente
            } else {
                let l = enviar(ed, windows_sys::Win32::UI::Controls::EM_LINEFROMCHAR, desde as usize, 0);
                enviar(ed, windows_sys::Win32::UI::Controls::EM_LINEINDEX, (l + 1) as usize, 0) as i32
            };
            if avance <= desde || avance >= total {
                break;
            }
            desde = avance;
            n += 1;
        }
        enviar(ed, EM_FORMATRANGE, 0, 0);
        DeleteObject(fuente as _);
    }
    r
}

/// Las páginas, a la impresora de `dc` por GDI, cada una a la hoja entera (contando lo que la impresora no alcanza).
fn por_gdi(dc: HDC, paginas: &[Vec<u8>], nombre: &str, salida: Option<&str>) -> bool {
    let m = |i: u32| unsafe { GetDeviceCaps(dc, i as i32) };
    let hoja = RECT { left: -m(PHYSICALOFFSETX), top: -m(PHYSICALOFFSETY), right: m(PHYSICALWIDTH) - m(PHYSICALOFFSETX), bottom: m(PHYSICALHEIGHT) - m(PHYSICALOFFSETY) };
    let (nombre, salida) = (ws(nombre), salida.map(ws));
    let di = DOCINFOW {
        cbSize: std::mem::size_of::<DOCINFOW>() as i32,
        lpszDocName: nombre.as_ptr(),
        lpszOutput: salida.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
        lpszDatatype: std::ptr::null(),
        fwType: 0,
    };
    if unsafe { StartDocW(dc, &di) } <= 0 {
        return false;
    }
    let mut bien = true;
    for b in paginas {
        unsafe {
            let emf = SetEnhMetaFileBits(b.len() as u32, b.as_ptr());
            bien &= StartPage(dc) > 0 && PlayEnhMetaFile(dc, emf, &hoja) != 0 && EndPage(dc) > 0;
            DeleteEnhMetaFile(emf);
        }
    }
    unsafe { EndDoc(dc) > 0 && bien }
}

// --- El diálogo clásico: las mismas páginas, medidas con la impresora elegida ---

fn clasico() {
    let mut pd: PRINTDLGW = unsafe { std::mem::zeroed() };
    pd.lStructSize = std::mem::size_of::<PRINTDLGW>() as u32;
    pd.hwndOwner = app().hwnd.get();
    pd.Flags = PD_RETURNDC | PD_NOSELECTION | PD_NOPAGENUMS | PD_USEDEVMODECOPIESANDCOLLATE;
    if unsafe { PrintDlgW(&mut pd) } == 0 || pd.hDC.is_null() {
        return;
    }
    let dc = pd.hDC;
    unsafe {
        let m = |i: u32| GetDeviceCaps(dc, i as i32) as f32;
        let papel = (m(PHYSICALWIDTH) * 96.0 / m(LOGPIXELSX).max(1.0), m(PHYSICALHEIGHT) * 96.0 / m(LOGPIXELSY).max(1.0));
        let paginas = componer(papel, dc);
        por_gdi(dc, &paginas, &ventana::titulo_documento(), None);
        DeleteDC(dc);
        if !pd.hDevMode.is_null() {
            GlobalFree(pd.hDevMode);
        }
        if !pd.hDevNames.is_null() {
            GlobalFree(pd.hDevNames);
        }
    }
}

// --- La prueba: el mismo camino que la ventana de Windows, sin ella (BLOC_IMPRIMIR=<carpeta>) ---

/// Desde otro hilo, como haría Windows: las páginas en A4 vertical y apaisado (`pagina-N.emf`), la primera de la vista
/// previa pintada por Direct2D (`previa-1.bmp`), el trabajo entero en XPS (`trabajo.oxps`) y `fin.txt` con lo hecho.
fn prueba(dir: std::path::PathBuf) {
    unsafe { CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED as u32) };
    let mut informe = String::new();
    let a4 = (793.7008, 1122.5197);
    for (nombre, papel) in [("vertical", a4), ("apaisado", (a4.1, a4.0))] {
        let Some(p) = paginas(papel) else {
            informe += &format!("{nombre}: sin páginas\n");
            continue;
        };
        informe += &format!("{nombre}: {} páginas\n", p.len());
        if nombre == "vertical" {
            for (i, b) in p.iter().enumerate() {
                let _ = std::fs::write(dir.join(format!("pagina-{}.emf", i + 1)), b);
            }
            let mut e = Estado { papel, paginas: Some(p.clone()), ..Default::default() };
            let Some(g) = graficos(&mut e).and(e.graficos.as_ref()) else {
                informe += "sin Direct2D\n";
                continue;
            };
            informe += match previa_bmp(g, &p[0], papel, &dir.join("previa-1.bmp")) {
                Some(()) => "previa: bien\n",
                None => "previa: fallo\n",
            };
            let t0 = std::time::Instant::now();
            informe += match trabajo_xps(g, &p, papel, &dir.join("trabajo.oxps")) {
                Some(()) => "trabajo: bien\n",
                None => "trabajo: fallo\n",
            };
            informe += &format!("trabajo: {} ms\n", t0.elapsed().as_millis());
            let t0 = std::time::Instant::now();
            informe += match pdf_gdi(&p, &dir.join("gdi.pdf")) {
                Some(()) => "gdi: bien\n",
                None => "gdi: fallo\n",
            };
            informe += &format!("gdi: {} ms\n", t0.elapsed().as_millis());
        }
    }
    let _ = std::fs::write(dir.join("fin.txt"), informe);
}

/// Como MakePage, y la textura leída de vuelta para guardarla.
fn previa_bmp(g: &Graficos, emf: &[u8], papel: (f32, f32), ruta: &std::path::Path) -> Option<()> {
    let (ancho, alto) = (papel.0 / 2.0, papel.1 / 2.0);
    let (textura, _) = pintar_textura(g, emf, papel, ancho, alto)?;
    let px = |x: f32| (x * PPP_PREVIA / 96.0).ceil().max(1.0) as u32;
    let (w, h) = (px(ancho), px(alto));
    let d = DescTextura { ancho: w, alto: h, mips: 1, capas: 1, formato: 87, muestras: [1, 0], uso: 3, enlaces: 0, cpu: 0x20000, otros: 0 };
    let mut p = std::ptr::null_mut();
    let copia = Com::de(unsafe { f!(g.d3d, 5, fn(*const DescTextura, *const c_void, *mut *mut c_void) -> i32)(g.d3d.0, &d, std::ptr::null(), &mut p) }, p)?;
    let mut c = std::ptr::null_mut();
    unsafe { f!(g.d3d, 40, fn(*mut *mut c_void) -> ())(g.d3d.0, &mut c) };
    let ctx = Com::de(0, c)?;
    #[repr(C)]
    struct Mapa {
        datos: *const u8,
        fila: u32,
        capa: u32,
    }
    let mut m = Mapa { datos: std::ptr::null(), fila: 0, capa: 0 };
    let mut bmp = Vec::new();
    unsafe {
        f!(ctx, 47, fn(*mut c_void, *mut c_void) -> ())(ctx.0, copia.0, textura.0);
        if f!(ctx, 14, fn(*mut c_void, u32, u32, u32, *mut Mapa) -> i32)(ctx.0, copia.0, 0, 1, 0, &mut m) < 0 {
            return None;
        }
        let fila = w as usize * 4;
        bmp.extend(b"BM");
        bmp.extend(((54 + fila * h as usize) as u32).to_le_bytes());
        bmp.extend([0u8; 4]);
        bmp.extend(54u32.to_le_bytes());
        bmp.extend(40u32.to_le_bytes());
        bmp.extend((w as i32).to_le_bytes());
        bmp.extend((-(h as i32)).to_le_bytes());
        bmp.extend(1u16.to_le_bytes());
        bmp.extend(32u16.to_le_bytes());
        bmp.extend([0u8; 24]);
        for y in 0..h as usize {
            bmp.extend_from_slice(std::slice::from_raw_parts(m.datos.add(y * m.fila as usize), fila));
        }
        f!(ctx, 15, fn(*mut c_void, u32) -> ())(ctx.0, copia.0, 0);
    }
    std::fs::write(ruta, bmp).ok()
}

/// Las mismas páginas impresas por GDI a «Microsoft Print to PDF», como el diálogo clásico: con qué comparar.
fn pdf_gdi(paginas: &[Vec<u8>], ruta: &std::path::Path) -> Option<()> {
    let dc = unsafe { CreateDCW(std::ptr::null(), ws("Microsoft Print to PDF").as_ptr(), std::ptr::null(), std::ptr::null()) };
    if dc.is_null() {
        return None;
    }
    let bien = por_gdi(dc, paginas, "Bloc", Some(&ruta.to_string_lossy()));
    unsafe { DeleteDC(dc) };
    bien.then_some(())
}

/// Como MakeDocument, a un archivo XPS en vez de a la impresora.
fn trabajo_xps(g: &Graficos, paginas: &[Vec<u8>], papel: (f32, f32), ruta: &std::path::Path) -> Option<()> {
    let fabrica = Com::crear(&CLSID_FABRICA_TRABAJOS, &IID_FABRICA_TRABAJOS)?;
    let mut flujo = std::ptr::null_mut();
    unsafe { CreateStreamOnHGlobal(std::ptr::null_mut(), 1, &mut flujo) };
    let flujo = Com::de(0, flujo)?;
    let impresora = ws("Microsoft Print to PDF");
    let trabajo = ws("Bloc");
    let mut p = std::ptr::null_mut();
    let destino = Com::de(
        unsafe { f!(fabrica, 3, fn(*const u16, *const u16, *mut c_void, *mut c_void, *mut *mut c_void) -> i32)(fabrica.0, impresora.as_ptr(), trabajo.as_ptr(), flujo.0, std::ptr::null_mut(), &mut p) },
        p,
    )?;
    imprimir_documento(g, destino.0, paginas, papel, 600.0)?;
    drop(destino);
    let mut memoria = std::ptr::null_mut();
    unsafe { GetHGlobalFromStream(flujo.0, &mut memoria) };
    let datos = unsafe {
        let p = GlobalLock(memoria) as *const u8;
        let v = std::slice::from_raw_parts(p, GlobalSize(memoria)).to_vec();
        GlobalUnlock(memoria);
        v
    };
    std::fs::write(ruta, datos).ok()
}

#[cfg(test)]
mod pruebas {
    use super::del_intervalo;

    #[test]
    fn intervalos() {
        assert_eq!(del_intervalo(4, &[]), vec![0, 1, 2, 3], "sin intervalo, todas");
        assert_eq!(del_intervalo(9, &[(1, 3), (5, 5)]), vec![0, 1, 2, 4], "1-3, 5");
        assert_eq!(del_intervalo(4, &[(3, 9)]), vec![2, 3], "las que hay");
        assert_eq!(del_intervalo(4, &[(2, 2), (1, 1)]), vec![1, 0], "en su orden");
    }
}
