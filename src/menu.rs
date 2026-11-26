// «Editar con Bloc» en el menú corto del botón derecho de Windows 11: una IExplorerCommand que sirve «Bloc.exe -Menu».
// El Explorador solo la pide si está registrado el paquete con ubicación externa de Bloc (recursos\Bloc.msix, firmado
// con el certificado propio de Bloc): ese paquete le dice qué extensiones la llevan y con qué orden se abre el
// servidor. Sin el paquete no se usa, y queda el «Editar con Bloc» del menú clásico (instalar::poner_editar).
use std::cell::Cell;
use std::ffi::c_void;

use windows_sys::core::GUID;
use windows_sys::Win32::System::Com::{CoInitializeEx, CoRegisterClassObject, CoRevokeClassObject, CoTaskMemAlloc, CoTaskMemFree, CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED, REGCLS_MULTIPLEUSE};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::com::Com;
use crate::textos::t;

pub const CLSID_EDITAR: GUID = GUID::from_u128(0x9CE57C74_B375_4377_B6F5_E23308B5B0D5);
const IID_IUNKNOWN: GUID = GUID::from_u128(0x00000000_0000_0000_C000_000000000046);
const IID_FABRICA: GUID = GUID::from_u128(0x00000001_0000_0000_C000_000000000046);
const IID_ORDEN: GUID = GUID::from_u128(0xA08CE4D0_FA25_44AB_B57C_C7B1C323E0B9);
const E_NOINTERFACE: i32 = 0x80004002_u32 as i32;
const E_NOTIMPL: i32 = 0x80004001_u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007000E_u32 as i32;
const SIN_AGREGAR: i32 = 0x80040110_u32 as i32; // CLASS_E_NOAGGREGATION
const SIGDN_FILESYSPATH: u32 = 0x80058000;
const ECS_HIDDEN: u32 = 2;
/// Lo que espera el proceso a otro clic derecho antes de acabar: abrir uno nuevo en cada menú lo haría más lento.
const ESPERA: u32 = 30_000;

thread_local!(static VIVAS: Cell<u32> = const { Cell::new(0) });
thread_local!(static RELOJ: Cell<usize> = const { Cell::new(0) });

pub fn servir() {
    let mut ficha = 0;
    unsafe {
        CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
        CoRegisterClassObject(&CLSID_EDITAR, &FABRICA as *const Fabrica as *mut c_void, CLSCTX_LOCAL_SERVER, REGCLS_MULTIPLEUSE as u32, &mut ficha);
    }
    rearmar();
    let mut m: MSG = unsafe { std::mem::zeroed() };
    while unsafe { GetMessageW(&mut m, std::ptr::null_mut(), 0, 0) } > 0 {
        unsafe { DispatchMessageW(&m) };
    }
    unsafe { CoRevokeClassObject(ficha) };
}

fn rearmar() {
    let antes = RELOJ.with(Cell::get);
    unsafe {
        if antes != 0 {
            KillTimer(std::ptr::null_mut(), antes);
        }
        RELOJ.with(|r| r.set(SetTimer(std::ptr::null_mut(), 0, ESPERA, Some(sin_pedir))));
    }
}

unsafe extern "system" fn sin_pedir(_: windows_sys::Win32::Foundation::HWND, _: u32, _: usize, _: u32) {
    if VIVAS.with(Cell::get) == 0 {
        unsafe { PostQuitMessage(0) };
    } else {
        rearmar();
    }
}

fn igual(a: &GUID, b: &GUID) -> bool {
    a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
}

type Qi = unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> i32;
type Cuenta = unsafe extern "system" fn(*mut c_void) -> u32;
type Texto = unsafe extern "system" fn(*mut c_void, *mut c_void, *mut *mut u16) -> i32;

// --- La fábrica: estática, sin cuenta ---

#[repr(C)]
struct Fabrica {
    vt: &'static VtFabrica,
}

#[repr(C)]
struct VtFabrica {
    qi: Qi,
    mas: Cuenta,
    menos: Cuenta,
    crear: unsafe extern "system" fn(*mut c_void, *mut c_void, *const GUID, *mut *mut c_void) -> i32,
    bloquear: unsafe extern "system" fn(*mut c_void, i32) -> i32,
}

static FABRICA: Fabrica = Fabrica { vt: &VtFabrica { qi: qi_fabrica, mas: uno, menos: uno, crear, bloquear } };

unsafe extern "system" fn uno(_: *mut c_void) -> u32 {
    1
}

unsafe extern "system" fn qi_fabrica(this: *mut c_void, iid: *const GUID, salida: *mut *mut c_void) -> i32 {
    let iid = unsafe { &*iid };
    let si = igual(iid, &IID_IUNKNOWN) || igual(iid, &IID_FABRICA);
    unsafe { *salida = if si { this } else { std::ptr::null_mut() } };
    if si { 0 } else { E_NOINTERFACE }
}

unsafe extern "system" fn crear(_: *mut c_void, fuera: *mut c_void, iid: *const GUID, salida: *mut *mut c_void) -> i32 {
    unsafe { *salida = std::ptr::null_mut() };
    if !fuera.is_null() {
        return SIN_AGREGAR;
    }
    let o = Box::into_raw(Box::new(Orden { vt: &VT_ORDEN, cuenta: Cell::new(1) }));
    VIVAS.with(|v| v.set(v.get() + 1));
    rearmar();
    let r = unsafe { qi_orden(o.cast(), iid, salida) };
    unsafe { soltar(o.cast()) };
    r
}

unsafe extern "system" fn bloquear(_: *mut c_void, _: i32) -> i32 {
    0
}

// --- La orden ---

#[repr(C)]
struct Orden {
    vt: &'static VtOrden,
    cuenta: Cell<u32>,
}

#[repr(C)]
struct VtOrden {
    qi: Qi,
    mas: Cuenta,
    menos: Cuenta,
    titulo: Texto,
    icono: Texto,
    ayuda: Texto,
    nombre: unsafe extern "system" fn(*mut c_void, *mut GUID) -> i32,
    estado: unsafe extern "system" fn(*mut c_void, *mut c_void, i32, *mut u32) -> i32,
    invocar: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void) -> i32,
    banderas: unsafe extern "system" fn(*mut c_void, *mut u32) -> i32,
    subordenes: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32,
}

static VT_ORDEN: VtOrden = VtOrden { qi: qi_orden, mas, menos: soltar, titulo, icono, ayuda, nombre, estado, invocar, banderas, subordenes };

fn orden<'a>(this: *mut c_void) -> &'a Orden {
    unsafe { &*(this as *const Orden) }
}

unsafe extern "system" fn qi_orden(this: *mut c_void, iid: *const GUID, salida: *mut *mut c_void) -> i32 {
    let iid = unsafe { &*iid };
    let si = igual(iid, &IID_IUNKNOWN) || igual(iid, &IID_ORDEN);
    unsafe { *salida = if si { this } else { std::ptr::null_mut() } };
    if !si {
        return E_NOINTERFACE;
    }
    unsafe { mas(this) };
    0
}

unsafe extern "system" fn mas(this: *mut c_void) -> u32 {
    let o = orden(this);
    o.cuenta.set(o.cuenta.get() + 1);
    o.cuenta.get()
}

unsafe extern "system" fn soltar(this: *mut c_void) -> u32 {
    let o = orden(this);
    let n = o.cuenta.get() - 1;
    o.cuenta.set(n);
    if n == 0 {
        drop(unsafe { Box::from_raw(this as *mut Orden) });
        VIVAS.with(|v| v.set(v.get() - 1));
    }
    n
}

/// Un texto para el Explorador, que lo libera con CoTaskMemFree.
fn dar(s: &str, salida: *mut *mut u16) -> i32 {
    let w: Vec<u16> = s.encode_utf16().chain([0]).collect();
    let p = unsafe { CoTaskMemAlloc(w.len() * 2) } as *mut u16;
    unsafe { *salida = p };
    if p.is_null() {
        return E_OUTOFMEMORY;
    }
    unsafe { std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len()) };
    0
}

unsafe extern "system" fn titulo(_: *mut c_void, _: *mut c_void, salida: *mut *mut u16) -> i32 {
    dar(t().editar_con, salida)
}

unsafe extern "system" fn icono(_: *mut c_void, _: *mut c_void, salida: *mut *mut u16) -> i32 {
    let exe = std::env::current_exe().unwrap_or_default();
    dar(&format!("{},0", exe.display()), salida)
}

unsafe extern "system" fn ayuda(_: *mut c_void, _: *mut c_void, salida: *mut *mut u16) -> i32 {
    unsafe { *salida = std::ptr::null_mut() };
    E_NOTIMPL
}

unsafe extern "system" fn nombre(_: *mut c_void, g: *mut GUID) -> i32 {
    unsafe { *g = CLSID_EDITAR };
    0
}

/// Escondida si se ha quitado «Editar con Bloc» en la página de elegir, o en una extensión sin marcar.
unsafe extern "system" fn estado(_: *mut c_void, elementos: *mut c_void, _: i32, salida: *mut u32) -> i32 {
    let ext = rutas(elementos, 1).first().and_then(|r| std::path::Path::new(r).extension().map(|e| format!(".{}", e.to_string_lossy().to_lowercase())));
    unsafe { *salida = if crate::instalar::en_menu(ext.as_deref()) { 0 } else { ECS_HIDDEN } };
    0
}

unsafe extern "system" fn invocar(_: *mut c_void, elementos: *mut c_void, _: *mut c_void) -> i32 {
    if let Ok(exe) = std::env::current_exe() {
        // El Explorador ya le ha dado el primer plano a este proceso: que pase a la ventana nueva.
        unsafe { AllowSetForegroundWindow(u32::MAX) };
        // Bloc abre un archivo por ventana; como el «Editar» clásico, hasta 15.
        for r in rutas(elementos, 15) {
            let _ = std::process::Command::new(&exe).arg(r).spawn();
        }
    }
    0
}

unsafe extern "system" fn banderas(_: *mut c_void, salida: *mut u32) -> i32 {
    unsafe { *salida = 0 };
    0
}

unsafe extern "system" fn subordenes(_: *mut c_void, salida: *mut *mut c_void) -> i32 {
    unsafe { *salida = std::ptr::null_mut() };
    E_NOTIMPL
}

/// Las rutas de los archivos elegidos (IShellItemArray), hasta `cuantas`.
fn rutas(elementos: *mut c_void, cuantas: u32) -> Vec<std::ffi::OsString> {
    if elementos.is_null() {
        return Vec::new();
    }
    let tabla = unsafe { *(elementos as *const *const usize) };
    let metodo = |n: usize| unsafe { *tabla.add(n) };
    let contar: unsafe extern "system" fn(*mut c_void, *mut u32) -> i32 = unsafe { std::mem::transmute(metodo(7)) };
    let elemento: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> i32 = unsafe { std::mem::transmute(metodo(8)) };
    let mut n = 0;
    unsafe { contar(elementos, &mut n) };
    (0..n.min(cuantas))
        .filter_map(|i| {
            let mut p = std::ptr::null_mut();
            let e = Com::de(unsafe { elemento(elementos, i, &mut p) }, p)?;
            let nombre: unsafe extern "system" fn(*mut c_void, u32, *mut *mut u16) -> i32 = unsafe { std::mem::transmute(e.metodo(5)) };
            let mut s = std::ptr::null_mut();
            if unsafe { nombre(e.0, SIGDN_FILESYSPATH, &mut s) } < 0 || s.is_null() {
                return None;
            }
            let largo = (0..).take_while(|&k| unsafe { *s.add(k) } != 0).count();
            // Tal cual: un nombre con media letra suelta (válido en NTFS) se abre igual.
            let r = std::os::windows::ffi::OsStringExt::from_wide(unsafe { std::slice::from_raw_parts(s, largo) });
            unsafe { CoTaskMemFree(s.cast()) };
            Some(r)
        })
        .collect()
}
